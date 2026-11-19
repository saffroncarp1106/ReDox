#![cfg(feature = "design-export")]

use redox_studio::{document::Document, scenes::SceneId};
use std::process::Command;

#[test]
fn preview_cli_exports_native_scenes_at_the_requested_viewport() {
    let directory =
        std::env::temp_dir().join(format!("redox-native-preview-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&directory).unwrap();
    let preview = || {
        let mut command = Command::new(env!("CARGO_BIN_EXE_redox-studio-native-preview"));
        command
            .current_dir(&directory)
            .env("LOCALAPPDATA", &directory)
            .env_remove("REDOX_UI_FOLDER");
        command
    };

    for (scene, expected_label) in [
        (SceneId::Welcome, None),
        (
            SceneId::Conversation,
            Some("Dummy text. Can we try this layout?"),
        ),
        (SceneId::Account, Some("Studio preview account")),
    ] {
        let output = directory.join(format!("{}.redoxskin", scene.id()));
        let finished = preview()
            .args(["codex", scene.id(), "1234", "768"])
            .arg(&output)
            .output()
            .unwrap();
        assert!(
            finished.status.success(),
            "{}: {}",
            scene.id(),
            String::from_utf8_lossy(&finished.stderr)
        );
        let document = Document::load(&output).unwrap();
        let source = document.source.unwrap();
        assert_eq!(source.theme, "codex");
        assert_eq!(source.scene, scene);
        let page = &document.pages[0];
        assert_eq!([page.nodes[0].width, page.nodes[0].height], [1234., 768.]);
        assert!(page.nodes.iter().any(|node| node.artwork.is_some()));
        if let Some(label) = expected_label {
            assert!(page.nodes.iter().any(|node| node.text.contains(label)));
        }
        std::fs::remove_file(output).unwrap();
    }

    let failed_output = directory.join("invalid.redoxskin");
    let failed = preview()
        .args(["codex", "welcome", "99999", "768"])
        .arg(&failed_output)
        .output()
        .unwrap();
    assert!(!failed.status.success());
    assert!(!failed_output.exists());
    assert!(std::fs::read_dir(&directory).unwrap().next().is_none());
    std::fs::remove_dir(directory).unwrap();
}
