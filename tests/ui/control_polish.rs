use super::*;
use egui::accesskit::{Action, ActionRequest};
use egui::{Event, Key, Modifiers, PointerButton, Pos2, Shape};

fn frame(
    context: &egui::Context,
    events: Vec<Event>,
    draw: impl FnMut(&egui::Context),
) -> egui::FullOutput {
    context.run(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1100.0, 760.0))),
            events,
            ..Default::default()
        },
        draw,
    )
}

fn has_text(output: &egui::FullOutput, label: &str) -> bool {
    output
        .shapes
        .iter()
        .any(|shape| matches!(&shape.shape, Shape::Text(text) if text.galley.text() == label))
}

fn key(key: Key) -> Event {
    Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    }
}

fn button(pos: Pos2, pressed: bool) -> Event {
    Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    }
}

fn sample_usage() -> UsageInfo {
    UsageInfo::parse(serde_json::json!({
        "rateLimitsByLimitId": {
            "codex": {
                "primary": {"usedPercent": 12.0, "windowDurationMins": 300},
                "secondary": {"usedPercent": 31.0, "windowDurationMins": 10080}
            },
            "review": {
                "primary": {"usedPercent": 11.0, "windowDurationMins": 1440}
            }
        }
    }))
    .unwrap()
}

fn usage_frame(
    ctx: &egui::Context,
    app: &mut ReDox,
    events: Vec<Event>,
) -> (egui::FullOutput, Rect) {
    let mut rect = Rect::NOTHING;
    let output = frame(ctx, events, |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| rect = app.usage_button(ui).rect);
    });
    (output, rect)
}

#[test]
fn usage_meter_shows_remaining_quota_and_opens_details() {
    let ctx = egui::Context::default();
    let mut app = ReDox::test_preview(&ctx, "codex");
    ctx.style_mut(|style| {
        style.interaction.tooltip_delay = 0.0;
        style.interaction.show_tooltips_only_when_still = false;
    });

    let (unknown, rect) = usage_frame(&ctx, &mut app, vec![]);
    assert!(
        !unknown
            .shapes
            .iter()
            .any(|shape| matches!(shape.shape, Shape::Path(_)))
    );
    app.usage = Some(sample_usage());
    let pos = rect.center();
    usage_frame(&ctx, &mut app, vec![Event::PointerMoved(pos)]);
    usage_frame(&ctx, &mut app, vec![]);
    let (hover, _) = usage_frame(&ctx, &mut app, vec![]);
    assert!(has_text(&hover, "Weekly window · 69% left"));

    let arc = hover
        .shapes
        .iter()
        .find_map(|shape| {
            if let Shape::Path(path) = &shape.shape {
                Some(&path.points)
            } else {
                None
            }
        })
        .unwrap();
    assert!(((arc[0] - pos).normalized() - vec2(0.0, -1.0)).length() < 0.02);
    let endpoint = (*arc.last().unwrap() - pos).normalized();
    assert!((endpoint - Vec2::angled(0.69 * TAU - FRAC_PI_2)).length() < 0.02);

    usage_frame(&ctx, &mut app, vec![button(pos, true)]);
    usage_frame(&ctx, &mut app, vec![button(pos, false)]);
    assert!(app.usage_open);
}

#[test]
fn square_attachment_button_opens_its_menu_in_every_theme() {
    for theme in crate::app::ui::renderers::RENDERERS
        .iter()
        .map(|renderer| renderer.id)
    {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let mut app = ReDox::test_preview(&ctx, theme);
        app.prepare_test_scene(false);
        let mut render = |events| frame(&ctx, events, |ctx| app.render_test_frame(ctx));
        render(vec![]);
        let output = render(vec![]);
        let (id, node) = output
            .platform_output
            .accesskit_update
            .unwrap()
            .nodes
            .into_iter()
            .find(|(_, node)| node.label() == Some("Add attachments"))
            .unwrap_or_else(|| panic!("missing attachment button in {theme}"));
        let bounds = node.bounds().unwrap();
        assert_eq!((bounds.width(), bounds.height()), (28.0, 28.0), "{theme}");

        render(vec![Event::AccessKitActionRequest(ActionRequest {
            action: Action::Click,
            target: id,
            data: None,
        })]);
        render(vec![]);
        let menu = render(vec![]);
        assert!(has_text(&menu, "Add files"), "{theme}");
        assert!(has_text(&menu, "Open project folder"), "{theme}");
        render(vec![key(Key::Escape)]);
        assert!(!has_text(&render(vec![]), "Add files"), "{theme}");
    }
}

#[test]
fn checkbox_uses_an_svg_and_preserves_label_click_and_keyboard_toggle() {
    let ctx = egui::Context::default();
    let _ = ReDox::test_preview(&ctx, "codex");
    let mut checked = false;
    let draw = |events, value: &mut bool| {
        let mut response = None;
        let output = frame(&ctx, events, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                response = Some(theme::checkbox(ui, value, "Toggle preference"));
            });
        });
        (output, response.unwrap())
    };

    let (_, response) = draw(vec![], &mut checked);
    let label = Pos2::new(response.rect.right() - 3.0, response.rect.center().y);
    draw(
        vec![Event::PointerMoved(label), button(label, true)],
        &mut checked,
    );
    let (output, response) = draw(vec![button(label, false)], &mut checked);
    assert!(checked && response.changed());
    assert!(
        output
            .shapes
            .iter()
            .any(|shape| matches!(shape.shape, Shape::Mesh(_)))
    );
    response.request_focus();
    let (_, response) = draw(vec![key(Key::Space)], &mut checked);
    assert!(!checked && response.changed());
}
