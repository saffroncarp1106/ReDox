use super::*;
use crate::domain::{Provider, Session};
use crate::run::{RunEvent, Update};

fn task(id: &str) -> Task {
    Task {
        id: id.into(),
        model: "qwen-test".into(),
        task: "Read the project".into(),
        status: Status::Running,
        progress: "Starting".into(),
        output: String::new(),
        error: None,
    }
}

#[test]
fn compact_handoff_preserves_identity_and_points_to_full_evidence() {
    let value = json!({"task_id":"job","status":"review_required","result":"é".repeat(10000),
        "evidence":(0..30).map(|_| json!({"command":"x".repeat(500)})).collect::<Vec<_>>(),
        "changed_paths":vec!["src/changed.rs"; 100]});
    let encoded = compact_handoff(value, PathBuf::from("full-report.json"));
    assert!(encoded.len() <= worker::REPORT_BYTES);
    let decoded: Value = serde_json::from_str(&encoded).unwrap();
    assert_eq!(decoded["task_id"], "job");
    assert_eq!(decoded["full_report"], "full-report.json");
    assert!(decoded["omitted_evidence"].as_u64().unwrap() > 0);
}

fn project() -> PathBuf {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("temp/verification/delegation")
        .join(uuid::Uuid::new_v4().to_string());
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("observed.txt"), "EXISTING_OBSERVATION").unwrap();
    root.canonicalize().unwrap()
}

fn reply(content: &str, calls: Value) -> String {
    format!(
        "{}\n",
        json!({"message":{"content":content,"tool_calls":calls},"done":true})
    )
}

async fn run_worker(args: Value, endpoint: String, root: PathBuf) -> Value {
    let settings = ollama::Settings {
        endpoint,
        preferred_model: "test-worker".into(),
        context_tokens: 32768,
        ..Default::default()
    };
    let (sender, mut events) = mpsc::channel(128);
    let drain = tokio::spawn(async move { while events.recv().await.is_some() {} });
    let (_cancel, receiver) = watch::channel(false);
    let output = execute(&args, settings, Some(root), Mode::Chat, sender, receiver)
        .await
        .unwrap();
    drain.await.unwrap();
    serde_json::from_str(&output).unwrap()
}

#[tokio::test]
async fn handoff_returns_final_answer_and_resumes_recorded_observations() {
    let root = project();
    let read = json!([{"function":{"name":"read_file","arguments":{"path":"observed.txt"}}}]);
    let (endpoint, _, handle) = ollama::tests::server(vec![
        ("/api/show", json!({"capabilities":["tools"]}).to_string()),
        ("/api/chat", reply(&"EARLY_COMMENTARY".repeat(1000), read)),
        ("/api/chat", reply("FINAL_FINDINGS", json!([]))),
    ]);
    let result = run_worker(json!({"task":"Read observed.txt","scope":"observed.txt","acceptance":"Report the observation"}), endpoint, root.clone()).await;
    handle.join().unwrap();
    assert_eq!(result["result"], "FINAL_FINDINGS");
    assert_eq!(result["status"], "review_required");
    assert_eq!(result["tool_calls"], 1);
    assert!(!result.to_string().contains("EARLY_COMMENTARY"));
    let (endpoint, captured, handle) = ollama::tests::server(vec![
        ("/api/show", json!({"capabilities":["tools"]}).to_string()),
        ("/api/chat", reply("CORRECTION_COMPLETE", json!([]))),
    ]);
    let resumed = run_worker(json!({"task":"Correct the summary using the observation already read","task_id":result["task_id"]}),endpoint,root.clone()).await;
    handle.join().unwrap();
    assert_eq!(resumed["task_id"], result["task_id"]);
    assert!(
        captured.lock().unwrap()[1].1["messages"]
            .to_string()
            .contains("EXISTING_OBSERVATION")
    );
    let bad = ollama::Settings {
        endpoint: "http://127.0.0.1:1".into(),
        preferred_model: "other-model".into(),
        ..Default::default()
    };
    let (sender, _events) = mpsc::channel(8);
    let (_cancel, receiver) = watch::channel(false);
    let error = execute(
        &json!({"task":"wrong model","task_id":result["task_id"]}),
        bad,
        Some(root),
        Mode::Chat,
        sender,
        receiver,
    )
    .await
    .unwrap_err();
    assert!(error.contains("original project, access mode and selected model"));
}

#[tokio::test]
async fn unchanged_reads_stop_with_a_resumable_stalled_handoff() {
    let root = project();
    let call = json!([{"function":{"name":"read_file","arguments":{"path":"observed.txt"}}}]);
    let mut responses = vec![("/api/show", json!({"capabilities":["tools"]}).to_string())];
    responses.extend((0..4).map(|_| ("/api/chat", reply("Reading", call.clone()))));
    let (endpoint, captured, handle) = ollama::tests::server(responses);
    let result = run_worker(json!({"task":"Inspect the file"}), endpoint, root).await;
    handle.join().unwrap();
    assert_eq!(result["status"], "stalled");
    assert_eq!(result["tool_calls"], 4);
    assert!(result["task_id"].is_string());
    assert!(
        captured.lock().unwrap()[4].1["messages"]
            .to_string()
            .contains("One recovery attempt remains")
    );
}

#[tokio::test]
async fn delegation_continues_across_two_windows_and_eighty_tools() {
    let root = project();
    let batch = |start| {
        let calls = (start..start + 40).map(|index| json!({"function":{"name":"read_file","arguments":{"path":format!("fixture-{index}.txt")}}})).collect::<Vec<_>>();
        reply("Inspecting fixtures", json!(calls))
    };
    for index in 0..80 {
        std::fs::write(
            root.join(format!("fixture-{index}.txt")),
            "fixture data\n".repeat(50),
        )
        .unwrap();
    }
    let (endpoint, captured, handle) = ollama::tests::server(vec![
        ("/api/show", json!({"capabilities":["tools"]}).to_string()),
        ("/api/chat", batch(0)),
        (
            "/api/chat",
            reply(
                "## Remaining tasks\nRead the second set.\n## Work completed\nRead fixtures 0 through 39.",
                json!([]),
            ),
        ),
        ("/api/chat", batch(40)),
        (
            "/api/chat",
            reply(
                "## Remaining tasks\nReport the verified result.\n## Work completed\nRead fixtures 0 through 79.",
                json!([]),
            ),
        ),
        ("/api/chat", reply("Verified all 80 fixtures.", json!([]))),
    ]);
    let result = run_worker(json!({"task":"Inspect files"}), endpoint, root).await;
    handle.join().unwrap();
    assert_eq!(result["status"], "review_required");
    assert_eq!(result["tool_calls"], 80);
    assert_eq!(result["result"], "Verified all 80 fixtures.");
    let calls = captured.lock().unwrap();
    assert_eq!(calls[2].1["stream"], false);
    assert!(
        calls[3].1["messages"]
            .to_string()
            .contains("Read the second set.")
    );
    assert!(
        calls[3].1["messages"]
            .to_string()
            .contains("fixture-39.txt")
    );
    assert_eq!(calls[4].1["stream"], false);
    assert!(
        calls[5].1["messages"]
            .to_string()
            .contains("Report the verified result.")
    );
    assert!(
        calls[5].1["messages"]
            .to_string()
            .contains("fixture-79.txt")
    );
    let folder = PathBuf::from(result["checkpoint"].as_str().unwrap()).with_file_name("handoffs");
    let handoffs = std::fs::read_dir(folder)
        .unwrap()
        .map(|entry| std::fs::read_to_string(entry.unwrap().path()).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(handoffs.len(), 2);
    for handoff in handoffs {
        assert!(handoff.contains("## Remaining tasks"));
        assert!(handoff.contains("## Work completed"));
        assert!(handoff.contains("Tool result: read_file"));
    }
}

#[tokio::test]
async fn repeated_truncations_return_partial_work_without_claiming_completion() {
    let root = project();
    let mut responses = vec![("/api/show", json!({"capabilities":[]}).to_string())];
    responses.extend((0..3).map(|_| {
        (
            "/api/chat",
            format!(
                "{}\n",
                json!({"message":{"content":"unfinished"},"done":true,"done_reason":"length"})
            ),
        )
    }));
    let (endpoint, _, handle) = ollama::tests::server(responses);
    let result = run_worker(json!({"task":"Finish a task"}), endpoint, root).await;
    handle.join().unwrap();
    assert_eq!(result["status"], "output_limit");
    assert_ne!(result["status"], "review_required");
}

#[test]
fn failed_handoffs_preserve_evidence_and_report_tool_failure() {
    let result = tool_result(Ok(
        json!({"status":"failed","result":"Actual server error","task_id":"retained"}).to_string(),
    ));
    assert_eq!(result["success"], false);
    assert!(result.to_string().contains("Actual server error"));
    assert!(result.to_string().contains("retained"));
}

#[test]
fn old_delegation_threads_upgrade_their_tool_schema_once() {
    let mut session = Session::default();
    session.new_chat();
    session.ollama.preferred_model = "test-worker".into();
    let chat = session.active_mut().unwrap();
    chat.delegation_tools = true;
    chat.thread_id = Some("old-schema".into());
    chat.draft = "Do a task".into();
    let run = session.prepare_run().unwrap();
    assert!(session.active().unwrap().thread_id.is_none());
    assert!(
        session
            .active()
            .unwrap()
            .previous_thread_ids
            .contains(&"old-schema".into())
    );
    crate::run::apply_event(
        &mut session,
        Some(run),
        RunEvent {
            run,
            update: Update::Thread("new-schema".into()),
        },
    );
    assert_eq!(
        session.active().unwrap().delegation_tool_version,
        TOOL_VERSION
    );
    session.finish_run(run, crate::domain::MessageStatus::Complete, None);
    session.active_mut().unwrap().draft = "Continue".into();
    session.prepare_run().unwrap();
    assert_eq!(
        session.active().unwrap().thread_id.as_deref(),
        Some("new-schema")
    );
}

#[tokio::test]
async fn recovered_worker_results_return_to_codex_and_agents() {
    let (endpoint, captured, handle) = ollama::tests::server(vec![
        ("/api/show", json!({"capabilities":[]}).to_string()),
        (
            "/api/chat",
            format!(
                "{}\n",
                json!({"message":{"content":"Abandoned output"},"done":false})
            ),
        ),
        (
            "/api/chat",
            format!(
                "{}\n",
                json!({"message":{"content":"LOCAL_REVIEW_RESULT"},"done":true,"prompt_eval_count":11,"eval_count":4})
            ),
        ),
    ]);
    let settings = ollama::Settings {
        endpoint,
        preferred_model: "selected-qwen".into(),
        ..ollama::Settings::default()
    };
    let (sender, mut events) = mpsc::channel(32);
    let (_cancel, receiver) = watch::channel(false);
    let result = tokio::time::timeout(std::time::Duration::from_secs(5), execute(
        &json!({"task":"Review the project","context":"Check naming","model":"ignored-unselected-model"}),
        settings, None, Mode::Chat, sender, receiver)).await.unwrap().unwrap();
    handle.join().unwrap();
    assert!(result.contains("LOCAL_REVIEW_RESULT"));
    assert!(!result.contains("Abandoned output"));
    assert_eq!(tool_result(Ok(result))["success"], true);
    let mut tasks = Vec::new();
    let mut tokens = 0;
    while let Some(event) = events.recv().await {
        match event {
            ProviderEvent::Delegation(event) => apply(&mut tasks, event),
            ProviderEvent::Metrics(stats) => tokens += stats.output_tokens,
            ProviderEvent::Content(_) => panic!(
                "Local output must return to Codex as a tool result, not impersonate its chat response"
            ),
            _ => {}
        }
    }
    assert_eq!(tokens, 4);
    assert_eq!(tasks[0].model, "selected-qwen");
    assert_eq!(tasks[0].output, "LOCAL_REVIEW_RESULT");
    assert_eq!(tasks[0].status, Status::Complete);
    assert_eq!(captured.lock().unwrap()[1].1["model"], "selected-qwen");
}

#[tokio::test]
async fn delegated_project_changes_use_the_parent_approval_flow_and_access_mode() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("temp/verification/delegation")
        .join(uuid::Uuid::new_v4().to_string());
    std::fs::create_dir_all(&root).unwrap();
    let call = json!({"message":{"content":"","tool_calls":[{"function":{"name":"write_file","arguments":{"path":"new.txt","content":"unapproved"}}}]},"done":true});
    let (endpoint, captured, handle) = ollama::tests::server(vec![
        ("/api/show", json!({"capabilities":["tools"]}).to_string()),
        ("/api/chat", format!("{call}\n")),
        (
            "/api/chat",
            format!(
                "{}\n",
                json!({"message":{"content":"Change rejected by the user."},"done":true})
            ),
        ),
    ]);
    let settings = ollama::Settings {
        endpoint,
        preferred_model: "qwen-test".into(),
        ..ollama::Settings::default()
    };
    let (sender, mut events) = mpsc::channel(32);
    let (_cancel, receiver) = watch::channel(false);
    let observe = async {
        let mut approval_count = 0;
        while let Some(event) = events.recv().await {
            if let ProviderEvent::Approval(approval) = event {
                assert!(approval.description.contains("new.txt"));
                approval.response.send(false).unwrap();
                approval_count += 1;
            }
        }
        approval_count
    };
    let args = json!({"task":"Create a file"});
    let (result, count) = tokio::join!(
        execute(
            &args,
            settings,
            Some(root.clone()),
            Mode::Code,
            sender,
            receiver
        ),
        observe
    );
    assert!(result.is_ok());
    assert_eq!(count, 1);
    assert!(!root.join("new.txt").exists());
    handle.join().unwrap();
    assert!(
        captured.lock().unwrap()[2].1["messages"]
            .as_array()
            .unwrap()
            .iter()
            .any(|message| message["role"] == "tool"
                && message["content"].as_str().unwrap().contains("rejected"))
    );
}

#[test]
fn codex_conversations_get_a_fresh_tool_enabled_thread_without_changing_provider() {
    let mut session = Session::default();
    session.new_chat();
    session.ollama.preferred_model = "qwen-test".into();
    let chat = session.active_mut().unwrap();
    chat.thread_id = Some("old-thread-without-tool".into());
    chat.draft = "Delegate a review".into();
    let run = session.prepare_run().unwrap();
    assert_eq!(session.active().unwrap().provider, Provider::Codex);
    assert!(session.active().unwrap().thread_id.is_none());
    assert!(
        session
            .active()
            .unwrap()
            .previous_thread_ids
            .contains(&"old-thread-without-tool".into())
    );
    crate::run::apply_event(
        &mut session,
        Some(run),
        RunEvent {
            run,
            update: Update::Thread("tool-enabled-thread".into()),
        },
    );
    assert!(session.active().unwrap().delegation_tools);
    session.finish_run(run, crate::domain::MessageStatus::Complete, None);
    session.active_mut().unwrap().draft = "Continue".into();
    session.prepare_run().unwrap();
    assert_eq!(
        session.active().unwrap().thread_id.as_deref(),
        Some("tool-enabled-thread")
    );
}

#[test]
fn progress_output_and_cancellation_are_scoped_to_the_parent_run() {
    let mut session = Session::default();
    session.new_chat();
    session.active_mut().unwrap().draft = "Delegate".into();
    let run = session.prepare_run().unwrap();
    crate::run::apply_event(
        &mut session,
        Some(run),
        RunEvent {
            run,
            update: Update::Delegation(Event::Started(task("one"))),
        },
    );
    crate::run::apply_event(
        &mut session,
        Some(run),
        RunEvent {
            run,
            update: Update::Delegation(Event::Content {
                id: "one".into(),
                text: "Local result".into(),
            }),
        },
    );
    assert_eq!(
        session.active().unwrap().delegated_tasks[0].output,
        "Local result"
    );
    assert_eq!(
        session.active().unwrap().messages.last().unwrap().content,
        ""
    );
    session.finish_run(run, crate::domain::MessageStatus::Cancelled, None);
    assert_eq!(
        session.active().unwrap().delegated_tasks[0].status,
        Status::Cancelled
    );
    crate::run::apply_event(
        &mut session,
        None,
        RunEvent {
            run,
            update: Update::Delegation(Event::Content {
                id: "one".into(),
                text: "stale".into(),
            }),
        },
    );
    assert_eq!(
        session.active().unwrap().delegated_tasks[0].output,
        "Local result"
    );
}

#[test]
fn interrupted_worker_logs_survive_restart_and_tool_responses_preserve_failure() {
    let mut session = Session::default();
    session.new_chat();
    session
        .active_mut()
        .unwrap()
        .delegated_tasks
        .push(task("one"));
    let mut restored: Session =
        serde_json::from_slice(&serde_json::to_vec(&session).unwrap()).unwrap();
    restored.recover();
    assert_eq!(
        restored.active().unwrap().delegated_tasks[0].status,
        Status::Interrupted
    );
    assert_eq!(tool_spec()["name"], TOOL_NAME);
    assert_eq!(tool_spec()["inputSchema"]["additionalProperties"], false);
    let result = tool_result(Err("Server unavailable".into()));
    assert_eq!(result["success"], false);
    assert!(
        result["contentItems"][0]["text"]
            .as_str()
            .unwrap()
            .contains("Server unavailable")
    );
    let mut text = String::new();
    append_bounded(&mut text, &"é".repeat(OUTPUT_LIMIT));
    assert_eq!(text.len(), OUTPUT_LIMIT);
}

#[tokio::test]
async fn disabled_delegation_does_not_contact_an_ollama_server() {
    let settings = ollama::Settings {
        endpoint: "http://127.0.0.1:1".into(),
        preferred_model: "qwen-test".into(),
        codex_delegation: false,
        ..ollama::Settings::default()
    };
    let (sender, mut events) = mpsc::channel(16);
    let (_cancel, receiver) = watch::channel(false);
    let error = execute(
        &json!({"task":"test"}),
        settings,
        None,
        Mode::Chat,
        sender,
        receiver,
    )
    .await
    .unwrap_err();
    assert!(error.contains("enable Codex delegation"));
    assert!(events.recv().await.is_none());
}

#[tokio::test]
#[ignore = "read-only installed Codex protocol check; does not start a model turn"]
async fn installed_codex_accepts_the_delegation_tool_without_a_generation() {
    let mut rpc = crate::codex::Rpc::connect(&crate::codex::find_executable())
        .await
        .unwrap();
    let result = rpc.call("thread/start", json!({"ephemeral":true,"sandbox":"read-only","approvalPolicy":"untrusted","dynamicTools":[tool_spec()]})).await;
    rpc.close().await;
    let result = result.unwrap();
    assert!(result["thread"]["id"].is_string());
}
