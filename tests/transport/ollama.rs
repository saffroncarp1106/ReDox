use super::*;

#[test]
fn edited_ollama_request_uses_the_revised_path_and_a_new_checkpoint() {
    use crate::domain::{PromptEdit, Session};
    let mut session = Session::default();
    session.new_chat();
    let chat = session.active_mut().unwrap();
    chat.set_provider(crate::domain::Provider::Ollama, "test".into());
    chat.draft = "Old task".into();
    let run = session.prepare_selected_run(None).unwrap();
    session.run_message_mut(run).unwrap().content = "Old reply".into();
    session.finish_run(run, MessageStatus::Complete, None);

    let chat = session.active().unwrap();
    let checkpoint = Request::from_chat(chat, Settings::default()).checkpoint;
    let prompt = PromptEdit {
        chat: chat.id,
        message: chat.messages[0].id,
        content: "New task",
        removed_attachments: &[],
    };
    session.prepare_edited_run(prompt, None).unwrap();
    let request = Request::from_chat(session.active().unwrap(), Settings::default());
    assert_ne!(request.checkpoint, checkpoint);
    assert_eq!(request.messages.len(), 1);
    assert_eq!(request.messages[0].content, "New task");
}
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};

use crate::run::{RunEvent, Update, apply_event};

pub(crate) type Requests = Arc<Mutex<Vec<(String, Value)>>>;

pub(crate) fn server(
    responses: Vec<(&'static str, String)>,
) -> (String, Requests, std::thread::JoinHandle<()>) {
    server_with_status(
        responses
            .into_iter()
            .map(|(path, body)| (path, 200, body))
            .collect(),
    )
}

fn server_with_status(
    responses: Vec<(&'static str, u16, String)>,
) -> (String, Requests, std::thread::JoinHandle<()>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let requests = Arc::new(Mutex::new(Vec::new()));
    let captured = requests.clone();
    let handle = std::thread::spawn(move || {
        for (expected_path, status, body) in responses {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut bytes = Vec::new();
            let end = loop {
                let mut byte = [0];
                socket.read_exact(&mut byte).unwrap();
                bytes.push(byte[0]);
                if bytes.ends_with(b"\r\n\r\n") {
                    break bytes.len();
                }
                assert!(bytes.len() < 16384);
            };
            let headers = String::from_utf8(bytes[..end].to_vec()).unwrap();
            assert!(
                headers.lines().next().unwrap().contains(expected_path),
                "{headers}"
            );
            let length: usize = headers
                .lines()
                .find_map(|line| {
                    line.to_ascii_lowercase()
                        .strip_prefix("content-length:")
                        .map(|length| length.trim().parse().unwrap())
                })
                .unwrap_or(0);
            let mut payload = vec![0; length];
            socket.read_exact(&mut payload).unwrap();
            captured.lock().unwrap().push((
                expected_path.into(),
                serde_json::from_slice(&payload).unwrap_or(Value::Null),
            ));
            write!(socket, "HTTP/1.1 {status} Response\r\nContent-Type: application/x-ndjson\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()).unwrap();
            for chunk in body.as_bytes().chunks(3) {
                socket.write_all(chunk).unwrap();
            }
        }
    });
    (endpoint, requests, handle)
}

fn request(endpoint: String, tools: bool) -> Request {
    let mut chat = Chat::new(None);
    chat.set_provider(crate::domain::Provider::Ollama, "qwen-test:latest".into());
    chat.draft = "test".into();
    Request {
        settings: Settings {
            endpoint,
            project_tools: tools,
            ..Settings::default()
        },
        model: chat.ollama_model,
        workspace: None,
        mode: Mode::Chat,
        desktop: Default::default(),
        desktop_activity: Default::default(),
        worker: None,
        saffyos_locked: false,
        saffyos_folder: None,
        recover_saffyos_loop: false,
        messages: vec![InputMessage {
            id: None,
            role: "user",
            content: "Hello".into(),
            images: Vec::new(),
        }],
        checkpoint: Some(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("temp/verification/ollama-state")
                .join(uuid::Uuid::new_v4().to_string())
                .join("steps.md"),
        ),
    }
}

fn project_request(endpoint: String) -> Request {
    let mut request = request(endpoint, true);
    let root = request
        .checkpoint
        .as_ref()
        .unwrap()
        .parent()
        .unwrap()
        .join("project");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("note.txt"), "READ_CONFIRMED").unwrap();
    request.workspace = Some(root);
    request.settings.context_tokens = 32768;
    request
}

fn read_reply(count: usize) -> String {
    read_paths((0..count).map(|_| "note.txt".to_owned()))
}

fn read_paths(paths: impl IntoIterator<Item = String>) -> String {
    let calls = paths
        .into_iter()
        .map(|path| json!({"function":{"name":"read_file","arguments":{"path":path}}}))
        .collect::<Vec<_>>();
    format!(
        "{}\n",
        json!({"message":{"content":"","tool_calls":calls},"done":true,"eval_count":1})
    )
}

fn text_reply(text: &str) -> String {
    format!(
        "{}\n",
        json!({"message":{"content":text},"done":true,"eval_count":1})
    )
}

#[tokio::test]
async fn saffyos_revises_its_own_file_but_rejects_escape_and_host_tools() {
    use crate::domain::{Provider, Session};

    let model_info =
        json!({"capabilities":["tools","vision"],"system":"Host profile instructions"});
    let files = [
        ("experiment.py", "print('test')"),
        ("experiment.ps1", "Write-Output test"),
        ("notes.md", "Ainsley test"),
        ("../outside.txt", "outside"),
        ("NUL.txt", "device"),
        ("experiment.py", "overwrite"),
    ];
    let mut calls = files.iter().map(|(name, content)| {
        json!({"function":{"name":"create_file","arguments":{"name":name,"content":content}}})
    }).collect::<Vec<_>>();
    calls
        .push(json!({"function":{"name":"run_command","arguments":{"command":"echo unexpected"}}}));
    let tool_reply = json!({"message":{"tool_calls":calls},"done":true});
    let revision = json!({"message":{"tool_calls":[
        {"function":{"name":"read_file","arguments":{"path":"notes.md"}}},
        {"function":{"name":"write_file","arguments":{"path":"notes.md","content":"Revised notes"}}}
    ]},"done":true});
    let (endpoint, captured, server) = server(vec![
        ("/api/show", model_info.to_string()),
        ("/api/chat", format!("{tool_reply}\n")),
        ("/api/chat", format!("{revision}\n")),
        ("/api/chat", text_reply("Notes revised in place.")),
    ]);
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("temp/verification/saffyos")
        .join(uuid::Uuid::new_v4().to_string());
    let folder = fixture.join("private-workspace");
    std::fs::create_dir_all(&folder).unwrap();
    let mut session = Session::default();
    session.open_saffyos(folder.canonicalize().unwrap());
    let chat = session.active_mut().unwrap();
    chat.set_provider(Provider::Ollama, RUST_PROFILE_MODEL.into());
    chat.set_provider(Provider::Codex, String::new());
    chat.set_mode(Mode::FullAccess);
    assert_eq!(chat.provider, Provider::Ollama);
    assert!(chat.mode == Mode::Chat);
    chat.draft = "Begin the test".into();
    session.prepare_selected_run(None).unwrap();

    let mut request = Request::from_chat(
        session.active().unwrap(),
        Settings {
            endpoint,
            project_tools: true,
            instructions: "Private global instructions".into(),
            ..Settings::default()
        },
    );
    request.desktop.enabled = true;
    assert!(request.checkpoint.is_none());
    let (sender, _events) = mpsc::channel(32);
    let (_stop, cancel) = watch::channel(false);
    stream_qualified_chat(request, sender, cancel)
        .await
        .unwrap();
    for (name, content) in &files[..2] {
        assert_eq!(
            std::fs::read_to_string(folder.join(name)).unwrap(),
            *content
        );
    }
    assert_eq!(
        std::fs::read_to_string(folder.join("notes.md")).unwrap(),
        "Revised notes"
    );
    assert_eq!(std::fs::read_dir(&folder).unwrap().count(), 3);
    assert!(!fixture.join("outside.txt").exists());
    server.join().unwrap();
    let requests = captured.lock().unwrap();
    let body = &requests[1].1;
    let names = body["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|tool| tool["function"]["name"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        names,
        [
            "run_python",
            "list_files",
            "make_directory",
            "create_file",
            "read_file",
            "write_file"
        ]
    );
    let tool_results = requests[2].1["messages"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|message| message["role"] == "tool")
        .collect::<Vec<_>>();
    assert_eq!(tool_results.len(), 7);
    assert!(tool_results[3..].iter().all(|message| {
        message["content"]
            .as_str()
            .unwrap()
            .starts_with("Tool failed:")
    }));
    assert!(
        tool_results[5]["content"]
            .as_str()
            .unwrap()
            .contains("use read_file, then write_file on the same path")
    );
    let revision_results = requests[3].1["messages"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|message| message["role"] == "tool")
        .rev()
        .take(2)
        .map(|message| message["content"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        revision_results,
        ["Updated notes.md (13 bytes).", "Ainsley test"]
    );
    let payload = body.to_string();
    for private in [
        "private-workspace",
        "Host profile instructions",
        "Private global instructions",
    ] {
        assert!(!payload.contains(private), "{payload}");
    }
}

#[tokio::test]
#[ignore = "requires the installed SaffyOS Python runtime"]
async fn saffyos_python_runs_offline_and_leaves_host_files_unchanged() {
    use crate::domain::{Provider, Session};

    let fixture = tempfile::tempdir().unwrap();
    let workspace = fixture.path().join("workspace");
    std::fs::create_dir(&workspace).unwrap();
    let script = r#"
import os
from pathlib import Path
import socket
import sys

assert sys.argv[1] == 'test-argument'
assert input() == 'test-input'
assert not os.environ
data = Path('input.txt')
assert data.read_text() == 'original'
data.write_text('changed')
assert data.read_text() == 'changed'
Path('output.txt').write_text('temporary')
try:
    assert Path('../outside.txt').read_text() != 'protected'
except OSError:
    pass
try:
    Path('../outside.txt').write_text('escaped')
except OSError:
    pass
Path('/guest-root.txt').write_text('inside the sandbox')
try:
    socket.socket().connect(('127.0.0.1', int(sys.argv[2])))
except (OSError, AttributeError):
    pass
else:
    raise AssertionError('networking succeeded')
print('sandbox passed')
print('stderr captured', file=sys.stderr)
"#;
    std::fs::write(workspace.join("check.py"), script).unwrap();
    std::fs::write(workspace.join("input.txt"), "original").unwrap();
    std::fs::write(fixture.path().join("outside.txt"), "protected").unwrap();

    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port().to_string();
    let call = json!({"message":{"tool_calls":[{"function":{
        "name":"run_python",
        "arguments":{"path":"check.py","inputs":["input.txt"],"args":["test-argument",port],"stdin":"test-input\n"}
    }}]},"done":true});
    let (endpoint, captured, handle) = server(vec![
        ("/api/show", json!({"capabilities":["tools"]}).to_string()),
        ("/api/chat", format!("{call}\n")),
        ("/api/chat", text_reply("Done.")),
    ]);
    let mut session = Session::default();
    session.open_saffyos(workspace.canonicalize().unwrap());
    let chat = session.active_mut().unwrap();
    chat.set_provider(Provider::Ollama, "test:latest".into());
    chat.draft = "Verify the workspace script".into();
    session.prepare_selected_run(None).unwrap();

    let request = Request::from_chat(
        session.active().unwrap(),
        Settings {
            endpoint,
            ..Settings::default()
        },
    );
    let (sender, _events) = mpsc::channel(64);
    let (_stop, cancel) = watch::channel(false);
    stream_qualified_chat(request, sender, cancel)
        .await
        .unwrap();
    handle.join().unwrap();

    let requests = captured.lock().unwrap();
    let message = requests[2].1["messages"]
        .as_array()
        .unwrap()
        .last()
        .unwrap();
    let content = message["content"].as_str().unwrap();
    let result: Value = serde_json::from_str(content).unwrap_or_else(|_| panic!("{content}"));
    assert_eq!(result["exit_code"], 0, "{result}");
    assert_eq!(result["stdout"], "sandbox passed\n", "{result}");
    assert_eq!(result["stderr"], "stderr captured\n", "{result}");
    assert_eq!(
        std::fs::read_to_string(workspace.join("input.txt")).unwrap(),
        "original"
    );
    assert_eq!(
        std::fs::read_to_string(fixture.path().join("outside.txt")).unwrap(),
        "protected"
    );
    assert!(!workspace.join("output.txt").exists());
}

#[test]
fn saffyos_file_tools_work_in_subdirectories_without_following_links() {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("temp/verification/saffyos")
        .join(uuid::Uuid::new_v4().to_string());
    let workspace = fixture.join("workspace");
    let outside = fixture.join("outside");
    std::fs::create_dir_all(workspace.join("Pictures")).unwrap();
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::write(outside.join("private.txt"), "outside").unwrap();
    let folder = saffyos_tools::open_folder(&workspace.canonicalize().unwrap()).unwrap();
    let call = |name, args| saffyos_tools::execute(&folder, name, &args);

    let root_listing: Value =
        serde_json::from_str(&call("list_files", json!({})).unwrap()).unwrap();
    assert_eq!(root_listing["directories"], json!(["Pictures"]));
    call("make_directory", json!({"path":"Pictures/Empty"})).unwrap();
    let path = "Pictures/Work in progress/notes.md";
    call(
        "create_file",
        json!({"name": path, "content":"original text"}),
    )
    .unwrap();
    let listing: Value = serde_json::from_str(
        &call("list_files", json!({"path":r"Pictures\Work in progress"})).unwrap(),
    )
    .unwrap();
    assert_eq!(listing["names"], json!(["notes.md"]));
    assert!(call("create_file", json!({"name":path,"content":"duplicate"})).is_err());
    assert_eq!(
        call("read_file", json!({"path":path})).unwrap(),
        "original text"
    );
    call("write_file", json!({"path":path,"content":"updated"})).unwrap();
    assert_eq!(
        std::fs::read_to_string(workspace.join(path)).unwrap(),
        "updated"
    );
    assert!(workspace.join("Pictures/Empty").is_dir());

    for blocked in [
        "../outside/private.txt",
        r"Pictures\..\..\outside\private.txt",
        "/Pictures/private.txt",
        r"C:private.txt",
        r"C:\private.txt",
        r"\\server\share\private.txt",
        "Pictures/CON .txt",
        "Pictures/LPT¹.txt",
        "Pictures/notes.md:stream",
    ] {
        assert!(
            call("create_file", json!({"name":blocked,"content":"bad"})).is_err(),
            "{blocked}"
        );
        assert!(
            call("read_file", json!({"path":blocked})).is_err(),
            "{blocked}"
        );
    }

    std::fs::hard_link(outside.join("private.txt"), workspace.join("linked.txt")).unwrap();
    assert!(call("read_file", json!({"path":"linked.txt"})).is_err());
    assert!(call("write_file", json!({"path":"linked.txt","content":"bad"})).is_err());

    let link = workspace.join("redirect");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&outside, &link).unwrap();
    #[cfg(windows)]
    assert!(
        std::process::Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command",
                "New-Item -ItemType Junction -Path $env:REDOX_TEST_LINK -Target $env:REDOX_TEST_TARGET -ErrorAction Stop | Out-Null"])
            .env("REDOX_TEST_LINK", link)
            .env("REDOX_TEST_TARGET", &outside)
            .status()
            .unwrap()
            .success()
    );
    assert!(call("list_files", json!({"path":"redirect"})).is_err());
    assert!(call("make_directory", json!({"path":"redirect/new-dir"})).is_err());
    assert!(
        call(
            "create_file",
            json!({"name":"redirect/new.txt","content":"bad"})
        )
        .is_err()
    );
    assert!(call("read_file", json!({"path":"redirect/private.txt"})).is_err());
    assert!(
        call(
            "write_file",
            json!({"path":"redirect/private.txt","content":"bad"})
        )
        .is_err()
    );
    assert_eq!(
        std::fs::read_to_string(outside.join("private.txt")).unwrap(),
        "outside"
    );
    assert_eq!(std::fs::read_dir(outside).unwrap().count(), 1);
}

#[tokio::test]
async fn rust_profile_uses_model_instructions_and_keeps_reasoning_out_of_answer() {
    let profile = "Review the Rust public interface.";
    let model_info = json!({"capabilities":["thinking"],"system":profile});
    let reply = json!({
        "message":{"content":"fn main() {}"},
        "done":true,
        "eval_count":8,
        "prompt_eval_count":240,
        "prompt_eval_cached_count":128,
        "prompt_eval_duration":250_000_000
    });
    for (model, expected) in [
        (RUST_PROFILE_MODEL, true),
        ("redox-qwen-rust:35b", true),
        ("qwen3.6:35b", false),
    ] {
        let (endpoint, captured, handle) = server(vec![
            ("/api/show", model_info.to_string()),
            (
                "/api/chat",
                format!(
                    "{}\n{reply}\n",
                    json!({"message":{"thinking":"Private reasoning"},"done":false})
                ),
            ),
        ]);
        let mut request = request(endpoint, false);
        request.model = model.into();
        request.settings.context_tokens = 64_000;
        request.settings.output_tokens = 16_384;
        let (sender, mut events) = mpsc::channel(16);
        let (_cancel, receiver) = watch::channel(false);
        stream_chat(request, sender, receiver).await.unwrap();
        handle.join().unwrap();

        let calls = captured.lock().unwrap();
        let body = &calls[1].1;
        assert_eq!(body["think"], expected);
        assert_eq!(body["options"]["num_predict"], 16_000);
        let system = body["messages"][0]["content"].as_str().unwrap();
        assert_eq!(system.contains(profile), expected);
        drop(calls);

        let mut answer = String::new();
        let mut usage = Vec::new();
        let mut stages = Vec::new();
        while let Some(event) = events.recv().await {
            match event {
                ProviderEvent::Content(content) => answer.push_str(&content),
                ProviderEvent::Context(sample) => usage.push(sample),
                ProviderEvent::OllamaStage(stage) => stages.push(stage),
                _ => {}
            }
        }
        assert_eq!(answer, "fn main() {}");
        assert_eq!(
            stages,
            [
                "Checking Ollama model…",
                "Waiting for Ollama to load or read the prompt…",
                "Thinking…",
                "Writing reply…",
            ],
        );
        let [estimate, measured] = usage.as_slice() else {
            panic!("Expected a context estimate followed by Ollama's count");
        };
        assert!(estimate.estimated && estimate.tokens > 0);
        assert_eq!(estimate.limit, 64_000);
        assert!(!measured.estimated);
        assert_eq!(measured.limit, 64_000);
        assert_eq!(measured.tokens, 248);
        assert_eq!(measured.cached_tokens, Some(128));
        assert_eq!(measured.prompt_seconds, Some(0.25));
    }
}

#[tokio::test]
async fn rust_profile_requests_a_direct_answer_after_reasoning_exhausts_output() {
    let truncated = json!({
        "message":{"thinking":"Checking the input specification"},
        "done":true,
        "done_reason":"length",
        "eval_count":2048
    });
    let (endpoint, captured, handle) = server(vec![
        (
            "/api/show",
            json!({"capabilities":["thinking"]}).to_string(),
        ),
        ("/api/chat", format!("{truncated}\n")),
        ("/api/chat", text_reply("fn main() {}")),
    ]);
    let mut request = request(endpoint, false);
    request.model = "redox-qwen-rust:35b".into();
    let (sender, mut events) = mpsc::channel(16);
    let (_cancel, receiver) = watch::channel(false);
    stream_chat(request, sender, receiver).await.unwrap();
    handle.join().unwrap();

    let calls = captured.lock().unwrap();
    let (first, second) = (&calls[1].1, &calls[2].1);
    assert_eq!(first["think"], true);
    assert_eq!(second["think"], false);
    assert_eq!(first["messages"], second["messages"]);
    drop(calls);

    let mut answer = String::new();
    while let Some(event) = events.recv().await {
        if let ProviderEvent::Content(content) = event {
            answer.push_str(&content);
        }
    }
    assert_eq!(answer, "fn main() {}");
}

#[tokio::test]
async fn versioned_ainsley_keeps_direct_answer_mode_after_a_tool_call() {
    let truncated = json!({
        "message":{"thinking":"Checking the input specification"},
        "done":true,
        "done_reason":"length",
        "eval_count":2048
    });
    let (endpoint, captured, handle) = server(vec![
        (
            "/api/show",
            json!({"capabilities":["thinking", "tools"]}).to_string(),
        ),
        ("/api/chat", format!("{truncated}\n")),
        ("/api/chat", read_reply(1)),
        ("/api/chat", text_reply("Read the note.")),
    ]);
    let mut request = project_request(endpoint);
    request.model = "Aisnley-Ox-1.4-bbb:35b".into();
    let (sender, mut events) = mpsc::channel(16);
    let (_cancel, receiver) = watch::channel(false);
    stream_chat(request, sender, receiver).await.unwrap();
    handle.join().unwrap();

    let calls = captured.lock().unwrap();
    let (first, second) = (&calls[1].1, &calls[2].1);
    assert_eq!(first["think"], true);
    assert_eq!(second["think"], false);
    assert_eq!(first["messages"], second["messages"]);
    let third = &calls[3].1;
    assert_eq!(third["think"], false);
    assert_eq!(
        third["messages"].as_array().unwrap().last().unwrap()["role"],
        "tool"
    );
    drop(calls);

    let mut answer = String::new();
    while let Some(event) = events.recv().await {
        if let ProviderEvent::Content(content) = event {
            answer.push_str(&content);
        }
    }
    assert_eq!(answer.trim(), "Read the note.");
}

#[tokio::test]
async fn ainsley_1_5_continues_with_its_reasoning_and_keeps_tool_support() {
    let plan = json!({
        "message": {"thinking": "Read and check the loop bounds."},
        "done": true,
        "done_reason": "length",
        "eval_count": 768
    });
    let partial = json!({
        "message": {"thinking": "I found the bug.", "content": "The loop stops"},
        "done": true,
        "done_reason": "length",
        "eval_count": 768
    });
    let (endpoint, captured, handle) = server(vec![
        (
            "/api/show",
            json!({"capabilities":["thinking", "tools"], "renderer":"qwen3.5", "parser":"qwen3.5"})
                .to_string(),
        ),
        ("/api/chat", format!("{plan}\n")),
        ("/api/chat", read_reply(1)),
        ("/api/chat", format!("{partial}\n")),
        ("/api/chat", text_reply(" advancing at one.")),
    ]);
    let mut request = project_request(endpoint);
    request.model = "Aisnley-Ox-1.5-bbb:exp-loop-check".into();
    let (sender, mut events) = mpsc::channel(32);
    let (_cancel, receiver) = watch::channel(false);
    stream_chat(request, sender, receiver).await.unwrap();
    handle.join().unwrap();

    {
        let calls = captured.lock().unwrap();
        let chat = |index: usize| &calls[index].1;
        let last_message = |index| chat(index)["messages"].as_array().unwrap().last().unwrap();
        assert_eq!(chat(1)["think"], true);
        assert_eq!(chat(1)["options"]["num_predict"], 768);
        assert_eq!(chat(2)["think"], true);
        assert_eq!(chat(2)["options"]["num_predict"], 1280);
        assert_eq!(
            last_message(2),
            &json!({"role":"assistant", "thinking": "Read and check the loop bounds.", "content":"\n"})
        );
        assert_eq!(chat(2)["tools"], chat(1)["tools"]);
        assert_eq!(last_message(3)["role"], "tool");
        let messages = chat(3)["messages"].as_array().unwrap();
        assert_eq!(
            messages[messages.len() - 2]["thinking"],
            "Read and check the loop bounds."
        );
        assert_eq!(chat(3)["options"]["num_predict"], 768);
        assert_eq!(last_message(4)["content"], "The loop stops");
        assert_eq!(last_message(4)["thinking"], "I found the bug.");
    }

    let mut answer = String::new();
    while let Some(event) = events.recv().await {
        if let ProviderEvent::Content(content) = event {
            answer.push_str(&content);
        }
    }
    assert_eq!(answer.trim(), "The loop stops advancing at one.");
}

#[tokio::test]
async fn ainsley_1_5_preserves_tool_evidence_when_reasoning_fills_context() {
    let mut tool: Value = serde_json::from_str(&read_reply(1)).unwrap();
    tool["message"]["thinking"] = json!("Reasoning before reading the file. ".repeat(230));
    let (endpoint, captured, handle) = server(vec![
        (
            "/api/show",
            json!({"capabilities":["thinking", "tools"], "renderer":"qwen3.5", "parser":"qwen3.5"})
                .to_string(),
        ),
        ("/api/chat", format!("{tool}\n")),
        ("/api/chat", text_reply("Read the note.")),
    ]);
    let mut request = project_request(endpoint);
    request.model = "Aisnley-Ox-1.5-bbb:exp-loop-check".into();
    request.settings.context_tokens = 8192;
    let (sender, _events) = mpsc::channel(32);
    let (_cancel, receiver) = watch::channel(false);
    stream_chat(request, sender, receiver).await.unwrap();
    handle.join().unwrap();

    let calls = captured.lock().unwrap();
    let messages = calls[2].1["messages"].as_array().unwrap();
    let result = messages.last().unwrap();
    assert_eq!(result["role"], "tool");
    assert_eq!(result["content"], "READ_CONFIRMED");
    let assistant = &messages[messages.len() - 2];
    assert!(assistant.get("thinking").is_none());
    assert_eq!(assistant["tool_calls"][0]["function"]["name"], "read_file");
}

#[tokio::test]
async fn agent_continues_beyond_sixteen_rounds_and_tool_calls() {
    let mut responses = vec![("/api/show", json!({"capabilities":["tools"]}).to_string())];
    let path = |index| format!("note-{index}.txt");
    responses.push(("/api/chat", read_paths((0..17).map(path))));
    for index in 17..35 {
        responses.push(("/api/chat", read_paths([path(index)])));
    }
    responses.push(("/api/chat", text_reply("FINISHED")));
    let (endpoint, captured, handle) = server(responses);
    let request = project_request(endpoint);
    for index in 0..35 {
        std::fs::write(
            request.workspace.as_ref().unwrap().join(path(index)),
            "READ_CONFIRMED",
        )
        .unwrap();
    }
    let checkpoint = request.checkpoint.clone().unwrap();
    let (sender, mut events) = mpsc::channel(256);
    let (_cancel, receiver) = watch::channel(false);
    stream_chat(request, sender, receiver).await.unwrap();
    handle.join().unwrap();

    {
        let calls = captured.lock().unwrap();
        assert_eq!(calls.len(), 21);
        assert_eq!(
            calls.last().unwrap().1["messages"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|message| message["role"] == "tool")
                .count(),
            35
        );
    }
    let saved = std::fs::read_to_string(checkpoint).unwrap();
    assert!(saved.contains("Status: complete"));
    assert!(saved.contains("READ_CONFIRMED"));
    let mut requests = 0;
    while let Some(event) = events.recv().await {
        if let ProviderEvent::Metrics(stats) = event {
            requests += stats.requests;
        }
    }
    assert_eq!(requests, 20);
}

#[tokio::test]
async fn context_rollover_saves_progress_and_reload_recalls_the_next_task() {
    let (endpoint, captured, handle) = server(vec![
        (
            "/api/show",
            json!({"capabilities":["tools"],"model_info":{
                "general.architecture":"qwen","qwen.context_length":65536
            }})
            .to_string(),
        ),
        ("/api/chat", read_reply(1)),
        (
            "/api/chat",
            text_reply(
                "CHECKPOINT_MARKER: note.txt was read. Next: report the result; no edits made.",
            ),
        ),
        ("/api/chat", text_reply("Result reported.")),
    ]);
    let mut request = project_request(endpoint);
    request.settings.context_tokens = 131_072;
    request.settings.output_tokens = 2048;
    std::fs::write(
        request.workspace.as_ref().unwrap().join("note.txt"),
        "PROJECT_DATA ".repeat(7000),
    )
    .unwrap();
    let checkpoint = request.checkpoint.clone().unwrap();
    let workspace = request.workspace.clone();
    let (sender, mut events) = mpsc::channel(128);
    let (_cancel, receiver) = watch::channel(false);
    stream_chat(request, sender, receiver).await.unwrap();
    handle.join().unwrap();

    {
        let calls = captured.lock().unwrap();
        for (_, body) in &calls[1..] {
            assert_eq!(body["options"]["num_ctx"], 65_536);
        }
        assert_eq!(calls[2].1["stream"], false);
        assert!(calls[2].1.get("tools").is_none());
        let summary_input = calls[2].1["messages"][1]["content"].as_str().unwrap();
        assert!(summary_input.contains("PROJECT_DATA"));
        assert!(summary_input.len() < 16 * 1024);
        let messages: Vec<Value> = serde_json::from_value(calls[3].1["messages"].clone()).unwrap();
        assert!(messages.iter().any(|message| {
            message["content"]
                .as_str()
                .is_some_and(|text| text.contains("CHECKPOINT_MARKER"))
        }));
        assert!(
            messages.last().unwrap()["content"]
                .as_str()
                .unwrap()
                .contains("Continue the current task from this saved checkpoint")
        );
        assert!(
            messages
                .iter()
                .all(|message| message["role"] != "tool" && message.get("tool_calls").is_none())
        );
        assert!(
            state::prompt_size(&messages, &calls[3].1["tools"])
                <= state::input_budget(65_536, 2048)
        );
    }
    let mut requests = 0;
    while let Some(event) = events.recv().await {
        if let ProviderEvent::Metrics(stats) = event {
            requests += stats.requests;
        }
    }
    assert_eq!(requests, 3);
    let handoff = std::fs::read_dir(checkpoint.with_file_name("handoffs"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap();
    assert!(
        std::fs::read_to_string(handoff.path())
            .unwrap()
            .contains(&"PROJECT_DATA ".repeat(7000))
    );

    let (endpoint, captured, handle) = server(vec![
        ("/api/show", json!({"capabilities":["tools"]}).to_string()),
        ("/api/chat", text_reply("Resumed.")),
    ]);
    let mut next = project_request(endpoint);
    next.checkpoint = Some(checkpoint);
    next.workspace = workspace;
    next.messages[0].content = "Continue with the next task.".into();
    let (sender, _events) = mpsc::channel(128);
    let (_cancel, receiver) = watch::channel(false);
    stream_chat(next, sender, receiver).await.unwrap();
    handle.join().unwrap();
    let calls = captured.lock().unwrap();
    assert!(
        calls[1].1["messages"]
            .to_string()
            .contains("CHECKPOINT_MARKER")
    );
    assert!(
        calls[1].1["messages"]
            .to_string()
            .contains("Continue with the next task.")
    );
}

#[tokio::test]
async fn ainsley_16_reloads_compacted_markdown_and_keeps_newer_messages() {
    use crate::domain::{Provider, Session};

    let show = json!({"capabilities":["thinking"]}).to_string();
    let (endpoint, captured, handle) = server(vec![
        ("/api/show", show.clone()),
        (
            "/api/chat",
            text_reply("## Work completed\n\nProject decision: RECALL_MARKER"),
        ),
        ("/api/chat", text_reply("Recent answer.")),
    ]);
    let mut session = Session::default();
    session.new_chat();
    let chat = session.active_mut().unwrap();
    chat.set_provider(Provider::Ollama, "Aisnley-Ox-1.6-bbb:35b".into());
    chat.draft = "Original project decision".into();
    let run = session.prepare_selected_run(None).unwrap();
    session.run_message_mut(run).unwrap().content = "OLD_TRANSCRIPT ".repeat(10_000);
    session.finish_run(run, MessageStatus::Complete, None);
    session.active_mut().unwrap().draft = "Recent question".into();
    let settings = Settings {
        endpoint,
        context_tokens: 8192,
        ..Settings::default()
    };
    let run = session.prepare_selected_run(None).unwrap();
    let request = Request::from_chat(session.active().unwrap(), settings.clone());
    let checkpoint = request.checkpoint.clone().unwrap();
    let (sender, _events) = mpsc::channel(64);
    let (_cancel, receiver) = watch::channel(false);
    stream_chat(request, sender, receiver).await.unwrap();
    handle.join().unwrap();
    assert!(
        captured.lock().unwrap()[1].1["messages"]
            .to_string()
            .contains("Original project decision")
    );
    assert!(
        std::fs::read_to_string(checkpoint)
            .unwrap()
            .contains("RECALL_MARKER")
    );
    session.run_message_mut(run).unwrap().content = "Recent answer.".into();
    session.finish_run(run, MessageStatus::Complete, None);

    let saved = serde_json::to_vec(&session).unwrap();
    let mut restarted: Session = serde_json::from_slice(&saved).unwrap();
    restarted.recover();
    restarted.active_mut().unwrap().draft = "What did we decide?".into();
    restarted.prepare_selected_run(None).unwrap();
    let (endpoint, captured, handle) = server(vec![
        ("/api/show", show),
        ("/api/chat", text_reply("RECALL_MARKER")),
    ]);
    let request = Request::from_chat(
        restarted.active().unwrap(),
        Settings {
            endpoint,
            ..settings
        },
    );
    let (sender, mut events) = mpsc::channel(64);
    let (_cancel, receiver) = watch::channel(false);
    stream_chat(request, sender, receiver).await.unwrap();
    handle.join().unwrap();
    let calls = captured.lock().unwrap();
    let body = &calls[1].1;
    assert_eq!(body["options"]["num_ctx"], 8192);
    let messages = body["messages"].to_string();
    assert!(messages.contains("RECALL_MARKER"));
    assert!(messages.contains("Recent question") && messages.contains("Recent answer."));
    assert!(messages.contains("What did we decide?"));
    assert!(!messages.contains("OLD_TRANSCRIPT"));
    let mut context = None;
    while let Some(event) = events.recv().await {
        if let ProviderEvent::Context(usage) = event {
            context = Some(usage);
        }
    }
    assert_eq!(context.unwrap().compressions, 1);
}

#[tokio::test]
async fn token_limited_output_continues_without_executing_incomplete_tools() {
    let partial = json!({"message":{"content":"Work in progress", "tool_calls":[{"function":{"name":"write_file","arguments":{"path":"never.txt","content":"unfinished"}}}]},"done":true,"done_reason":"length"});
    let (endpoint, captured, handle) = server(vec![
        ("/api/show", json!({"capabilities":["tools"]}).to_string()),
        ("/api/chat", format!("{partial}\n")),
        ("/api/chat", text_reply("Now finished.")),
    ]);
    let mut request = project_request(endpoint);
    request.mode = Mode::FullAccess;
    let root = request.workspace.clone().unwrap();
    let (sender, _events) = mpsc::channel(128);
    let (_cancel, receiver) = watch::channel(false);
    stream_chat(request, sender, receiver).await.unwrap();
    handle.join().unwrap();

    assert!(!root.join("never.txt").exists());
    let calls = captured.lock().unwrap();
    assert!(
        calls[2].1["messages"]
            .to_string()
            .contains("Continue the unfinished task")
    );
    assert!(
        calls[2].1["messages"]
            .as_array()
            .unwrap()
            .iter()
            .all(|message| message.get("tool_calls").is_none())
    );
}

#[tokio::test]
async fn failed_context_summary_continues_from_the_saved_action_results() {
    let mut responses = vec![
        ("/api/show", json!({"capabilities":["tools"]}).to_string()),
        ("/api/chat", read_reply(1)),
    ];
    responses.extend((0..4).map(|_| ("/api/chat", text_reply(""))));
    responses.push(("/api/chat", text_reply("Continued from saved steps.")));
    let (endpoint, captured, handle) = server(responses);
    let request = project_request(endpoint);
    std::fs::write(
        request.workspace.as_ref().unwrap().join("note.txt"),
        "RECOVERABLE_DATA ".repeat(4000),
    )
    .unwrap();
    let checkpoint = request.checkpoint.clone().unwrap();
    let (sender, _events) = mpsc::channel(128);
    let (_cancel, receiver) = watch::channel(false);
    stream_chat(request, sender, receiver).await.unwrap();
    handle.join().unwrap();

    let saved = std::fs::read_to_string(checkpoint).unwrap();
    assert!(saved.contains("Tool result: read_file"));
    assert!(saved.contains("RECOVERABLE_DATA"));
    assert!(saved.contains("Status: complete"));
    let calls = captured.lock().unwrap();
    assert!(calls[2..6].iter().all(|(_, body)| body["stream"] == false));
    assert_eq!(calls[6].1["stream"], true);
    assert!(
        calls[6].1["messages"]
            .to_string()
            .contains("RECOVERABLE_DATA")
    );
}

#[tokio::test]
async fn cancellation_keeps_the_pending_action_for_a_later_resume() {
    let call = json!({"message":{"tool_calls":[{"function":{"name":"write_file","arguments":{"path":"pending.txt","content":"not approved"}}}]},"done":true});
    let (endpoint, _, handle) = server(vec![
        ("/api/show", json!({"capabilities":["tools"]}).to_string()),
        ("/api/chat", format!("{call}\n")),
    ]);
    let mut request = project_request(endpoint);
    request.mode = Mode::Code;
    let original_task = "Write pending.txt only after approval.";
    request.messages[0].content = original_task.into();
    let checkpoint = request.checkpoint.clone().unwrap();
    let root = request.workspace.clone().unwrap();
    let (sender, mut events) = mpsc::channel(128);
    let (cancel, receiver) = watch::channel(false);
    let observe = async {
        while let Some(event) = events.recv().await {
            if let ProviderEvent::Approval(approval) = event {
                cancel.send(true).unwrap();
                let _pending_approval = approval;
                std::future::pending::<()>().await;
            }
        }
    };
    tokio::pin!(observe);
    let result = tokio::select! {
        result = stream_chat(request, sender, receiver) => result,
        _ = &mut observe => panic!("No approval requested"),
    };
    assert_eq!(result.unwrap_err(), "Cancelled.");
    handle.join().unwrap();
    assert!(!root.join("pending.txt").exists());
    let saved = std::fs::read_to_string(&checkpoint).unwrap();
    assert!(saved.contains("Starting tool 1: write_file"));
    assert!(saved.contains("Status: in progress"));
    assert!(!saved.contains("Tool result: write_file"));
    let (endpoint, captured, handle) = server(vec![
        ("/api/show", json!({"capabilities":["tools"]}).to_string()),
        ("/api/chat", text_reply("Checked the saved task.")),
    ]);
    let mut resumed = project_request(endpoint);
    resumed.checkpoint = Some(checkpoint);
    resumed.workspace = Some(root);
    resumed.messages[0].content = "Continue".into();
    let (sender, _events) = mpsc::channel(128);
    let (_cancel, receiver) = watch::channel(false);
    stream_chat(resumed, sender, receiver).await.unwrap();
    handle.join().unwrap();
    let calls = captured.lock().unwrap();
    let messages = calls[1].1["messages"].as_array().unwrap();
    assert_eq!(messages.last().unwrap()["content"], original_task);
    assert!(
        messages[1]["content"]
            .as_str()
            .unwrap()
            .contains("pending.txt")
    );
}

#[tokio::test]
async fn discovery_and_fragmented_streams_use_native_ollama_routes_and_record_usage() {
    let body = "{\"message\":{\"content\":\"Hé\"},\"done\":false}\n{\"message\":{\"content\":\"llo\"},\"done\":false}\n{\"message\":{\"content\":\"\"},\"done\":true,\"prompt_eval_count\":7,\"eval_count\":3,\"eval_duration\":1000000000}";
    let (endpoint, captured, handle) = server(vec![
        (
            "/api/tags",
            json!({"models":[{"name":"qwen-test:latest"}]}).to_string(),
        ),
        (
            "/api/show",
            json!({"capabilities":["completion"]}).to_string(),
        ),
        ("/api/chat", body.into()),
    ]);
    assert_eq!(
        list_models(&endpoint).await.unwrap()[0].name,
        "qwen-test:latest"
    );
    let (sender, mut events) = mpsc::channel(16);
    let (_cancel, receiver) = watch::channel(false);
    let mut local_request = request(endpoint, false);
    local_request.settings.context_tokens = 262_144;
    local_request.settings.cpu_only = true;
    stream_chat(local_request, sender, receiver).await.unwrap();
    let mut text = String::new();
    let mut usage = Usage::default();
    while let Some(event) = events.recv().await {
        match event {
            ProviderEvent::Content(content) => text.push_str(&content),
            ProviderEvent::Metrics(stats) => usage.record(stats),
            ProviderEvent::Context(_) | ProviderEvent::OllamaStage(_) => {}
            _ => panic!("Unexpected provider event"),
        }
    }
    assert_eq!(text, "Héllo");
    assert_eq!(usage.prompt_tokens, 7);
    assert_eq!(usage.output_tokens, 3);
    assert_eq!(usage.last.eval_seconds, 1.0);
    handle.join().unwrap();
    let requests = captured.lock().unwrap();
    assert_eq!(requests[2].1["model"], "qwen-test:latest");
    assert_eq!(requests[2].1["messages"][1]["role"], "user");
    assert!(requests[2].1.get("tools").is_none());
    assert_eq!(requests[2].1["options"]["num_gpu"], 0);
    assert_eq!(requests[2].1["options"]["num_ctx"], 262_144);
}

#[tokio::test]
async fn local_agent_executes_a_project_tool_and_supplies_its_result_to_the_model() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("temp/verification/ollama-stream")
        .join(uuid::Uuid::new_v4().to_string());
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("note.txt"), "PROJECT_DATA_123").unwrap();
    let call = json!({"message":{"role":"assistant","content":"","tool_calls":[{"function":{"name":"read_file","arguments":{"path":"note.txt"}}}]},"done":true,"eval_count":2});
    let final_reply = json!({"message":{"content":"Read the project."},"done":true,"eval_count":4});
    let (endpoint, captured, handle) = server(vec![
        ("/api/show", json!({"capabilities":["tools"]}).to_string()),
        ("/api/chat", format!("{call}\n")),
        ("/api/chat", format!("{final_reply}\n")),
    ]);
    let mut request = request(endpoint, true);
    request.workspace = Some(root);
    let (sender, mut events) = mpsc::channel(16);
    let (_cancel, receiver) = watch::channel(false);
    stream_chat(request, sender, receiver).await.unwrap();
    handle.join().unwrap();
    {
        let calls = captured.lock().unwrap();
        let messages = calls[2].1["messages"].as_array().unwrap();
        assert!(
            messages.iter().any(
                |message| message["role"] == "tool" && message["content"] == "PROJECT_DATA_123"
            )
        );
    }
    let mut stats = None;
    while let Some(event) = events.recv().await {
        if let ProviderEvent::Metrics(value) = event {
            stats = Some(value);
        }
    }
    assert_eq!(stats.unwrap().output_tokens, 6);
}

#[tokio::test]
async fn exhausted_stream_retries_keep_partial_text_and_pre_cancel_does_not_connect() {
    let mut responses = vec![
        ("/api/show", json!({"capabilities":[]}).to_string()),
        (
            "/api/chat",
            "{\"message\":{\"content\":\"partial\"},\"done\":false}\n".into(),
        ),
    ];
    responses.extend((0..3).map(|_| ("/api/chat", String::new())));
    let (endpoint, _, handle) = server(responses);
    let (sender, mut events) = mpsc::channel(16);
    let (_cancel, receiver) = watch::channel(false);
    assert!(
        stream_chat(request(endpoint, false), sender, receiver)
            .await
            .unwrap_err()
            .contains("failed after 4 attempts")
    );
    let mut chunks = Vec::new();
    while let Some(event) = events.recv().await {
        match event {
            ProviderEvent::Content(text) => chunks.push(text),
            ProviderEvent::Status(_)
            | ProviderEvent::Context(_)
            | ProviderEvent::OllamaStage(_) => {}
            _ => panic!("Unexpected event after retry exhaustion"),
        }
    }
    assert_eq!(chunks, ["partial"]);
    handle.join().unwrap();
    let (sender, _) = mpsc::channel(16);
    let (_cancel, receiver) = watch::channel(true);
    assert_eq!(
        stream_chat(
            request("http://127.0.0.1:1".into(), false),
            sender,
            receiver
        )
        .await
        .unwrap_err(),
        "Cancelled."
    );
}

#[tokio::test]
async fn retries_only_the_failed_step_and_replaces_its_partial_output() {
    let write = json!({"message":{"content":"Writing the file.","tool_calls":[{
        "function":{"name":"write_file","arguments":{"path":"done.txt","content":"saved"}}
    }]},"done":true});
    let abandoned_call = json!({"message":{"content":"Stale draft.","tool_calls":[{
        "function":{"name":"write_file","arguments":{"path":"never.txt","content":"unfinished"}}
    }]},"done":false});
    let (endpoint, captured, handle) = server_with_status(vec![
        ("/api/show", 503, json!({"error":"starting"}).to_string()),
        (
            "/api/show",
            200,
            json!({"capabilities":["tools"]}).to_string(),
        ),
        ("/api/chat", 200, format!("{write}\n")),
        (
            "/api/chat",
            200,
            format!("{abandoned_call}\n{{\"error\":\"runner stopped\"}}\n"),
        ),
        ("/api/chat", 200, text_reply("File saved.")),
    ]);
    let mut request = project_request(endpoint);
    request.mode = Mode::FullAccess;
    let root = request.workspace.clone().unwrap();
    let checkpoint = request.checkpoint.clone().unwrap();
    let (sender, mut events) = mpsc::channel(32);
    let (_cancel, receiver) = watch::channel(false);
    stream_chat(request, sender, receiver).await.unwrap();
    handle.join().unwrap();

    let mut session = crate::domain::Session::default();
    session.new_chat();
    let chat = session.active_mut().unwrap();
    chat.set_provider(crate::domain::Provider::Ollama, "qwen-test".into());
    chat.draft = "Write a file".into();
    let run = session.prepare_run().unwrap();
    let mut retries = 0;
    while let Some(event) = events.recv().await {
        let update = match event {
            ProviderEvent::Content(content) => Update::Content(content),
            ProviderEvent::ReplaceContent { offset, text } => {
                Update::ReplaceContent { offset, text }
            }
            ProviderEvent::Status(status) if status.contains("retrying") => {
                retries += 1;
                continue;
            }
            _ => continue,
        };
        apply_event(&mut session, Some(run), RunEvent { run, update });
    }
    assert_eq!(
        session.run_message_mut(run).unwrap().content,
        "Writing the file.\n\nFile saved."
    );
    assert_eq!(retries, 2);
    assert_eq!(
        std::fs::read_to_string(root.join("done.txt")).unwrap(),
        "saved"
    );
    assert!(!root.join("never.txt").exists());
    let journal = state::Journal::open(checkpoint, Some(root), "Resume".into(), false)
        .await
        .unwrap();
    assert_eq!(
        journal
            .progress
            .context()
            .matches("Tool result: write_file")
            .count(),
        1
    );
    let calls = captured.lock().unwrap();
    assert_eq!(calls[3].1, calls[4].1);
}

#[tokio::test]
async fn saffyos_loop_recovers_model_lookup_and_retries_only_the_failed_step() {
    let read = json!({"message":{"tool_calls":[{
        "function":{"name":"read_file","arguments":{"path":"note.txt"}}
    }]},"done":true});
    let mut responses = vec![("/api/show", 503, "{\"error\":\"offline\"}".into()); 4];
    responses.push((
        "/api/show",
        200,
        json!({"capabilities":["tools"]}).to_string(),
    ));
    responses.push(("/api/chat", 200, format!("{read}\n")));
    responses.extend(vec![
        (
            "/api/chat",
            200,
            "{\"error\":\"runner stopped\"}\n".into()
        );
        4
    ]);
    responses.push(("/api/chat", 200, text_reply("Recovered.")));
    let (endpoint, captured, handle) = server_with_status(responses);

    let folder = tempfile::tempdir().unwrap();
    std::fs::write(folder.path().join("note.txt"), "saved tool evidence").unwrap();
    let mut request = request(endpoint, false);
    request.saffyos_locked = true;
    request.saffyos_folder = Some(folder.path().canonicalize().unwrap());
    let (sender, mut events) = mpsc::channel(64);
    let (_cancel, receiver) = watch::channel(false);
    stream_chat(request.with_saffyos_loop_recovery(), sender, receiver)
        .await
        .unwrap();
    handle.join().unwrap();

    let mut recoveries = 0;
    while let Some(event) = events.recv().await {
        if matches!(event, ProviderEvent::Status(status) if status.contains("auto-loop will retry"))
        {
            recoveries += 1;
        }
    }
    assert_eq!(recoveries, 2);
    let requests = captured.lock().unwrap();
    assert!(requests[6].1["messages"]
        .as_array().unwrap().iter()
        .any(|message| message["role"] == "tool" && message["content"] == "saved tool evidence"));
    assert!(
        requests[6..]
            .iter()
            .all(|request| request.1 == requests[6].1)
    );
}

#[tokio::test]
async fn stop_cancels_retry_backoff() {
    let (endpoint, captured, handle) = server(vec![
        ("/api/show", json!({"capabilities":[]}).to_string()),
        ("/api/chat", String::new()),
    ]);
    let (sender, mut events) = mpsc::channel(16);
    let (cancel, receiver) = watch::channel(false);
    let worker = tokio::spawn(stream_chat(request(endpoint, false), sender, receiver));

    loop {
        let event = events.recv().await.expect("Ollama did not retry");
        if matches!(event, ProviderEvent::Status(status) if status.contains("retrying")) {
            break;
        }
    }
    cancel.send(true).unwrap();
    assert_eq!(worker.await.unwrap().unwrap_err(), "Cancelled.");
    handle.join().unwrap();
    assert_eq!(captured.lock().unwrap().len(), 2);
}

#[tokio::test]
async fn permanent_http_error_returns_without_retrying() {
    let (endpoint, _, handle) = server_with_status(vec![
        ("/api/show", 200, json!({"capabilities":[]}).to_string()),
        (
            "/api/chat",
            404,
            json!({"error":"model not found"}).to_string(),
        ),
    ]);
    let (sender, mut events) = mpsc::channel(16);
    let (_cancel, receiver) = watch::channel(false);
    let error = stream_chat(request(endpoint, false), sender, receiver)
        .await
        .unwrap_err();
    handle.join().unwrap();
    assert!(error.contains("model not found"));
    while let Some(event) = events.recv().await {
        assert!(matches!(
            event,
            ProviderEvent::Context(_) | ProviderEvent::OllamaStage(_)
        ));
    }
}

#[cfg(windows)]
#[tokio::test]
async fn folderless_ollama_desktop_input_uses_shared_approval() {
    let call = json!({"message":{"role":"assistant","tool_calls":[{
        "function":{"name":"redox_remote_control","arguments":{"action":"keypress","keys":["ESC"]}}
    }]},"done":true});
    let reply = json!({"message":{"content":"Desktop input was declined."},"done":true});
    let (endpoint, captured, handle) = server(vec![
        (
            "/api/show",
            json!({"capabilities":["vision","tools"]}).to_string(),
        ),
        ("/api/chat", format!("{call}\n")),
        ("/api/chat", format!("{reply}\n")),
    ]);
    let mut request = request(endpoint, false);
    request.desktop.enabled = true;
    let activity = request.desktop_activity.clone();
    let (sender, mut events) = mpsc::channel(16);
    let (_cancel, receiver) = watch::channel(false);
    let worker = tokio::spawn(stream_chat(request, sender, receiver));
    while let Some(event) = events.recv().await {
        if let ProviderEvent::Approval(approval) = event {
            approval.response.send(false).unwrap();
        }
    }
    worker.await.unwrap().unwrap();
    handle.join().unwrap();
    let requests = captured.lock().unwrap();
    let tools = requests[1].1["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0]["function"]["name"], "redox_remote_control");
    assert!(
        requests[2].1["messages"]
            .as_array()
            .unwrap()
            .iter()
            .any(|message| {
                message["role"] == "tool"
                    && message["content"]
                        .as_str()
                        .unwrap()
                        .contains("not approved")
            })
    );
    assert!(!activity.snapshot().active);
}

#[test]
fn desktop_screenshot_is_an_ollama_tool_message_with_an_image() {
    let message = desktop_tool_message(Ok(vec![
        json!({"type":"inputText","text":"Origin (-1920, 0)"}),
        json!({"type":"inputImage","imageUrl":"data:image/jpeg;base64,YWJj"}),
    ]));
    assert_eq!(message["role"], "tool");
    assert_eq!(message["tool_name"], "redox_remote_control");
    assert_eq!(message["content"], "Origin (-1920, 0)");
    assert_eq!(message["images"], json!(["YWJj"]));
}

#[test]
fn local_chats_survive_restart_and_switching_provider_starts_a_fresh_codex_thread() {
    let mut session = crate::domain::Session::default();
    session.new_chat();
    let chat = session.active_mut().unwrap();
    chat.thread_id = Some("old-codex-thread".into());
    chat.set_provider(crate::domain::Provider::Ollama, "qwen3.5:9b".into());
    chat.draft = "Offload this request".into();
    assert!(chat.thread_id.is_none());
    let id = session.prepare_run().unwrap();
    assert!(
        session
            .run_message_mut(id)
            .unwrap()
            .model
            .starts_with("Ollama ·")
    );
    let saved = serde_json::to_vec(&session).unwrap();
    let mut restored: crate::domain::Session = serde_json::from_slice(&saved).unwrap();
    restored.recover();
    assert_eq!(restored.active().unwrap().ollama_model, "qwen3.5:9b");
    assert_eq!(
        restored.active().unwrap().provider,
        crate::domain::Provider::Ollama
    );
    assert_eq!(
        restored.active().unwrap().messages.last().unwrap().status,
        MessageStatus::Interrupted
    );
    assert!(api_url("file:///project", "chat").is_err());
    assert_eq!(
        api_url("http://localhost:11434/api/", "chat")
            .unwrap()
            .path(),
        "/api/chat"
    );
}

#[tokio::test]
#[ignore = "uses an installed local Ollama model; no model download or Codex request"]
async fn live_local_model_smoke_test() {
    let model = std::env::var("REDOX_OLLAMA_TEST_MODEL").expect("Set REDOX_OLLAMA_TEST_MODEL");
    let mut request = request("http://127.0.0.1:11434".into(), false);
    request.model = model;
    request.settings.output_tokens = 16;
    request.settings.cpu_only =
        std::env::var("REDOX_OLLAMA_TEST_CPU").is_ok_and(|value| value == "1");
    request.messages[0].content = "Reply with exactly LOCAL_OK".into();
    let (sender, mut events) = mpsc::channel(128);
    let (_cancel, receiver) = watch::channel(false);
    tokio::time::timeout(
        Duration::from_secs(120),
        stream_chat(request, sender, receiver),
    )
    .await
    .unwrap()
    .unwrap();
    let mut text = String::new();
    let mut stats = None;
    while let Some(event) = events.recv().await {
        match event {
            ProviderEvent::Content(content) => text.push_str(&content),
            ProviderEvent::Metrics(value) => stats = Some(value),
            _ => {}
        }
    }
    assert!(text.contains("LOCAL_OK"), "{text}");
    let stats = stats.expect("No usage metrics");
    assert!(stats.output_tokens > 0);
    println!(
        "Local Ollama replied {text}; {} output tokens",
        stats.output_tokens
    );
}

#[tokio::test]
#[ignore = "uses an installed local Ollama model to read a project fixture and roll over context"]
async fn live_local_project_read_test() {
    let model = std::env::var("REDOX_OLLAMA_TEST_MODEL").expect("Set REDOX_OLLAMA_TEST_MODEL");
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("temp/verification/ollama-live-project")
        .join(uuid::Uuid::new_v4().to_string());
    std::fs::create_dir_all(&root).unwrap();
    let proof = format!("release_channel=canary-{}", uuid::Uuid::new_v4());
    std::fs::write(
        root.join("proof.txt"),
        format!("{}{proof}\n", "Fixture data.\n".repeat(5_000)),
    )
    .unwrap();
    let mut request = request("http://127.0.0.1:11434".into(), true);
    request.model = model;
    request.workspace = Some(root);
    request.settings.cpu_only =
        std::env::var("REDOX_OLLAMA_TEST_CPU").is_ok_and(|value| value == "1");
    request.settings.context_tokens = 64_000;
    request.settings.output_tokens = 256;
    request.messages[0].content = "Read proof.txt in the selected project using read_file. Return only the exact release_channel= line. It is at the end of the file. Once the recorded result contains that line, use it without rereading the file.".into();
    let (sender, mut events) = mpsc::channel(256);
    let (_cancel, receiver) = watch::channel(false);
    tokio::time::timeout(
        Duration::from_secs(480),
        stream_chat(request, sender, receiver),
    )
    .await
    .unwrap()
    .unwrap();
    let mut text = String::new();
    let mut saw_read = false;
    let mut read_after_rollover = false;
    let mut rolled_over = false;
    let mut stats = None;
    while let Some(event) = events.recv().await {
        match event {
            ProviderEvent::Content(content) => text.push_str(&content),
            ProviderEvent::Status(status) => {
                if status.contains("read_file") {
                    saw_read = true;
                    read_after_rollover |= rolled_over;
                }
                rolled_over |= status.contains("compacting context");
            }
            ProviderEvent::Metrics(value) => stats = Some(value),
            _ => {}
        }
    }
    assert!(saw_read, "The model did not use the file tool");
    assert!(rolled_over, "The fixture did not trigger context rollover");
    assert!(
        !read_after_rollover,
        "The model reread the fixture after rollover"
    );
    assert!(
        text.contains(&proof),
        "The model did not return the actual file content: {text}"
    );
    println!(
        "Local Ollama read a random project fixture and recalled its final line after rollover ({} generated tokens)",
        stats.unwrap().output_tokens
    );
}

#[tokio::test]
#[ignore = "uses the installed Ainsley 1.5 Ollama model to inspect a Rust fixture"]
async fn live_ainsley_1_5_diagnoses_a_stalled_rust_loop() {
    let mut request = project_request("http://127.0.0.1:11434".into());
    request.model = std::env::var("REDOX_OLLAMA_TEST_MODEL")
        .unwrap_or_else(|_| "Aisnley-Ox-1.5-bbb:exp-loop-check".into());
    std::fs::write(
        request.workspace.as_ref().unwrap().join("step.rs"),
        "pub fn count_steps(mut x: u32) -> u32 { let mut steps = 0; while x > 0 { if x > 1 { x -= 1; } steps += 1; } steps }",
    )
    .unwrap();
    request.messages[0].content = "Read step.rs with read_file. Explain which value of x stops the loop making progress, and describe the smallest fix. Read only.".into();
    let (sender, mut events) = mpsc::channel(64);
    let (_cancel, receiver) = watch::channel(false);
    let collect = async {
        let mut answer = String::new();
        let mut read = false;
        while let Some(event) = events.recv().await {
            match event {
                ProviderEvent::Content(text) => answer.push_str(&text),
                ProviderEvent::ReplaceContent { offset, text } => {
                    answer.truncate(offset);
                    answer.push_str(&text);
                }
                ProviderEvent::Status(status) => read |= status.contains("read_file"),
                _ => {}
            }
        }
        (answer, read)
    };
    let (result, (answer, read)) = tokio::time::timeout(Duration::from_secs(180), async {
        tokio::join!(stream_chat(request, sender, receiver), collect)
    })
    .await
    .unwrap();
    result.unwrap();
    assert!(read, "Ainsley did not read the Rust fixture");
    assert!(answer.contains('1'), "{answer}");
    assert!(
        [
            "advance",
            "decrement",
            "terminat",
            "infinite",
            "stuck",
            "progress"
        ]
        .iter()
        .any(|word| answer.to_ascii_lowercase().contains(word)),
        "{answer}"
    );
    println!("{answer}");
}

#[tokio::test]
#[ignore = "uses the local Ainsley 1.5 model to edit a disposable Rust fixture"]
async fn live_ainsley_1_5_applies_an_exact_rust_edit() {
    let mut request = project_request("http://127.0.0.1:11434".into());
    request.model = std::env::var("REDOX_OLLAMA_TEST_MODEL")
        .unwrap_or_else(|_| "Aisnley-Ox-1.5-bbb:exp-invariants".into());
    request.mode = Mode::Code;
    request.settings.context_tokens = 8192;
    let root = request.workspace.as_ref().unwrap();
    let path = root.join("range.rs");
    let source = std::env::var_os("REDOX_OLLAMA_REPAIR_SOURCE")
        .map(|path| std::fs::read_to_string(path).unwrap())
        .unwrap_or_else(|| {
            "pub fn distance(lo: i64, hi: i64) -> u64 { (hi - lo) as u64 }\n".into()
        });
    std::fs::write(&path, &source).unwrap();
    println!("Rust fixture in {}", path.display());
    std::fs::write(root.join("AGENTS.md"), "This is a disposable Rust test project. Edit range.rs only. The test harness runs the verification.\n").unwrap();
    let authorized_path = path.canonicalize().unwrap();
    request.messages[0].content = "Read range.rs. Its distance function assumes lo <= hi. It panics in a debug build for i64::MIN through i64::MAX. Use edit_file to fix the expression while preserving the signature. Only change range.rs. The harness will run the tests, so no shell commands are needed.".into();
    if let Ok(diagnostic) = std::env::var("REDOX_OLLAMA_REPAIR_FEEDBACK") {
        request.messages[0].content.push_str(&format!(
            "\nThe previous edit was compiled and tested. It still failed this runtime check.\n{diagnostic}\nThe expected result for those actual operands is u64::MAX. Use the failure output to make one exact arithmetic edit, then report the changed expression so the harness can test it."
        ));
    }
    let guidance = std::env::var("REDOX_OLLAMA_REPAIR_GUIDANCE").ok();
    if let Some(guidance) = &guidance {
        request.messages[0].content = format!(
            "Read range.rs and use edit_file to fix distance. Its full-width boundary test panicked on signed arithmetic. {guidance} Preserve the signature and edit only range.rs. After the tool confirms the edit, reply READY_FOR_TEST. The harness runs the test."
        );
    }
    let (sender, mut events) = mpsc::channel(64);
    let (_cancel, receiver) = watch::channel(false);
    let collect = async {
        let mut approved = false;
        let mut answer = String::new();
        while let Some(event) = events.recv().await {
            match event {
                ProviderEvent::Approval(approval) => {
                    let matching = approval
                        .description
                        .starts_with("Ollama requests edit_file\n")
                        && approval.description.lines().nth(1) == authorized_path.to_str()
                        && (guidance.is_none() || !approved);
                    approved |= matching;
                    let _ = approval.response.send(matching);
                }
                ProviderEvent::Content(text) => answer.push_str(&text),
                ProviderEvent::ReplaceContent { offset, text } => {
                    answer.truncate(offset);
                    answer.push_str(&text);
                }
                _ => {}
            }
        }
        (approved, answer)
    };
    let (result, (approved, answer)) = tokio::time::timeout(Duration::from_secs(240), async {
        tokio::join!(stream_chat(request, sender, receiver), collect)
    })
    .await
    .unwrap();
    result.unwrap();
    assert!(approved, "No exact edit to the Rust fixture was requested.");
    assert_ne!(std::fs::read_to_string(&path).unwrap(), source);
    if guidance.is_some() {
        assert!(answer.contains("READY_FOR_TEST"), "{answer}");
    }
    println!("Verified an edit in {}", path.display());
}

#[test]
fn borrowed_chat_request_preserves_the_wire_format() {
    let messages = [json!({"role":"user","content":"test","images":["image data"]})];
    let tools = json!([{"type":"function"}]);
    let mut body = ChatBody {
        model: "mock",
        messages: &messages,
        stream: true,
        options: ChatOptions {
            num_ctx: 1024,
            num_predict: 128,
            num_gpu: Some(0),
        },
        keep_alive: Some(90),
        tools: Some(&tools),
        think: Some(false),
    };
    assert_eq!(
        serde_json::to_value(&body).unwrap(),
        json!({
            "model":"mock", "messages":messages, "stream":true,
            "options":{"num_ctx":1024,"num_predict":128,"num_gpu":0},
            "keep_alive":90, "tools":tools, "think":false
        })
    );
    body.options.num_gpu = None;
    body.keep_alive = None;
    body.tools = None;
    body.think = None;
    assert_eq!(
        serde_json::to_value(&body).unwrap(),
        json!({
            "model":"mock", "messages":messages, "stream":true,
            "options":{"num_ctx":1024,"num_predict":128}
        })
    );
}

#[tokio::test]
#[ignore = "requires the local Ainsley 1.6 Ollama model"]
async fn ainsley_16_live_chat_state_recall_and_edit() {
    const MODEL: &str = "Aisnley-Ox-1.6-bbb:35b";
    let comparison = std::env::var("REDOX_BENCH_MODEL").ok();
    let model = comparison.as_deref().unwrap_or(MODEL);
    let gated = std::env::var_os("REDOX_BENCH_GATE").is_some();
    eprintln!(
        "Model: {model}; normal startup route: {}",
        gated || comparison.is_none()
    );
    let mut request = project_request(Settings::default().endpoint);
    request.model = model.into();
    request.mode = Mode::FullAccess;
    request.settings.context_tokens = 8192;
    request.settings.keep_alive = Some(300);
    let root = request.workspace.as_ref().unwrap().clone();
    let checkpoint = request.checkpoint.clone().unwrap();
    std::fs::write(
        root.join("main.rs"),
        r#"fn main() {
    let command = std::env::args().nth(1);
    match command.as_deref() {
        Some("ping") => println!("pong"),
        _ => println!("Usage: ping"),
    }
}
"#,
    )
    .unwrap();
    request.messages.clear();

    for (prompt, expected) in [
        (
            "What state are you in? Tell me your exact model, access mode and active context window. Answer from ReDox state without tools.",
            &[model, "Full access", "8192"][..],
        ),
        (
            "Run the command `echo RECALL_6713` in the project and give me its output.",
            &["RECALL_6713"],
        ),
        (
            "What exact command did I ask you to run in my previous message? Answer from our conversation without running it again.",
            &["echo RECALL_6713"],
        ),
        (
            "Implement a new CLI command `triple` in main.rs. Preserve ping. Add a separate fn triple(value: i32) -> i32, accept an integer argument and print its tripled value, and update usage. Use rustc to compile main.rs to demo.exe and run demo.exe triple 14 to verify. Report the observed output.",
            &["42"],
        ),
    ] {
        request.messages.push(InputMessage {
            id: None,
            role: "user",
            content: prompt.into(),
            images: Vec::new(),
        });
        let turn = Request {
            settings: request.settings.clone(),
            model: request.model.clone(),
            workspace: request.workspace.clone(),
            mode: request.mode,
            desktop: Default::default(),
            desktop_activity: Default::default(),
            messages: request.messages.clone(),
            checkpoint: request.checkpoint.clone(),
            worker: None,
            saffyos_locked: false,
            saffyos_folder: None,
            recover_saffyos_loop: false,
        };
        let (sender, mut events) = mpsc::channel(64);
        let (_stop, cancel) = watch::channel(false);
        let mut answer = String::new();
        let mut tools = Vec::new();
        let started = std::time::Instant::now();
        let mut stats = Stats::default();
        let run = async {
            if gated || comparison.is_none() {
                stream_qualified_chat(turn, sender, cancel).await
            } else {
                stream_chat(turn, sender, cancel).await
            }
        };
        let (result, ()) = tokio::time::timeout(Duration::from_secs(180), async {
            tokio::join!(run, async {
                while let Some(event) = events.recv().await {
                    match event {
                        ProviderEvent::Content(delta) => answer.push_str(&delta),
                        ProviderEvent::ReplaceContent { offset, text } => {
                            answer.truncate(offset);
                            answer.push_str(&text);
                        }
                        ProviderEvent::Status(status) => {
                            if gated {
                                eprintln!("{:.2}s {status}", started.elapsed().as_secs_f64());
                            }
                            tools.push(status);
                        }
                        ProviderEvent::Metrics(metrics) => stats = metrics,
                        _ => {}
                    }
                }
            })
        })
        .await
        .unwrap_or_else(|_| (Err("180-second turn timeout".into()), ()));
        let expected_text = expected.iter().all(|value| {
            answer
                .to_ascii_lowercase()
                .replace(',', "")
                .contains(&value.to_ascii_lowercase())
        });
        let tool_count = tools
            .iter()
            .filter(|status| {
                [
                    "read_file",
                    "edit_file",
                    "write_file",
                    "list_files",
                    "run_command",
                    "make_directory",
                ]
                .iter()
                .any(|name| status.ends_with(&format!(": {name}")))
            })
            .count();
        let valid_tools = (!prompt.contains("without") || tool_count == 0)
            && (!(prompt.starts_with("Run the command") || prompt.starts_with("Implement"))
                || tools.iter().any(|status| status.ends_with(": run_command")));
        let passed = result.is_ok() && expected_text && valid_tools;
        eprintln!(
            "Turn passed={passed} wall={:.2}s model_calls={} prompt_tokens={} output_tokens={} tool_calls={tool_count} error={:?}\n{prompt}\n{answer}\n{tools:?}",
            started.elapsed().as_secs_f64(),
            stats.requests,
            stats.prompt_tokens,
            stats.output_tokens,
            result.err()
        );
        if gated {
            return;
        }
        if comparison.is_none() {
            assert!(passed, "Live workflow failed: {prompt}");
        }
        request.messages.push(InputMessage {
            id: None,
            role: "assistant",
            content: answer,
            images: Vec::new(),
        });
    }
    let function_added = std::fs::read_to_string(root.join("main.rs"))
        .is_ok_and(|source| source.contains("fn triple("));
    let observed = |args: &[&str]| {
        std::process::Command::new(root.join("demo.exe"))
            .args(args)
            .output()
            .ok()
            .filter(|output| output.status.success())
            .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
    };
    let triple = observed(&["triple", "14"]);
    let ping = observed(&["ping"]);
    let artifact_passed =
        function_added && triple.as_deref() == Some("42") && ping.as_deref() == Some("pong");
    eprintln!(
        "Artifact passed={artifact_passed} function={function_added} triple={triple:?} ping={ping:?}"
    );
    if comparison.is_none() {
        assert!(artifact_passed);
    }
    eprintln!("Verified workspace: {}", root.display());
    eprintln!("Final checkpoint: {}", checkpoint.display());
}

#[tokio::test]
async fn ainsley_16_uses_thinking_and_executes_a_native_project_edit() {
    let edit = json!({"message":{"tool_calls":[{"function":{"name":"edit_file","arguments":{
        "path":"note.txt","edits":[{"old":"READ_CONFIRMED","new":"EDIT_CONFIRMED"}]
    }}}]},"done":true});
    let (endpoint, captured, handle) = server(vec![
        (
            "/api/show",
            json!({"capabilities":["thinking","tools"]}).to_string(),
        ),
        ("/api/chat", format!("{edit}\n")),
        ("/api/chat", text_reply("Edited note.txt.")),
    ]);
    let mut request = project_request(endpoint);
    request.model = "Aisnley-Ox-1.6-bbb:35b".into();
    request.mode = Mode::FullAccess;
    request.messages[0].content = "Replace READ_CONFIRMED with EDIT_CONFIRMED in note.txt.".into();
    let note = request.workspace.as_ref().unwrap().join("note.txt");
    let (sender, _events) = mpsc::channel(64);
    let (_stop, cancel) = watch::channel(false);
    stream_qualified_chat(request, sender, cancel)
        .await
        .unwrap();
    handle.join().unwrap();
    let calls = captured.lock().unwrap();
    assert_eq!(calls[1].1["think"], true);
    assert!(
        calls[2].1["messages"]
            .as_array()
            .unwrap()
            .iter()
            .any(|message| { message["role"] == "tool" && message["tool_name"] == "edit_file" })
    );
    assert_eq!(std::fs::read_to_string(note).unwrap(), "EDIT_CONFIRMED");
}

#[tokio::test]
async fn saffyos_turn_continues_past_the_regular_round_limit() {
    let mut responses = vec![("/api/show", json!({"capabilities":[]}).to_string())];
    for round in 0..=MAX_CHAT_ROUNDS {
        let partial = json!({
            "message": {"content": format!("Partial reply {round}")},
            "done": true,
            "done_reason": "length",
        });
        responses.push(("/api/chat", format!("{partial}\n")));
    }
    responses.push(("/api/chat", text_reply("Finished the long turn.")));

    let (endpoint, captured, handle) = server(responses);
    let mut request = request(endpoint, false);
    request.saffyos_locked = true;
    request.saffyos_folder = Some(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .canonicalize()
            .unwrap(),
    );
    request.settings.context_tokens = 65536;
    let (sender, mut events) = mpsc::channel(256);
    let (_stop, cancel) = watch::channel(false);
    stream_qualified_chat(request, sender, cancel)
        .await
        .unwrap();
    handle.join().unwrap();

    assert_eq!(captured.lock().unwrap().len(), MAX_CHAT_ROUNDS + 3);
    assert!(std::iter::from_fn(|| events.try_recv().ok()).any(|event| {
        matches!(event, ProviderEvent::Content(text) if text == "Finished the long turn.")
    }));
}

#[tokio::test]
async fn ainsley_16_stops_after_varying_truncated_replies() {
    let mut responses = vec![(
        "/api/show",
        json!({"capabilities":["thinking"]}).to_string(),
    )];
    for index in 0..MAX_CHAT_ROUNDS {
        responses.push((
            "/api/chat",
            format!(
                "{}\n",
                json!({
                    "message": {"content": format!("Partial reply {index}")},
                    "done": true,
                    "done_reason": "length"
                })
            ),
        ));
    }
    let (endpoint, captured, handle) = server(responses);
    let mut request = request(endpoint, false);
    request.model = "Aisnley-Ox-1.6-bbb:35b".into();
    request.settings.context_tokens = 65536;
    let checkpoint = request.checkpoint.clone().unwrap();
    let (sender, mut events) = mpsc::channel(256);
    let (_stop, cancel) = watch::channel(false);
    let error = stream_qualified_chat(request, sender, cancel)
        .await
        .unwrap_err();
    assert!(error.contains("32-round response limit"));
    assert!(checkpoint.with_file_name("handoffs").is_dir());
    handle.join().unwrap();
    let calls = captured.lock().unwrap();
    assert_eq!(calls.len(), MAX_CHAT_ROUNDS + 1);
    assert!(calls[1..].iter().all(|(_, body)| body["think"] == false));
    while events.try_recv().is_ok() {}
}
