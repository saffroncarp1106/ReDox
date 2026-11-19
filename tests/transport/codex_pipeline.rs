use super::*;
use crate::perf_measure as measure;

#[test]
fn rpc_payloads_move_without_copying_and_errors_keep_their_meaning() {
    let message = json!({"id":1, "result":{"text":"x".repeat(1024 * 1024), "items":[1,null,true]}});
    let pointer = message["result"]["text"].as_str().unwrap().as_ptr();
    let result = rpc_result(message).unwrap();
    assert_eq!(result["text"].as_str().unwrap().as_ptr(), pointer);
    assert_eq!(result["items"], json!([1, null, true]));
    assert_eq!(rpc_result(json!({"result":null})).unwrap(), Value::Null);
    assert_eq!(
        rpc_result(json!({"id":1})).unwrap_err(),
        "Codex response has no result."
    );
    assert_eq!(
        rpc_result(json!({"error":{"message":"Rejected"},"result":123})).unwrap_err(),
        "Rejected"
    );
    assert_eq!(
        rpc_result(json!({"error":null})).unwrap_err(),
        "Codex request failed."
    );
}

#[test]
fn streamed_items_suppress_duplicate_completions_and_keep_final_only_answers() {
    let mut answer = AnswerTracker::default();
    answer.delta("streamed", "hello ").unwrap();
    answer.delta("streamed", "世界").unwrap();
    assert!(!answer.complete("streamed", "hello 世界").unwrap());
    assert!(!answer.complete("empty", "").unwrap());
    assert!(answer.complete("final", "Only completed event").unwrap());
    assert!(!answer.complete("final", "Only completed event").unwrap());
    assert_eq!(answer.bytes, "hello 世界Only completed event".len());
    assert_eq!(answer.seen.len(), 2);
}

#[test]
fn text_limit_still_covers_all_streamed_and_final_only_items() {
    let mut answer = AnswerTracker::default();
    answer
        .delta("first", &"x".repeat(MAX_ANSWER_BYTES - 1))
        .unwrap();
    assert!(answer.complete("last", "x").unwrap());
    assert!(!answer.complete("first", "duplicate").unwrap());
    assert!(answer.delta("last", "x").unwrap_err().contains("8 MiB"));
}

#[tokio::test]
async fn large_protocol_records_release_the_buffer_and_following_records_still_decode() {
    let large = json!({"result":"x".repeat(1024 * 1024)});
    let input = format!("{large}\n{{\"id\":1}}\n{{\"id\":2}}\n");
    let mut reader = BufReader::new(input.as_bytes());
    let mut record = Vec::new();
    assert_eq!(
        read_message_buffered(&mut reader, &mut record)
            .await
            .unwrap(),
        large
    );
    assert!(record.capacity() <= RETAINED_RECORD_BYTES);
    assert_eq!(
        read_message_buffered(&mut reader, &mut record)
            .await
            .unwrap()["id"],
        1
    );
    let small_capacity = record.capacity();
    assert!(small_capacity > 0);
    assert_eq!(
        read_message_buffered(&mut reader, &mut record)
            .await
            .unwrap()["id"],
        2
    );
    assert_eq!(record.capacity(), small_capacity);
}

#[test]
fn notification_filter_keeps_visible_streams_errors_and_lifecycle_events() {
    let params = initialize_params();
    let excluded = params["capabilities"]["optOutNotificationMethods"]
        .as_array()
        .unwrap();
    assert!(excluded.contains(&json!("item/commandExecution/outputDelta")));
    for method in [
        "item/agentMessage/delta",
        "item/started",
        "item/completed",
        "turn/started",
        "turn/completed",
        "turn/diff/updated",
        "thread/tokenUsage/updated",
        "error",
    ] {
        assert!(!excluded.contains(&json!(method)), "must retain {method}");
    }
}

#[tokio::test]
#[ignore = "read-only installed Codex handshake; does not start a model turn"]
async fn installed_codex_accepts_filtered_notifications() {
    let executable = std::env::var("REDOX_PIPELINE_CODEX").unwrap_or_else(|_| find_executable());
    let mut rpc = Rpc::connect(&executable).await.unwrap();
    rpc.close().await;
}

#[test]
#[ignore = "manual paired pipeline benchmark; no model requests"]
fn codex_pipeline_benchmark() {
    use std::hint::black_box;
    use std::time::Instant;

    let input =
        json!({"id":1,"result":{"thread":{"turns":[{"text":"x".repeat(4 * 1024 * 1024)}]}}});
    let run_rpc = |old: bool| {
        let messages: Vec<_> = (0..8).map(|_| input.clone()).collect();
        let start = Instant::now();
        for message in messages {
            if old {
                black_box(message.get("result").cloned().unwrap());
            } else {
                black_box(rpc_result(black_box(message)).unwrap());
            }
        }
        start.elapsed().as_secs_f64() * 1e9 / 8.0
    };
    measure::paired("codex_rpc_4m", || run_rpc(true), || run_rpc(false));

    let chunk = "x".repeat(1024);
    let before = || {
        measure::timed(16, || {
            let mut messages = std::collections::HashMap::<String, String>::new();
            let mut bytes = 0usize;
            for _ in 0..4096 {
                let text = black_box(chunk.as_str());
                let id = black_box("message-id");
                bytes = bytes.saturating_add(text.len());
                assert!(bytes <= MAX_ANSWER_BYTES);
                messages.entry(id.to_owned()).or_default().push_str(text);
            }
            black_box(messages);
        })
    };
    let after = || {
        measure::timed(16, || {
            let mut answer = AnswerTracker::default();
            for _ in 0..4096 {
                answer
                    .delta(black_box("message-id"), black_box(chunk.as_str()))
                    .unwrap();
            }
            black_box(answer);
        })
    };
    measure::paired("codex_stream_4m", before, after);
}
