use std::collections::BTreeSet;

use serde_json::{Value, json};

pub(super) const REASONING_TRIALS: usize = 10;

pub(super) struct ReasoningTrial {
    pub prompt: String,
    pub expected: Value,
    pub fixture: bool,
}

pub(super) fn reasoning_trials(marker: &str) -> Vec<ReasoningTrial> {
    let mut seed = marker.bytes().fold(0_u64, |seed, byte| {
        seed.wrapping_mul(31).wrapping_add(u64::from(byte))
    });
    let mut next = || {
        seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
        seed >> 32
    };
    (0..REASONING_TRIALS)
        .map(|round| {
            let (prompt, expected, fixture) = match round {
                0 => {
                    let expected = json!({"acknowledged":true,"marker":marker,"type":"remember_marker"});
                    let prompt = format!(
                        "Set and remember the persistent benchmark marker for this session: '{marker}'. Acknowledge it with exactly one JSON object: {expected}"
                    );
                    (prompt, expected, false)
                }
                1 | 8 => (
                    "Call read_file with {\"path\":\"fixture.json\"}. After the tool returns, copy the resulting JSON object as your final answer.".into(),
                    json!({"release_channel":format!("canary-{:08x}", next())}),
                    true,
                ),
                even if even % 2 == 0 => {
                    let values = (0..12).map(|_| (next() % 18) as i32 - 8).collect::<Vec<_>>();
                    let positive = values.iter().copied().filter(|&v| v > 0).collect::<BTreeSet<_>>();
                    let total: i32 = positive.iter().sum();
                    let prompt = format!(
                        "Process this list of small integers: {values:?}. Reply with exactly one JSON object with keys \"sorted_unique_positive\" (the sorted, de-duplicated list of strictly positive integers) and \"sum\" (the sum of those distinct positive integers). No extra keys."
                    );
                    (prompt, json!({"sorted_unique_positive":positive,"sum":total}), false)
                }
                _ => {
                    let deltas = (0..36).map(|_| (next() % 16) as i32 - 8)
                        .map(|delta| if delta >= 0 { delta + 1 } else { delta }).collect::<Vec<_>>();
                    let running = deltas.iter().scan(0, |total, delta| {
                        *total += delta;
                        Some(*total)
                    }).collect::<Vec<_>>();
                    let prompt = format!(
                        "A counter starts at 0. Apply these deltas in order: {deltas:?}. Reply with exactly one JSON array containing the running total after each delta is applied. No extra prose."
                    );
                    (prompt, json!(running), false)
                }
            };
            ReasoningTrial { prompt, expected, fixture }
        })
        .collect()
}

pub(super) fn strict_pass(reply: &str, expected: &Value) -> bool {
    let Some((header, body)) = reply.trim().split_once('\n') else {
        return false;
    };
    let language = header.split_whitespace().next().unwrap_or_default();
    if language != "```" && !language.eq_ignore_ascii_case("```json") {
        return false;
    }
    let Some(body) = body.strip_suffix("\n```") else {
        return false;
    };
    serde_json::from_str::<Value>(body).is_ok_and(|answer| &answer == expected)
}
