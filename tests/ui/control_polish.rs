use super::*;
use egui::{Event, Modifiers, PointerButton, Pos2, Shape};

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
    let mut app = ReDox::design_preview(&ctx, "codex");
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
