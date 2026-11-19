#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;

#[test]
fn imported_local_images_remain_attachments_and_only_the_first_prompt_anchors_the_turn() {
    let page = serde_json::json!({"data":[{"id":"turn","status":"completed","items":[
        {"type":"userMessage","content":[{"type":"localImage","path":"/fixture/image.png"}]},
        {"type":"agentMessage","text":"Initial reply."},
        {"type":"userMessage","content":[{"type":"text","text":"A steer in the same turn."}]}
    ]}]});
    let messages = redox::history::parse_turns(&page, "model").unwrap();
    assert_eq!(messages.len(), 3);
    assert_eq!(
        messages[0].attachments[0].path,
        std::path::Path::new("/fixture/image.png")
    );
    assert_eq!(messages[0].activity.turn_id, "turn");
    assert!(messages[2].activity.turn_id.is_empty());
}

#[tokio::test]
async fn codex_fork_verifies_the_cut_even_across_pages() {
    let root = std::env::temp_dir().join(format!("redox-history-edit-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&root).unwrap();
    for accepts_cutoff in [true, false] {
        let executable = root.join(if accepts_cutoff { "current" } else { "legacy" });
        std::fs::write(&executable, MOCK).unwrap();
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();

        let result = redox::history::fork_before(
            executable.to_str().unwrap(),
            "source".into(),
            "edited-turn".into(),
        )
        .await;
        if accepts_cutoff {
            let fork = result.unwrap();
            assert_eq!(fork.thread, "fork");
            assert_eq!(fork.source, "source");
            assert_eq!(fork.before_turn, "edited-turn");
        } else {
            assert!(matches!(result, Err(error) if error.contains("verify the rewind")));
        }
    }
    std::fs::remove_dir_all(root).unwrap();
}

const MOCK: &str = r#"#!/usr/bin/env python3
import json
import os
import sys

for line in sys.stdin:
    request = json.loads(line)
    if "id" not in request:
        continue
    method, params = request["method"], request["params"]
    if method == "initialize":
        result = {}
    elif method == "thread/fork":
        assert params == {"threadId": "source", "beforeTurnId": "edited-turn", "excludeTurns": True}
        result = {"thread": {"id": "fork"}}
    elif method == "thread/archive":
        assert params["threadId"] == "fork"
        result = {}
    elif method == "thread/turns/list":
        assert params["itemsView"] == "notLoaded"
        assert params["sortDirection"] == "desc"
        if params["threadId"] == "fork":
            assert params["limit"] == 1
            cutoff = os.path.basename(sys.argv[0]) == "current"
            result = {"data": [{"id": "earlier-turn" if cutoff else "later-turn"}]}
        elif params["cursor"] == "older":
            result = {"data": [{"id": "earlier-turn"}]}
        else:
            result = {"data": [{"id": "later-turn"}, {"id": "edited-turn"}], "nextCursor": "older"}
    else:
        raise AssertionError(method)
    print(json.dumps({"id": request["id"], "result": result}), flush=True)
"#;
