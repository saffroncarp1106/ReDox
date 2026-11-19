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
