use super::*;

#[test]
fn unchanged_reads_and_cycles_allow_one_recovery_then_stop() {
    let mut guard = Guard::new();
    for expected in [None, None, Some("recover"), Some("stalled")] {
        assert_eq!(
            guard.observe("read_file", &json!({"path":"src/lib.rs"}), "unchanged"),
            expected
        );
    }
    let mut guard = Guard::new();
    let mut actions = Vec::new();
    for _ in 0..3 {
        actions.push(guard.observe("read_file", &json!({"path":"a"}), "a"));
        actions.push(guard.observe("read_file", &json!({"path":"b"}), "b"));
    }
    assert_eq!(actions.last(), Some(&Some("recover")));
    assert_eq!(
        guard.observe("read_file", &json!({"path":"a"}), "a"),
        Some("stalled")
    );
}

#[test]
fn progress_has_no_fixed_tool_or_truncation_limit() {
    let mut guard = Guard::new();
    for index in 0..100 {
        assert_eq!(
            guard.observe("read_file", &json!({"path":"a"}), &index.to_string()),
            None
        );
    }
    assert_eq!(guard.calls, 100);
    for index in 0..10 {
        assert!(!guard.truncated(&index.to_string()));
    }
    assert!(!guard.truncated("same output"));
    assert!(!guard.truncated(" same output "));
    assert!(guard.truncated("same output"));
}

#[test]
fn command_evidence_records_nonzero_exit_without_claiming_success() {
    let item = evidence(
        "run_command",
        &json!({"command":"cargo test"}),
        &Ok("Exit: exit code: 1\nstdout...".into()),
    );
    assert_eq!(item["result"], "Exit: exit code: 1");
    assert_eq!(clipped("ééé", 3), "é");
}

#[tokio::test]
async fn git_inventory_distinguishes_new_work_from_existing_edits() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("temp/verification/inventory")
        .join(uuid::Uuid::new_v4().to_string());
    std::fs::create_dir_all(&root).unwrap();
    let root = root.canonicalize().unwrap();
    for args in [
        vec!["init", "--quiet"],
        vec![
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.invalid",
            "commit",
            "--allow-empty",
            "--quiet",
            "-m",
            "fixture",
        ],
    ] {
        let status = tokio::process::Command::new("git")
            .args(args)
            .current_dir(&root)
            .status()
            .await
            .unwrap();
        assert!(status.success());
    }
    std::fs::write(root.join("existing.txt"), "existing unsaved work").unwrap();
    let before = snapshot(&root).await.unwrap();
    std::fs::write(root.join("new.txt"), "worker output").unwrap();
    let after = snapshot(&root).await.unwrap();
    assert_eq!(before.get("existing.txt"), after.get("existing.txt"));
    assert_ne!(before.get("new.txt"), after.get("new.txt"));
    std::fs::write(root.join("existing.txt"), "worker correction").unwrap();
    let corrected = snapshot(&root).await.unwrap();
    assert_ne!(after.get("existing.txt"), corrected.get("existing.txt"));
}
