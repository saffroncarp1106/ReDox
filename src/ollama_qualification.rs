use std::path::Path;
use std::time::Duration;

use serde_json::{Value, json};
use tokio::sync::{Mutex, mpsc, watch};

use super::{
    InputMessage, Mode, ProviderEvent, Request, api_url, client, is_rust_profile, send, state,
    stream_chat, stream_chat_inner,
};

#[path = "ollama_qualification/cases.rs"]
mod cases;
use cases::{REASONING_TRIALS, reasoning_trials, strict_pass};

#[cfg(test)]
#[path = "../tests/transport/ollama_qualification.rs"]
mod tests;

const PARSER_PROMPT: &str = include_str!("ollama_qualification/parser_prompt.txt");
const PARSER_HARNESS: &str = include_str!("ollama_qualification/parser_harness.rs");
const KEEP_WARM_SECONDS: i64 = 900;

#[derive(PartialEq)]
struct LoadKey {
    endpoint: url::Url,
    model: String,
    context: u32,
    output: u32,
    cpu_only: bool,
}

struct Qualified {
    key: LoadKey,
    resident: (String, Option<u64>),
    history: Vec<InputMessage>,
}

static QUALIFIED: Mutex<Option<Qualified>> = Mutex::const_new(None);

pub async fn stream_qualified_chat(
    mut request: Request,
    sender: mpsc::Sender<ProviderEvent>,
    mut cancel: watch::Receiver<bool>,
) -> Result<(), String> {
    if !is_rust_profile(&request.model) {
        return stream_chat(request, sender, cancel).await;
    }
    if *cancel.borrow() {
        return Err("Cancelled.".into());
    }

    let service_endpoint = tokio::select! {
        result = prepare_at_service(&request, &sender) => result?,
        _ = cancel.changed() => return Err("Cancelled.".into()),
    };
    if let Some(endpoint) = service_endpoint {
        request.settings.endpoint = endpoint;
        return stream_chat(request, sender, cancel).await;
    }

    let mut qualified = tokio::select! {
        result = prepare(&request, &sender, cancel.clone()) => result?,
        _ = cancel.changed() => return Err("Cancelled.".into()),
    };
    let mut history = qualified.as_ref().unwrap().history.clone();
    history.append(&mut request.messages);
    request.messages = history;
    request.settings.keep_alive = Some(KEEP_WARM_SECONDS);
    let mut compacted = false;
    let result = stream_chat_inner(request, sender, cancel, &mut compacted).await;
    if compacted {
        *qualified = None;
    }
    result
}

async fn prepare_at_service(
    request: &Request,
    sender: &mpsc::Sender<ProviderEvent>,
) -> Result<Option<String>, String> {
    let configured = &request.settings.endpoint;
    let api = api_url(configured, "ps")?;
    let default_local = api.scheme() == "http"
        && api.port() == Some(11434)
        && matches!(api.host_str(), Some("localhost" | "127.0.0.1"))
        && api.path() == "/api/ps";
    if default_local && qualify_at_endpoint("http://127.0.0.1:11435", request, sender).await? {
        return Ok(Some("http://127.0.0.1:11435".into()));
    }
    Ok(qualify_at_endpoint(configured, request, sender)
        .await?
        .then(|| configured.clone()))
}

async fn qualify_at_endpoint(
    endpoint: &str,
    request: &Request,
    sender: &mpsc::Sender<ProviderEvent>,
) -> Result<bool, String> {
    let api = api_url(endpoint, "ps")?;
    let health = api
        .join("../_redox/health")
        .map_err(|error| error.to_string())?;
    let service = reqwest::Client::builder()
        .no_proxy()
        .connect_timeout(Duration::from_secs(3))
        .build()
        .map_err(|error| error.to_string())?;
    let Ok(response) = service
        .get(health)
        .timeout(Duration::from_secs(2))
        .send()
        .await
    else {
        return Ok(false);
    };
    if !response.status().is_success()
        || response
            .json::<Value>()
            .await
            .ok()
            .is_none_or(|body| body["service"] != "redox-qualified-ollama")
    {
        return Ok(false);
    }

    status(sender, "Waiting for server qualification").await?;
    let qualify = api
        .join("../_redox/qualify")
        .map_err(|error| error.to_string())?;
    let mut options = json!({"num_ctx":request.settings.context_tokens});
    if request.settings.cpu_only {
        options["num_gpu"] = json!(0);
    }
    let response = service
        .post(qualify)
        .json(&json!({"model":request.model,"options":options}))
        .send()
        .await
        .map_err(|error| format!("Server qualification failed: {error}"))?;
    super::checked(response)
        .await
        .map_err(|error| error.message)?;
    Ok(true)
}

pub async fn qualify_model(
    settings: super::Settings,
    sender: mpsc::Sender<ProviderEvent>,
    mut cancel: watch::Receiver<bool>,
) -> Result<Vec<Value>, String> {
    if !is_rust_profile(&settings.preferred_model) {
        return Ok(Vec::new());
    }
    if *cancel.borrow() {
        return Err("Cancelled.".into());
    }
    let request = Request::for_task(settings, String::new(), None, Mode::Chat);
    let qualified = tokio::select! {
        result = prepare(&request, &sender, cancel.clone()) => result?,
        _ = cancel.changed() => return Err("Cancelled.".into()),
    };
    Ok(qualified
        .as_ref()
        .unwrap()
        .history
        .iter()
        .map(|message| json!({"role":message.role,"content":message.content}))
        .collect())
}

async fn prepare(
    request: &Request,
    sender: &mpsc::Sender<ProviderEvent>,
    cancel: watch::Receiver<bool>,
) -> Result<tokio::sync::MutexGuard<'static, Option<Qualified>>, String> {
    let client = client()?;
    let mut qualified = QUALIFIED.lock().await;
    let key = LoadKey {
        endpoint: api_url(&request.settings.endpoint, "ps")?,
        model: request.model.to_ascii_lowercase(),
        context: request.settings.context_tokens,
        output: request.settings.output_tokens,
        cpu_only: request.settings.cpu_only,
    };
    let already_ready = if let Some(previous) = qualified.as_ref().filter(|saved| saved.key == key)
    {
        resident(&client, &key).await?.as_ref() == Some(&previous.resident)
    } else {
        false
    };
    if !already_ready {
        *qualified = None;
        let mut attempt = 0;
        loop {
            attempt += 1;
            status(sender, format!("Attempt {attempt} · loading fresh weights")).await?;
            unload(&client, request).await?;
            if let Some(history) = qualify(request, sender, cancel.clone(), attempt).await?
                && let Some(runner) = resident(&client, &key).await?
            {
                status(
                    sender,
                    "10/10 reasoning and 1/1 Rust passed · starting your prompt",
                )
                .await?;
                *qualified = Some(Qualified {
                    key,
                    resident: runner,
                    history,
                });
                break;
            }
        }
    }
    Ok(qualified)
}

async fn resident(
    client: &reqwest::Client,
    key: &LoadKey,
) -> Result<Option<(String, Option<u64>)>, String> {
    let response = client
        .get(key.endpoint.clone())
        .timeout(Duration::from_secs(15))
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(|error| format!("Cannot check Aisnley-Ox-1-bbb residency: {error}"))?;
    let data: Value = response
        .json()
        .await
        .map_err(|error| format!("Invalid Ollama running model list: {error}"))?;
    let models = data["models"]
        .as_array()
        .ok_or("Invalid Ollama running model list.")?;
    Ok(models.iter().find_map(|model| {
        let name = model["name"].as_str().or_else(|| model["model"].as_str())?;
        name.eq_ignore_ascii_case(&key.model).then_some(())?;
        Some((
            model["digest"].as_str()?.into(),
            model["context_length"].as_u64(),
        ))
    }))
}

async fn unload(client: &reqwest::Client, request: &Request) -> Result<(), String> {
    client
        .post(api_url(&request.settings.endpoint, "generate")?)
        .timeout(Duration::from_secs(60))
        .json(&json!({"model":request.model,"keep_alive":0,"stream":false}))
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(|error| format!("Cannot unload Aisnley-Ox-1-bbb for qualification: {error}"))?;
    Ok(())
}

async fn status(
    sender: &mpsc::Sender<ProviderEvent>,
    message: impl AsRef<str>,
) -> Result<(), String> {
    send(
        sender,
        ProviderEvent::Status(format!(
            "Aisnley-Ox-1-bbb qualification · {}",
            message.as_ref()
        )),
    )
    .await
}

async fn qualify(
    original: &Request,
    sender: &mpsc::Sender<ProviderEvent>,
    cancel: watch::Receiver<bool>,
    attempt: usize,
) -> Result<Option<Vec<InputMessage>>, String> {
    let id = uuid::Uuid::new_v4();
    let checkpoint = state::checkpoint_path(id).ok_or("Application data folder unavailable.")?;
    let workspace = checkpoint.with_file_name("qualification");
    tokio::fs::create_dir_all(&workspace)
        .await
        .map_err(|error| error.to_string())?;
    let mut settings = original.settings.clone();
    settings.preferred_model.clone_from(&original.model);
    settings.project_tools = true;
    settings.keep_alive = Some(KEEP_WARM_SECONDS);
    settings.output_tokens = 1024;
    settings.instructions = "Solve each benchmark task independently unless it asks you to recall a conversational value. When asked to read a fixture, call read_file before answering. Return the requested JSON in exactly one fenced JSON block.".into();
    let mut history = Vec::new();
    for (index, trial) in reasoning_trials(&format!("jam-{}", id.simple()))
        .into_iter()
        .enumerate()
    {
        let number = index + 1;
        status(
            sender,
            format!("Attempt {attempt} · reasoning {number}/{REASONING_TRIALS}"),
        )
        .await?;
        if trial.fixture {
            tokio::fs::write(workspace.join("fixture.json"), trial.expected.to_string())
                .await
                .map_err(|error| format!("Cannot write qualification fixture: {error}"))?;
        }
        history.push(message("user", trial.prompt));
        let reply = trial_reply(
            settings.clone(),
            &workspace,
            &checkpoint,
            &history,
            cancel.clone(),
        )
        .await?;
        if reply.as_ref().is_none_or(|(answer, read_file)| {
            !strict_pass(answer, &trial.expected) || (trial.fixture && !read_file)
        }) {
            status(
                sender,
                format!("Reasoning trial {number} failed · retrying from trial 1"),
            )
            .await?;
            return Ok(None);
        }
        history.push(message("assistant", reply.unwrap().0));
    }

    status(
        sender,
        format!("Attempt {attempt} · 10/10 reasoning passed · generating Rust parser"),
    )
    .await?;
    settings.project_tools = false;
    settings.output_tokens = original.settings.output_tokens;
    settings.instructions = "Return the complete source for solution.rs. Use the standard library and preserve the exact public interface. Return plain Rust or one Rust code fence.".into();
    history.push(message("user", PARSER_PROMPT.trim().into()));
    if let Some((answer, _)) =
        trial_reply(settings, &workspace, &checkpoint, &history, cancel).await?
    {
        status(sender, "Building the Rust parser with Cargo").await?;
        if builds(&workspace, &answer).await? {
            history.push(message("assistant", answer));
            return Ok(Some(history));
        }
    }
    status(
        sender,
        "Rust build failed · retrying from reasoning trial 1",
    )
    .await?;
    Ok(None)
}

fn message(role: &'static str, content: String) -> InputMessage {
    InputMessage {
        role,
        content,
        images: Vec::new(),
    }
}

async fn trial_reply(
    settings: super::Settings,
    workspace: &Path,
    checkpoint: &Path,
    history: &[InputMessage],
    cancel: watch::Receiver<bool>,
) -> Result<Option<(String, bool)>, String> {
    let mut request = Request::for_task(
        settings,
        String::new(),
        Some(workspace.to_owned()),
        Mode::Chat,
    );
    request.messages = history.to_vec();
    // A separate journal avoids replaying previous tasks in addition to the explicit history.
    request.checkpoint =
        Some(checkpoint.with_file_name(format!("trial-{}.md", history.len() / 2 + 1)));
    let (send, mut receive) = mpsc::channel(64);
    let mut compacted = false;
    let (result, (answer, read_file)) = tokio::join!(
        stream_chat_inner(request, send, cancel, &mut compacted),
        async {
            let mut answer = String::new();
            let mut read_file = false;
            while let Some(event) = receive.recv().await {
                match event {
                    ProviderEvent::Content(text) => answer.push_str(&text),
                    ProviderEvent::ReplaceContent { offset, text } => {
                        answer.truncate(offset);
                        answer.push_str(&text);
                    }
                    ProviderEvent::Status(text) => read_file |= text.ends_with(": read_file"),
                    _ => {}
                }
            }
            (answer, read_file)
        }
    );
    if compacted {
        return Err("Aisnley-Ox-1-bbb ran out of context while qualifying. Increase the context window in Agents.".into());
    }
    match result {
        Ok(()) => Ok(Some((answer, read_file))),
        Err(error)
            if error.contains("Ollama returned no answer")
                || error.contains("reached the output limit") =>
        {
            Ok(None)
        }
        Err(error) => Err(error),
    }
}

async fn builds(workspace: &Path, reply: &str) -> Result<bool, String> {
    let reply = reply.trim();
    let source = if let Some(fence) = reply.strip_prefix("```") {
        let Some((language, rest)) = fence.split_once('\n') else {
            return Ok(false);
        };
        if !matches!(
            language.trim().to_ascii_lowercase().as_str(),
            "" | "rust" | "rs"
        ) {
            return Ok(false);
        }
        let Some(source) = rest.strip_suffix("\n```") else {
            return Ok(false);
        };
        source
    } else {
        reply
    };
    if source.trim().is_empty() || source.contains("```") {
        return Ok(false);
    }
    let source_dir = workspace.join("src");
    tokio::fs::create_dir_all(&source_dir)
        .await
        .map_err(|error| error.to_string())?;
    let manifest = "[package]\nname = \"redox-qualification\"\nversion = \"0.1.0\"\nedition = \"2021\"\nbuild = false\n[workspace]\n";
    for (path, contents) in [
        (workspace.join("Cargo.toml"), manifest),
        (source_dir.join("main.rs"), PARSER_HARNESS),
        (source_dir.join("solution.rs"), source),
    ] {
        tokio::fs::write(path, contents)
            .await
            .map_err(|error| error.to_string())?;
    }
    let mut cargo = tokio::process::Command::new("cargo");
    cargo
        .current_dir(workspace)
        .args([
            "build",
            "--offline",
            "--color",
            "never",
            "--target-dir",
            "target",
        ])
        .kill_on_drop(true);
    #[cfg(windows)]
    cargo.creation_flags(windows_sys::Win32::System::Threading::CREATE_NO_WINDOW);
    let output = tokio::time::timeout(Duration::from_secs(180), cargo.output())
        .await
        .map_err(|_| "Aisnley-Ox-1-bbb qualification Cargo build timed out.")?
        .map_err(|error| format!("Cannot start Cargo for qualification: {error}"))?;
    tokio::fs::write(workspace.join("cargo-output.txt"), &output.stderr)
        .await
        .map_err(|error| error.to_string())?;
    Ok(output.status.success())
}
