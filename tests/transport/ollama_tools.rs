use super::*;

fn project() -> PathBuf {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("temp/verification/ollama-tools")
        .join(uuid::Uuid::new_v4().to_string())
        .join("project");
    std::fs::create_dir_all(&root).unwrap();
    root
}

#[tokio::test]
async fn project_reads_work_but_traversal_and_read_only_writes_are_rejected() {
    let root = project();
    std::fs::write(root.join("note.txt"), "LOCAL_PROJECT_DATA").unwrap();
    let (sender, mut events) = mpsc::channel(8);
    assert_eq!(
        execute(
            "read_file",
            &json!({"path":"note.txt"}),
            Some(&root),
            Mode::Chat,
            &sender
        )
        .await
        .unwrap(),
        "LOCAL_PROJECT_DATA"
    );
    assert!(
        execute(
            "list_files",
            &json!({"path":"."}),
            Some(&root),
            Mode::Chat,
            &sender
        )
        .await
        .unwrap()
        .contains("note.txt")
    );
    assert!(
        execute(
            "read_file",
            &json!({"path":"../outside.txt"}),
            Some(&root),
            Mode::Code,
            &sender
        )
        .await
        .is_err()
    );
    assert!(
        execute(
            "write_file",
            &json!({"path":"bad.txt","content":"bad"}),
            Some(&root),
            Mode::Chat,
            &sender
        )
        .await
        .is_err()
    );
    assert!(
        execute(
            "run_command",
            &json!({"command":"echo forbidden"}),
            Some(&root),
            Mode::Chat,
            &sender
        )
        .await
        .is_err()
    );
    assert!(!root.join("bad.txt").exists());
    assert!(events.try_recv().is_err());
}

#[tokio::test]
async fn workspace_writes_wait_for_approval_and_rejection_keeps_files_unchanged() {
    let root = project();
    let (sender, mut events) = mpsc::channel(8);
    let reject = async {
        if let Some(ProviderEvent::Approval(approval)) = events.recv().await {
            assert!(approval.description.contains("new.txt"));
            assert!(!root.join("new.txt").exists());
            approval.response.send(false).unwrap();
        } else {
            panic!("No approval request");
        }
    };
    let args = json!({"path":"new.txt","content":"new content"});
    let (result, _) = tokio::join!(
        execute("write_file", &args, Some(&root), Mode::Code, &sender),
        reject
    );
    assert!(result.is_err());
    assert!(!root.join("new.txt").exists());
    let accept = async {
        if let Some(ProviderEvent::Approval(approval)) = events.recv().await {
            approval.response.send(true).unwrap();
        } else {
            panic!("No approval request");
        }
    };
    let (result, _) = tokio::join!(
        execute("write_file", &args, Some(&root), Mode::Code, &sender),
        accept
    );
    assert!(result.is_ok());
    assert_eq!(
        std::fs::read_to_string(root.join("new.txt")).unwrap(),
        "new content"
    );
}

#[test]
fn absolute_outside_paths_and_symlink_escapes_are_confined_to_full_access() {
    let root = project();
    let outside = root.parent().unwrap().join("outside.txt");
    std::fs::write(&outside, "private").unwrap();
    assert!(resolve_path(&root, outside.to_str().unwrap(), Mode::Code, false).is_err());
    assert_eq!(
        resolve_path(&root, outside.to_str().unwrap(), Mode::FullAccess, false).unwrap(),
        outside.canonicalize().unwrap()
    );
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&outside, root.join("link.txt")).unwrap();
        assert!(resolve_path(&root, "link.txt", Mode::Code, true).is_err());
        std::os::unix::fs::symlink(
            root.parent().unwrap().join("missing-outside.txt"),
            root.join("dangling.txt"),
        )
        .unwrap();
        assert!(resolve_path(&root, "dangling.txt", Mode::Code, true).is_err());
    }
}

#[tokio::test]
async fn exact_edits_reject_read_only_and_ambiguous_batches_without_changes() {
    let root = project();
    let path = root.join("note.txt");
    std::fs::write(&path, "before aaa").unwrap();
    let (sender, mut events) = mpsc::channel(8);
    let edits = json!({"path": "note.txt", "edits": [
        {"old": "before", "new": "after"},
        {"old": "aa", "new": "x"}
    ]});
    for mode in [Mode::Chat, Mode::Code] {
        let result = execute("edit_file", &edits, Some(&root), mode, &sender).await;
        assert!(result.is_err());
        assert!(events.try_recv().is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "before aaa");
    }
}

#[tokio::test]
async fn exact_edits_recheck_the_approved_file_and_apply_the_batch_together() {
    let root = project();
    let path = root.join("step.rs");
    std::fs::write(&path, "let count = 1;\nlet size = 2;\n").unwrap();
    let (sender, mut events) = mpsc::channel(8);
    let edits = json!({"path": "step.rs", "edits": [
        {"old": "count = 1", "new": "count = 3"},
        {"old": "size = 2", "new": "size = 4"}
    ]});
    let (stale, ()) = tokio::time::timeout(Duration::from_secs(5), async {
        tokio::join!(
            execute("edit_file", &edits, Some(&root), Mode::Code, &sender),
            async {
                let Some(ProviderEvent::Approval(approval)) = events.recv().await else {
                    panic!("Expected edit approval");
                };
                assert!(approval.description.contains("count = 3"));
                assert!(approval.description.contains("size = 4"));
                std::fs::write(&path, "let count = 1;\nlet size = 2;\n// concurrent edit\n")
                    .unwrap();
                approval.response.send(true).unwrap();
            }
        )
    })
    .await
    .unwrap();
    assert!(stale.unwrap_err().contains("file changed"));
    assert!(
        std::fs::read_to_string(&path)
            .unwrap()
            .contains("count = 1")
    );

    let result = execute("edit_file", &edits, Some(&root), Mode::FullAccess, &sender).await;
    assert!(result.unwrap().contains("Applied 2 edits"));
    assert_eq!(
        std::fs::read_to_string(path).unwrap(),
        "let count = 3;\nlet size = 4;\n// concurrent edit\n"
    );
    assert_eq!(std::fs::read_dir(root).unwrap().count(), 1);
}

#[tokio::test]
async fn command_timeout_is_validated_and_shown_in_approval() {
    let root = project();
    let (sender, mut events) = mpsc::channel(1);
    for invalid in [json!(0), json!(MAX_COMMAND_TIMEOUT + 1), json!("600")] {
        let result = execute(
            "run_command",
            &json!({"command":"echo LOCAL_COMMAND_OK", "timeout_seconds":invalid}),
            Some(&root),
            Mode::Code,
            &sender,
        )
        .await;
        assert!(result.unwrap_err().contains("timeout_seconds"));
        assert!(events.try_recv().is_err());
    }

    let arguments = json!({"command":"echo LOCAL_COMMAND_OK", "timeout_seconds":600});
    let (result, ()) = tokio::join!(
        execute("run_command", &arguments, Some(&root), Mode::Code, &sender),
        async {
            let Some(ProviderEvent::Approval(approval)) = events.recv().await else {
                panic!("Expected command approval");
            };
            assert!(approval.description.contains("Timeout: 600 seconds."));
            approval.response.send(true).unwrap();
        }
    );
    assert!(result.unwrap().contains("LOCAL_COMMAND_OK"));
}

#[tokio::test]
async fn commands_are_real_and_read_only_tool_definitions_exclude_mutations() {
    let root = project();
    assert!(
        run_command("echo LOCAL_COMMAND_OK", &root, DEFAULT_COMMAND_TIMEOUT)
            .await
            .unwrap()
            .contains("LOCAL_COMMAND_OK")
    );
    let read_only = definitions(Mode::Chat).to_string();
    assert!(read_only.contains("read_file"));
    assert!(!read_only.contains("write_file"));
    assert!(!read_only.contains("run_command"));
    let available = definitions(Mode::Code).to_string();
    assert!(available.contains(if cfg!(windows) {
        "cmd.exe /D /S /C"
    } else {
        "/bin/sh -c"
    }));
}

#[tokio::test]
async fn output_capture_drains_large_streams_without_closing_the_writer() {
    use tokio::io::AsyncWriteExt;

    let (mut writer, reader) = tokio::io::duplex(1024);
    let payload = vec![b'x'; COMMAND_OUTPUT_LIMIT as usize * 3];
    let write = async {
        writer.write_all(&payload).await.unwrap();
        writer.shutdown().await.unwrap();
    };
    let (output, ()) = tokio::time::timeout(Duration::from_secs(5), async {
        tokio::join!(read_output(reader), write)
    })
    .await
    .expect("large output should drain without a deadlock");
    let output = output.unwrap();
    assert!(output.starts_with(&"x".repeat(COMMAND_OUTPUT_LIMIT as usize)));
    assert!(output.contains("Output truncated after 64 KiB"));
    assert!(output.len() < COMMAND_OUTPUT_LIMIT as usize + 100);
}

#[tokio::test]
async fn output_at_or_below_the_limit_is_preserved_without_a_truncation_notice() {
    for payload in [
        Vec::new(),
        b"hello\n".to_vec(),
        vec![b'x'; COMMAND_OUTPUT_LIMIT as usize],
        vec![0xff, b'!'],
    ] {
        let output = read_output(payload.as_slice()).await.unwrap();
        assert_eq!(output, String::from_utf8_lossy(&payload));
    }
}

#[tokio::test]
async fn verbose_commands_finish_with_both_output_streams_bounded() {
    let root = project().canonicalize().unwrap();
    #[cfg(windows)]
    let command = r#"powershell.exe -NoProfile -NonInteractive -Command "$ErrorActionPreference = 'Stop'; $text = 'x' * (192 * 1024); [Console]::Out.Write($text); [Console]::Error.Write($text); Set-Content -LiteralPath finished.txt -Value DONE; exit 0""#;
    #[cfg(not(windows))]
    let command = "i=0; while [ $i -lt 4096 ]; do printf 'xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx'; printf 'yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy' >&2; i=$((i+1)); done; printf DONE > finished.txt";
    let output = run_command(command, &root, DEFAULT_COMMAND_TIMEOUT)
        .await
        .unwrap();
    assert!(
        output.starts_with("Exit: exit status: 0") || output.starts_with("Exit: exit code: 0"),
        "{}",
        &output[..output.len().min(80)]
    );
    assert!(
        root.join("finished.txt").exists(),
        "command did not finish: {}",
        &output[..output.len().min(1000)]
    );
    assert_eq!(output.matches("Output truncated after 64 KiB").count(), 2);
    assert!(output.len() < COMMAND_OUTPUT_LIMIT as usize * 2 + 300);
}

#[cfg(windows)]
#[tokio::test]
async fn windows_commands_preserve_quoted_paths_arguments_and_failure_status() {
    let root = project().canonicalize().unwrap();
    std::fs::write(
        root.join("command with spaces.cmd"),
        "@echo off\r\necho %1\r\nexit /b 7\r\n",
    )
    .unwrap();
    let output = run_command(
        r#""command with spaces.cmd" "two words & literal""#,
        &root,
        DEFAULT_COMMAND_TIMEOUT,
    )
    .await
    .unwrap();
    assert!(output.starts_with("Exit: exit code: 7"), "{output}");
    assert!(output.contains("\"two words & literal\""), "{output}");
    std::fs::write(root.join("text with spaces.txt"), "QUOTED_PATH_CONTENT").unwrap();
    let output = run_command(
        r#"type "text with spaces.txt""#,
        &root,
        DEFAULT_COMMAND_TIMEOUT,
    )
    .await
    .unwrap();
    assert!(output.starts_with("Exit: exit code: 0"), "{output}");
    assert!(output.contains("QUOTED_PATH_CONTENT"), "{output}");
}
