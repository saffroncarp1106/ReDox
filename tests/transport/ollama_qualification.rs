use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;

use super::*;

const SAFFYOS_MODEL: &str = "Aisnley-Ox-1.5-bbb:exp-invariants";
const SUBMISSION: &str = r#"
pub enum Expr {
    Identifier(String),
    Not(Box<Expr>),
    And(Box<Expr>, Box<Expr>),
    Or(Box<Expr>, Box<Expr>),
}
pub struct ParseError {
    pub offset: usize,
    pub message: String,
}
pub fn parse_filter(input: &str) -> Result<Expr, ParseError> {
    Ok(Expr::Identifier(input.into()))
}
"#;

#[test]
fn strict_reasoning_cases_include_tools_and_reject_bare_json() {
    let trials = reasoning_trials("jam-example");
    assert_eq!(trials.len(), 10);
    assert_eq!(trials.iter().filter(|trial| trial.fixture).count(), 2);
    assert!(strict_pass("```json\n{\"n\":1}\n```", &json!({"n": 1})));
    assert!(!strict_pass("{\"n\":1}", &json!({"n": 1})));
    assert!(!strict_pass("```json\n{\"n\":1.0}\n```", &json!({"n": 1})));
}

#[tokio::test]
async fn retries_reasoning_and_cargo_failures_then_reuses_qualification() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        let mut attempt = 0;
        let mut delivered = 0;
        let mut reads = 0;
        let mut rust = 0;
        let mut repairs = 0;
        while delivered < 2 {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut reader = BufReader::new(&socket);
            let mut head = String::new();
            reader.read_line(&mut head).unwrap();
            let mut length = 0;
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                if line == "\r\n" {
                    break;
                }
                if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                    length = value.trim().parse().unwrap();
                }
            }
            let mut payload = vec![0; length];
            reader.read_exact(&mut payload).unwrap();
            let path = head.split_whitespace().nth(1).unwrap();
            let body: Value = serde_json::from_slice(&payload).unwrap_or_default();
            let response = match path {
                "/_redox/health" => json!({"service":"plain-ollama"}),
                "/api/generate" => {
                    assert_eq!(body["keep_alive"], 0);
                    attempt += 1;
                    json!({"done":true,"done_reason":"unload"})
                }
                "/api/ps" => {
                    assert_eq!(attempt, 3);
                    json!({"models":[{"name":SAFFYOS_MODEL,"digest":"stable","context_length":64000}]})
                }
                "/api/show" => json!({"capabilities":["tools","thinking"]}),
                "/api/chat" => {
                    let messages = body["messages"].as_array().unwrap();
                    let users = messages
                        .iter()
                        .filter(|message| message["role"] == "user")
                        .map(|message| message["content"].as_str().unwrap())
                        .collect::<Vec<_>>();
                    let first = users[0];
                    let marker = first.split('\'').nth(1).unwrap();
                    if users.iter().any(|user| *user == "held user prompt") {
                        assert_eq!(attempt, 3);
                        assert!(users.iter().any(|user| user.contains("Build a parser")));
                        assert_eq!(users.len(), REASONING_TRIALS + 2);
                        assert_eq!(messages[messages.len() - 2]["content"], SUBMISSION);
                        assert_eq!(body["keep_alive"], KEEP_WARM_SECONDS);
                        delivered += 1;
                        reply("ordinary answer")
                    } else if attempt == 1 {
                        assert_eq!(users.len(), 1);
                        let mut response = reply("{\"acknowledged\":true}");
                        response["load_duration"] = json!(200_000_000);
                        response
                    } else if users.last().unwrap().contains("Build a parser") {
                        assert_eq!(users.len(), REASONING_TRIALS + 1);
                        let expected = reasoning_trials(marker).pop().unwrap().expected;
                        assert_eq!(
                            messages[messages.len() - 2]["content"],
                            format!("```json\n{expected}\n```")
                        );
                        rust += 1;
                        reply(&format!("{SUBMISSION}\nstruct Parser {{ input: &str }}"))
                    } else if users
                        .last()
                        .unwrap()
                        .starts_with("Cargo rejected the parser.")
                    {
                        assert_eq!(users.len(), REASONING_TRIALS + 2);
                        assert!(users.last().unwrap().contains("E0106"));
                        assert!(users.last().unwrap().contains("missing lifetime specifier"));
                        repairs += 1;
                        reply(if attempt == 2 {
                            "pub fn broken("
                        } else {
                            SUBMISSION
                        })
                    } else {
                        let trials = reasoning_trials(marker);
                        if users
                            .last()
                            .unwrap()
                            .starts_with("Your response reached its output token limit.")
                        {
                            assert_eq!(attempt, 3);
                            assert_eq!(users.len(), REASONING_TRIALS + 1);
                            return_reply(&trials[REASONING_TRIALS - 1].expected)
                        } else if attempt == 3 && users.len() == REASONING_TRIALS {
                            let mut response = reply("```json\n[1,");
                            response["done_reason"] = json!("length");
                            response
                        } else {
                            let trial = &trials[users.len() - 1];
                            if trial.fixture && messages.last().unwrap()["role"] != "tool" {
                                reads += 1;
                                json!({"done":true,"message":{"tool_calls":[{"function":{"name":"read_file","arguments":{"path":"fixture.json"}}}]}})
                            } else {
                                if trial.fixture {
                                    assert_eq!(
                                        serde_json::from_str::<Value>(
                                            messages.last().unwrap()["content"].as_str().unwrap()
                                        )
                                        .unwrap(),
                                        trial.expected
                                    );
                                }
                                reply(&format!("```json\n{}\n```", trial.expected))
                            }
                        }
                    }
                }
                _ => panic!("Unexpected request {head}"),
            };
            let response = format!("{response}\n");
            write!(
                socket,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}",
                response.len()
            )
            .unwrap();
        }
        assert_eq!(reads, 4);
        assert_eq!(rust, 2);
        assert_eq!(repairs, 2);
    });
    let mut statuses = Vec::new();
    let mut completed_timings = Vec::new();
    for _ in 0..2 {
        let request = Request::for_task(
            super::super::Settings {
                endpoint: endpoint.clone(),
                preferred_model: SAFFYOS_MODEL.into(),
                context_tokens: 64_000,
                output_tokens: 4096,
                ..Default::default()
            },
            "held user prompt".into(),
            None,
            Mode::Chat,
        );
        let (sender, mut events) = mpsc::channel(128);
        let (_stop, cancel) = watch::channel(false);
        tokio::time::timeout(
            Duration::from_secs(25),
            stream_qualified_chat(request, sender, cancel),
        )
        .await
        .unwrap()
        .unwrap();
        let mut output = String::new();
        while let Some(event) = events.recv().await {
            match event {
                ProviderEvent::Status(status) => statuses.push(status),
                ProviderEvent::Qualification(timing) if timing.completed => {
                    completed_timings.push(timing);
                }
                ProviderEvent::Content(text) => output.push_str(&text),
                _ => {}
            }
        }
        assert_eq!(output, "ordinary answer");
    }
    server.join().unwrap();
    assert_eq!(
        statuses.first(),
        Some(&format!(
            "{SAFFYOS_MODEL} qualification · Attempt 1 · loading fresh weights"
        ))
    );
    assert!(
        statuses
            .iter()
            .any(|status| status.contains("Reasoning trial 1 failed"))
    );
    assert!(
        statuses
            .iter()
            .any(|status| status.contains("Rust build failed"))
    );
    assert_eq!(
        statuses
            .iter()
            .filter(|status| status.contains("starting your prompt"))
            .count(),
        1
    );
    let [timing] = completed_timings.as_slice() else {
        panic!("Expected exactly one completed qualification");
    };
    assert_eq!(timing.first_load_seconds, Some(0.2));
    assert_eq!(timing.attempt, 3);
    assert_eq!(timing.trial_seconds.len(), REASONING_TRIALS);
    assert!(timing.elapsed_seconds >= timing.rust_seconds.unwrap());
}

fn reply(text: &str) -> Value {
    json!({"done":true,"message":{"content":text},"eval_count":10,"prompt_eval_count":10,
           "load_duration":1_000_000})
}

fn return_reply(expected: &Value) -> Value {
    reply(&format!("```json\n{expected}\n```"))
}
