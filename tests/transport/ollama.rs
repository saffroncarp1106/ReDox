use super::*;
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
        messages: vec![InputMessage {
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
async fn rust_profile_uses_model_instructions_and_keeps_reasoning_out_of_answer() {
    let profile = "Review the Rust public interface.";
    let model_info = json!({"capabilities":["thinking"],"system":profile});
    let reply = json!({
        "message":{"content":"fn main() {}","thinking":"Private reasoning"},
        "done":true,
        "eval_count":8
    });
    for (model, expected) in [
        (RUST_PROFILE_MODEL, true),
        ("redox-qwen-rust:35b", true),
        ("qwen3.6:35b", false),
    ] {
        let (endpoint, captured, handle) = server(vec![
            ("/api/show", model_info.to_string()),
            ("/api/chat", format!("{reply}\n")),
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
        while let Some(event) = events.recv().await {
            if let ProviderEvent::Content(content) = event {
                answer.push_str(&content);
            }
        }
        assert_eq!(answer, "fn main() {}");
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
        ("/api/show", json!({"capabilities":["thinking"]}).to_string()),
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
    assert!(matches!(events.recv().await, Some(ProviderEvent::Content(text)) if text == "partial"));
    while let Some(event) = events.recv().await {
        assert!(matches!(event, ProviderEvent::Status(_)));
    }
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
    let journal = state::Journal::open(checkpoint, Some(root), "Resume".into())
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
async fn stop_cancels_retry_backoff() {
    let (endpoint, captured, handle) = server(vec![
        ("/api/show", json!({"capabilities":[]}).to_string()),
        ("/api/chat", String::new()),
    ]);
    let (sender, mut events) = mpsc::channel(16);
    let (cancel, receiver) = watch::channel(false);
    let worker = tokio::spawn(stream_chat(request(endpoint, false), sender, receiver));

    assert!(
        matches!(events.recv().await, Some(ProviderEvent::Status(status)) if status.contains("retrying"))
    );
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
    assert!(events.recv().await.is_none());
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
