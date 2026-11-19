use super::*;
use egui::accesskit::{Action, ActionRequest, NodeId};
use egui::{Event, Modifiers, PointerButton, Pos2, Rect, Shape, TextureId};
use redox_studio::scenes::{SceneId, THEMES};

fn frame(ctx: &egui::Context, app: &mut ReDox, time: f64, events: Vec<Event>) -> egui::FullOutput {
    ctx.run(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1120.0, 760.0))),
            time: Some(time),
            events,
            ..Default::default()
        },
        |ctx| app.render_design(ctx),
    )
}

fn copy_buttons(output: &egui::FullOutput) -> Vec<(NodeId, Rect)> {
    let mut buttons: Vec<_> = output
        .platform_output
        .accesskit_update
        .as_ref()
        .unwrap()
        .nodes
        .iter()
        .filter(|(_, node)| node.label() == Some("Copy response"))
        .map(|(id, node)| {
            let rect = node.bounds().unwrap();
            (
                *id,
                Rect::from_min_max(
                    egui::pos2(rect.x0 as f32, rect.y0 as f32),
                    egui::pos2(rect.x1 as f32, rect.y1 as f32),
                ),
            )
        })
        .collect();
    buttons.sort_by(|a, b| a.1.top().total_cmp(&b.1.top()));
    buttons
}

fn icon_at(output: &egui::FullOutput, rect: Rect) -> TextureId {
    output
        .shapes
        .iter()
        .find_map(|clipped| {
            let Shape::Mesh(mesh) = &clipped.shape else {
                return None;
            };
            let bounds = mesh.calc_bounds();
            (rect.contains_rect(bounds) && bounds.size() == vec2(16.0, 16.0))
                .then_some(mesh.texture_id)
        })
        .expect("message action icon")
}

fn has_tooltip(output: &egui::FullOutput, label: &str) -> bool {
    output
        .shapes
        .iter()
        .any(|clipped| matches!(&clipped.shape, Shape::Text(text) if text.galley.text() == label))
}

fn check_clipboard(output: &egui::FullOutput, expected: &str) {
    let commands: Vec<_> = output
        .platform_output
        .commands
        .iter()
        .filter_map(|command| match command {
            egui::OutputCommand::CopyText(text) => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(commands, [expected]);
}

#[test]
fn copy_response_confirms_and_resets_in_every_theme() {
    for theme in THEMES {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let mut app = ReDox::design_preview(&ctx, theme);
        app.prepare_design_scene(SceneId::Conversation);
        ctx.style_mut(|style| {
            style.interaction.tooltip_delay = 0.0;
            style.interaction.show_tooltips_only_when_still = false;
        });
        let chat = app.session.active_mut().unwrap();
        let mut other = chat.messages[1].clone();
        other.id = uuid::Uuid::new_v4();
        other.content = "Another reply".into();
        chat.messages.push(other);
        let content = chat.messages[1].content.clone();
        frame(&ctx, &mut app, 0.0, vec![]);
        let initial = frame(&ctx, &mut app, 0.1, vec![]);
        let buttons = copy_buttons(&initial);
        let &[(button, rect), (_, other)] = buttons.as_slice() else {
            panic!("expected two copy buttons in {theme}");
        };
        let copy = icon_at(&initial, rect);
        let pos = rect.center();
        let click = |pressed| Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };

        frame(
            &ctx,
            &mut app,
            0.2,
            vec![Event::PointerMoved(pos), click(true)],
        );
        let clicked = frame(&ctx, &mut app, 0.25, vec![click(false)]);
        check_clipboard(&clicked, &content);
        let check = icon_at(&clicked, rect);
        assert_ne!(check, copy, "{theme}");
        assert_eq!(icon_at(&clicked, other), copy, "{theme}");

        let confirming = frame(&ctx, &mut app, 0.5, vec![]);
        assert!(has_tooltip(&confirming, "Copied"), "{theme}");
        let repeated = frame(
            &ctx,
            &mut app,
            0.8,
            vec![Event::AccessKitActionRequest(ActionRequest {
                action: Action::Click,
                target: button,
                data: None,
            })],
        );
        check_clipboard(&repeated, &content);
        let away = pos + vec2(85.0, 0.0);
        let still_copied = frame(&ctx, &mut app, 1.3, vec![Event::PointerMoved(away)]);
        assert_eq!(icon_at(&still_copied, rect), check, "{theme}");
        assert!(has_tooltip(&still_copied, "Copied"), "{theme}");

        frame(&ctx, &mut app, 1.4, vec![Event::PointerMoved(pos)]);
        let cleared = frame(&ctx, &mut app, 1.85, vec![]);
        assert_eq!(icon_at(&cleared, rect), copy, "{theme}");
        assert!(!has_tooltip(&cleared, "Copied"), "{theme}");
        assert!(!has_tooltip(&cleared, "Copy response"), "{theme}");

        frame(&ctx, &mut app, 2.0, vec![Event::PointerMoved(away)]);
        frame(&ctx, &mut app, 2.1, vec![Event::PointerMoved(pos)]);
        let hover = frame(&ctx, &mut app, 2.3, vec![]);
        assert!(has_tooltip(&hover, "Copy response"), "{theme}");
    }
}
