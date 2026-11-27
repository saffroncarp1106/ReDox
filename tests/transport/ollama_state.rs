use super::*;
use crate::perf_measure as measure;

fn original_prompt_size(messages: &[Value], definitions: &Value) -> usize {
    messages
        .iter()
        .map(|message| {
            let mut text = message.clone();
            let images = text
                .as_object_mut()
                .and_then(|object| object.remove("images"));
            let image_tokens = images
                .as_ref()
                .and_then(Value::as_array)
                .map_or(0, |images| images.len() * 4096);
            text.to_string().len() + image_tokens
        })
        .sum::<usize>()
        + definitions.to_string().len()
}

#[test]
fn prompt_estimates_preserve_json_bytes_and_image_allowances() {
    let messages = vec![
        json!({"role":"user", "content":"Quotes: \" \\ \n\t\u{0000} café 🦀"}),
        json!({"role":"assistant", "tool_calls":[{"function":{"name":"read_file", "arguments":{"path":"src/main.rs", "line":42}}}]}),
        json!({"role":"tool", "content":{"images":["nested fields still count"], "values":[true, false, null, -1, 1.25]}}),
        json!({"role":"user", "content":"image", "images":["first", "second"]}),
        json!({"images":[]}),
        json!({"images":null}),
        json!({"images":"malformed but previously excluded"}),
        json!({"\"key\n":"escaped key", "empty":""}),
        json!({}),
        json!(null),
        json!("not an object"),
        json!([1, "array", {"images":["not a top-level field"]}]),
    ];
    for definitions in [
        json!(null),
        json!([]),
        json!([{"name":"tool", "description":"é\n\""}]),
    ] {
        for message in &messages {
            assert_eq!(
                prompt_size(std::slice::from_ref(message), &definitions),
                original_prompt_size(std::slice::from_ref(message), &definitions),
                "message: {message}"
            );
        }
        assert_eq!(
            prompt_size(&messages, &definitions),
            original_prompt_size(&messages, &definitions)
        );
        assert_eq!(
            prompt_size(&[], &definitions),
            original_prompt_size(&[], &definitions)
        );
    }
}

#[test]
fn image_payload_length_does_not_change_the_context_allowance() {
    let mut message = json!({"role":"user", "content":"Inspect", "images":["small"]});
    let expected = prompt_size(std::slice::from_ref(&message), &json!([]));
    message["images"][0] = json!("x".repeat(4 * 1024 * 1024));
    assert_eq!(prompt_size(&[message], &json!([])), expected);
}

#[test]
#[ignore = "manual timing comparison; not an end-to-end client benchmark"]
fn prompt_estimation_benchmark() {
    use std::hint::black_box;

    let definitions = json!([{"name":"read_file", "parameters":{"path":{"type":"string"}}}]);
    for (name, messages) in [
        (
            "text_64k",
            vec![json!({"role":"user", "content":"x".repeat(64 * 1024)})],
        ),
        (
            "image_4m",
            vec![
                json!({"role":"user", "content":"Inspect", "images":["x".repeat(4 * 1024 * 1024)]}),
            ],
        ),
    ] {
        assert_eq!(
            prompt_size(&messages, &definitions),
            original_prompt_size(&messages, &definitions)
        );
        measure::paired(
            &format!("ollama_{name}"),
            || {
                measure::timed(64, || {
                    black_box(original_prompt_size(
                        black_box(&messages),
                        black_box(&definitions),
                    ));
                })
            },
            || {
                measure::timed(64, || {
                    black_box(prompt_size(black_box(&messages), black_box(&definitions)));
                })
            },
        );
    }
}

#[test]
#[ignore = "release benchmark with a synthetic 6 MiB Ollama journal"]
fn checkpoint_save_benchmark() {
    let path = checkpoint_path(uuid::Uuid::new_v4()).unwrap();
    let journal = Journal {
        path: Some(path.clone()),
        progress: Progress {
            task: "Synthetic benchmark".into(),
            actions: (0..96)
                .map(|index| {
                    format!(
                        "Tool result {index}\n{}",
                        "Some recorded output.\n".repeat(2978)
                    )
                })
                .collect(),
            ..Progress::default()
        },
    };
    let mut times = Vec::new();
    for _ in 0..16 {
        let start = std::time::Instant::now();
        journal.save().unwrap();
        times.push(start.elapsed().as_secs_f64() * 1_000.0);
    }
    times.sort_by(f64::total_cmp);
    let written = std::fs::metadata(&path).unwrap().len();
    println!(
        "CHECKPOINT_SAVE {written} bytes median_ms {:.3}",
        times[times.len() / 2]
    );
    std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
}

#[tokio::test]
async fn legacy_chat_recovers_the_latest_compression_handoff() {
    let path = checkpoint_path(uuid::Uuid::new_v4()).unwrap();
    let journal = Journal::open(path.clone(), None, "Previous task".into(), false)
        .await
        .unwrap();
    journal
        .save_handoff(
            "## Remaining tasks\n\nNothing pending.\n\n## Work completed\n\nRECOVERED_DECISION",
        )
        .unwrap();
    journal
        .save_handoff("## Remaining tasks\n\nOllama reached the 32-round response limit.")
        .unwrap();
    let reloaded = Journal::open(path, None, "Follow-up".into(), true)
        .await
        .unwrap();
    let recall = reloaded.progress.recall.unwrap();
    assert!(recall.summary.contains("RECOVERED_DECISION"));
    assert!(recall.through.is_none());
    assert_eq!(reloaded.progress.compressions, 0);
}
