use super::*;
use crate::codex::ProviderEvent;
use crate::domain::{Message, MessageStatus};
use serde_json::json;
use std::io::{Read, Write};
use std::net::TcpListener;
use tokio::sync::{mpsc, watch};

fn serve_once(response: String) -> (String, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let handle = std::thread::spawn(move || {
        if let Ok((mut socket, _)) = listener.accept() {
            let mut buffer = [0u8; 8192];
            let _ = socket.read(&mut buffer);
            let _ = socket.write_all(response.as_bytes());
        }
    });
    (format!("http://{address}/"), handle)
}

fn sse(body: &str) -> String {
    format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n{body}")
}

fn request(endpoint: String) -> Request {
    Request {
        settings: Settings {
            api_key: "test-key".into(),
            base_url: endpoint,
            model: "deepseek-chat".into(),
        },
        model: "deepseek-chat".into(),
        system: "You are ReDox.".into(),
        messages: vec![json!({"role": "user", "content": "hi"})],
        desktop: Default::default(),
        desktop_activity: Default::default(),
    }
}

fn message(role: Role, content: &str) -> Message {
    Message {
        id: uuid::Uuid::new_v4(),
        role,
        content: content.into(),
        status: MessageStatus::Complete,
        error: None,
        model: String::new(),
        attachments: Vec::new(),
        activity: Default::default(),
    }
}

#[test]
fn endpoint_accepts_base_and_slash_forms() {
    let mut settings = Settings::default();
    assert_eq!(
        settings
            .endpoint()
            .unwrap()
            .join("chat/completions")
            .unwrap()
            .as_str(),
        "https://api.deepseek.com/chat/completions"
    );
    settings.base_url = "https://example.test/v1".into();
    assert_eq!(
        settings
            .endpoint()
            .unwrap()
            .join("chat/completions")
            .unwrap()
            .as_str(),
        "https://example.test/v1/chat/completions"
    );
    settings.base_url = String::new();
    assert!(settings.endpoint().is_ok());
}

#[test]
fn empty_model_falls_back_to_the_default() {
    let settings = Settings {
        api_key: "k".into(),
        base_url: String::new(),
        model: "   ".into(),
    };
    assert_eq!(settings.model(), DEFAULT_MODEL);
}

#[test]
fn history_skips_failed_and_blank_messages() {
    let messages = vec![
        message(Role::User, "hello"),
        message(Role::Assistant, "hi there"),
        Message {
            error: Some("boom".into()),
            ..message(Role::Assistant, "half")
        },
        message(Role::User, "   "),
    ];
    let rendered = history(&messages);
    assert_eq!(rendered.len(), 2);
    assert_eq!(rendered[0]["role"], "user");
    assert_eq!(rendered[0]["content"], "hello");
    assert_eq!(rendered[1]["role"], "assistant");
}

#[tokio::test]
async fn streams_content_and_reports_usage() {
    let (endpoint, handle) = serve_once(sse(
        "data: {\"choices\":[{\"delta\":{\"content\":\"Hello\"}}]}\n\n\
         data: {\"choices\":[{\"delta\":{\"content\":\" there.\"}}]}\n\n\
         data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":12,\"completion_tokens\":5,\"total_tokens\":17,\"prompt_cache_hit_tokens\":8}}\n\n\
         data: [DONE]\n\n",
    ));
    let (sender, mut receiver) = mpsc::channel(32);
    let (_cancel_tx, cancel_rx) = watch::channel(false);
    stream_chat(request(endpoint), sender, cancel_rx).await.unwrap();
    handle.join().unwrap();

    let mut content = String::new();
    let mut metrics = None;
    let mut context = None;
    while let Ok(event) = receiver.try_recv() {
        match event {
            ProviderEvent::Content(text) => content.push_str(&text),
            ProviderEvent::Metrics(stats) => metrics = Some(stats),
            ProviderEvent::Context(usage) => context = Some(usage),
            _ => {}
        }
    }
    assert_eq!(content, "Hello there.");
    let metrics = metrics.expect("metrics");
    assert_eq!(metrics.prompt_tokens, 12);
    assert_eq!(metrics.output_tokens, 5);
    assert_eq!(metrics.requests, 1);
    let context = context.expect("context");
    assert_eq!(context.tokens, 17);
    assert_eq!(context.limit, MODEL_LIMIT);
    assert_eq!(context.cached_tokens, Some(8));
    assert!(!context.estimated);
}

#[tokio::test]
async fn api_errors_surface_the_message() {
    let body = "{\"error\":{\"message\":\"Authentication Fails (invalid key)\",\"type\":\"auth\"}}";
    let response = format!(
        "HTTP/1.1 401 Unauthorized\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let (endpoint, handle) = serve_once(response);
    let (sender, _receiver) = mpsc::channel(32);
    let (_cancel_tx, cancel_rx) = watch::channel(false);
    let error = stream_chat(request(endpoint), sender, cancel_rx)
        .await
        .unwrap_err();
    handle.join().unwrap();
    assert!(error.contains("401"));
    assert!(error.contains("invalid key"));
}

#[tokio::test]
async fn cancellation_is_reported_before_any_request() {
    let (sender, _receiver) = mpsc::channel(8);
    let (_cancel_tx, cancel_rx) = watch::channel(true);
    let error = stream_chat(request("http://127.0.0.1:9/".into()), sender, cancel_rx)
        .await
        .unwrap_err();
    assert!(error.contains("Cancelled"));
}
