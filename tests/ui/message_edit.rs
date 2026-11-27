use super::*;
use egui::accesskit::{Action, ActionRequest, NodeId};
use egui::{Event as InputEvent, Key, Modifiers};

fn frame(ctx: &egui::Context, app: &mut ReDox, events: Vec<InputEvent>) -> egui::FullOutput {
    ctx.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1120., 760.),
            )),
            events,
            ..Default::default()
        },
        |ctx| app.render_test_frame(ctx),
    )
}

fn button(output: &egui::FullOutput, label: &str) -> NodeId {
    output
        .platform_output
        .accesskit_update
        .as_ref()
        .unwrap()
        .nodes
        .iter()
        .find(|(_, node)| {
            node.label() == Some(label)
                && !node.is_disabled()
                && node.supports_action(Action::Click)
        })
        .map(|(id, _)| *id)
        .unwrap_or_else(|| panic!("missing enabled {label} button"))
}

fn click(id: NodeId) -> Vec<InputEvent> {
    vec![InputEvent::AccessKitActionRequest(ActionRequest {
        action: Action::Click,
        target: id,
        data: None,
    })]
}

fn focused_text(ctx: &egui::Context, app: &mut ReDox, text: &str) -> egui::FullOutput {
    frame(
        ctx,
        app,
        vec![
            InputEvent::Key {
                key: Key::A,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::COMMAND,
            },
            InputEvent::Text(text.into()),
        ],
    )
}

#[test]
fn every_builtin_theme_can_edit_cancel_send_and_switch_an_inline_prompt() {
    let root = std::env::temp_dir().join(format!("redox-edit-ui-{}", Uuid::new_v4()));
    std::fs::create_dir(&root).unwrap();
    let cli = root.join("unavailable-cli");
    std::fs::write(&cli, "Local fixture.").unwrap();

    for theme in ["codex", "win98", "wii"] {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let mut app = ReDox::test_preview(&ctx, theme);
        app.prepare_test_scene(true);
        app.ui_preferences.hide_update_prompts = true;
        app.session.codex_path = cli.to_string_lossy().into_owned();
        app.runtime = Some(
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(1)
                .enable_all()
                .build()
                .unwrap(),
        );
        let original = app.session.active().unwrap().messages[0].content.clone();
        let original_reply = app.session.active().unwrap().messages[1].content.clone();
        let id = app.session.active().unwrap().messages[0].id;
        frame(&ctx, &mut app, vec![]);
        let initial = frame(&ctx, &mut app, vec![]);
        frame(&ctx, &mut app, click(button(&initial, "Edit message")));
        let opened = frame(&ctx, &mut app, vec![]);
        assert_eq!(app.message_edit.as_ref().unwrap().text, original, "{theme}");
        assert_ne!(
            button(&opened, "Send"),
            button(&opened, "Cancel"),
            "{theme}"
        );
        let typed = focused_text(&ctx, &mut app, "Cancelled edit");
        assert_eq!(
            app.message_edit.as_ref().unwrap().text,
            "Cancelled edit",
            "{theme}"
        );
        frame(&ctx, &mut app, click(button(&typed, "Cancel")));
        assert!(app.message_edit.is_none(), "{theme}");
        assert_eq!(
            app.session.active().unwrap().messages[0].content,
            original,
            "{theme}"
        );

        let restored = frame(&ctx, &mut app, vec![]);
        frame(&ctx, &mut app, click(button(&restored, "Edit message")));
        frame(&ctx, &mut app, vec![]);
        let typed = focused_text(&ctx, &mut app, "Revised prompt");
        frame(&ctx, &mut app, click(button(&typed, "Send")));
        assert!(app.message_edit.is_none(), "{theme}");
        let chat = app.session.active().unwrap();
        assert_eq!(chat.messages[0].content, "Revised prompt", "{theme}");
        assert_eq!(chat.prompt_version(id), Some((2, 2)), "{theme}");
        app.stop_response();
        let edited = frame(&ctx, &mut app, vec![]);
        frame(&ctx, &mut app, click(button(&edited, "Previous version")));
        let chat = app.session.active().unwrap();
        assert_eq!(chat.messages[0].content, original, "{theme}");
        assert_eq!(chat.messages[1].content, original_reply, "{theme}");
        let original = frame(&ctx, &mut app, vec![]);
        frame(&ctx, &mut app, click(button(&original, "Next version")));
        assert_eq!(
            app.session.active().unwrap().messages[0].content,
            "Revised prompt",
            "{theme}"
        );
    }
    std::fs::remove_dir_all(root).unwrap();
}
