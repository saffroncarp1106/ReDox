use std::{env, io, path::PathBuf, process::ExitCode};

use redox::codex::ProviderEvent;
use redox::domain::{Chat, Message, MessageStatus, Mode, Provider, Role};
use redox::ollama::{self, Settings, Stats};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::sync::{mpsc, watch};

fn event(value: Value) {
    println!("{value}");
}

fn message(role: Role, content: String, model: &str) -> Message {
    Message {
        id: uuid::Uuid::new_v4(),
        role,
        content,
        status: MessageStatus::Complete,
        error: None,
        model: model.into(),
        attachments: Vec::new(),
        activity: Default::default(),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
enum Suite {
    General,
    Rust,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Input {
    Prompt(String),
    Task {
        prompt: String,
        model: String,
        suite: Option<Suite>,
        output: Option<u32>,
        keep_alive: Option<i64>,
    },
}

async fn bench() -> Result<(), String> {
    let mut args = env::args_os().skip(1);
    let usage = "Usage: ollama-bench-runner <endpoint> <model> <context> <output> <workspace> [general|rust]";
    let [
        Some(endpoint),
        Some(model),
        Some(context),
        Some(output),
        Some(workspace),
        suite,
        None,
    ] = std::array::from_fn(|_| args.next())
    else {
        return Err(usage.into());
    };
    let rust_suite = match suite
        .as_deref()
        .map(|value| value.to_str().ok_or(usage))
        .transpose()?
    {
        None | Some("general") => false,
        Some("rust") => true,
        _ => return Err(usage.into()),
    };
    let model = model.into_string().map_err(|_| usage)?;
    let workspace = PathBuf::from(workspace);
    let settings = Settings {
        endpoint: endpoint.into_string().map_err(|_| usage)?,
        preferred_model: model.clone(),
        context_tokens: context.to_str().ok_or(usage)?.parse().map_err(|_| usage)?,
        output_tokens: output.to_str().ok_or(usage)?.parse().map_err(|_| usage)?,
        ..Settings::default()
    };
    let mut chat = Chat::new((!rust_suite).then(|| workspace.clone()));
    chat.provider = Provider::Ollama;
    chat.ollama_model = model;
    chat.mode = Mode::Chat;
    let folder = dirs::data_local_dir()
        .ok_or("Application data folder unavailable.")?
        .join("ReDox/agent-state")
        .join(chat.id.to_string());
    event(json!({"event":"ready", "chat":chat.id, "folder":folder}));

    for line in io::stdin().lines() {
        let line = line.map_err(|error| error.to_string())?;
        let input = serde_json::from_str(&line).map_err(|error| error.to_string())?;
        let (prompt, model, suite, output, keep_alive) = match input {
            Input::Prompt(prompt) => (prompt, chat.ollama_model.clone(), None, None, None),
            Input::Task {
                prompt,
                model,
                suite,
                output,
                keep_alive,
            } => (prompt, model.trim().to_owned(), suite, output, keep_alive),
        };
        if model.is_empty() {
            return Err("A task must name a nonempty Ollama model.".into());
        }
        chat.ollama_model.clone_from(&model);
        chat.messages.push(message(Role::User, prompt, &model));
        let rust_task = suite
            .as_ref()
            .map_or(rust_suite, |value| matches!(value, Suite::Rust));
        let mut task_settings = settings.clone();
        task_settings.project_tools = !rust_task;
        task_settings.output_tokens = output.unwrap_or(settings.output_tokens);
        task_settings.keep_alive = keep_alive;
        task_settings.instructions = if rust_task {
            "Return the complete Rust source for the requested solution.rs module. Use the standard library and the exact public signatures in the task. Return plain Rust or one Rust code fence."
        } else {
            "Solve each benchmark task independently unless it asks you to recall a conversational value. When a task asks you to read a fixture, call the supplied read_file tool before giving your final answer. Return the requested JSON as your final answer. Keep any explicitly remembered benchmark marker available for later tasks."
        }.into();
        let mut request = ollama::Request::from_chat(&chat, task_settings);
        if suite.is_some() {
            request.workspace = Some(workspace.clone());
        }
        let (sender, mut events) = mpsc::channel(128);
        let (_stop, cancel) = watch::channel(false);
        let mut text = String::new();
        let mut stats = Stats::default();
        let mut rollovers = 0;
        let mut file_reads = 0;
        let (result, ()) = tokio::join!(ollama::stream_chat(request, sender, cancel), async {
            while let Some(update) = events.recv().await {
                match update {
                    ProviderEvent::Content(delta) => {
                        text.push_str(&delta);
                        event(json!({"event":"delta", "text":delta}));
                    }
                    ProviderEvent::ReplaceContent {
                        offset,
                        text: replacement,
                    } => {
                        text.truncate(offset);
                        text.push_str(&replacement);
                        event(json!({"event":"replace", "text":text}));
                    }
                    ProviderEvent::Metrics(value) => stats = value,
                    ProviderEvent::Status(status) => {
                        rollovers +=
                            usize::from(status.ends_with("saving progress and compacting context"));
                        file_reads += usize::from(status.ends_with(": read_file"));
                        event(json!({"event":"status", "text":status}));
                    }
                    ProviderEvent::Approval(approval) => {
                        let _ = approval.response.send(false);
                    }
                    _ => {}
                }
            }
        });
        if result.is_ok() {
            if rollovers > 0 {
                // The native journal now carries the compacted part of this chat.
                chat.messages.drain(..chat.messages.len() - 1);
            }
            chat.messages
                .push(message(Role::Assistant, text.clone(), &model));
        } else {
            chat.messages.pop();
        }
        event(
            json!({"event":"done", "model":model, "text":text, "error":result.err(), "stats":stats,
            "rollovers":rollovers, "file_reads":file_reads}),
        );
    }
    Ok(())
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    match bench().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Benchmark runner: {error}");
            ExitCode::FAILURE
        }
    }
}
