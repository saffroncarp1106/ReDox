use super::*;
use pulldown_cmark::Options;

#[test]
fn math_delimiters_do_not_change_literal_code() {
    let source = "Formula \\(x^2\\).\n\n```html\n<sup>2</sup> &amp; \\[x\\]\n```\n\n`\\(literal\\)`\n\n\\[\\frac{1}{2}\\]";
    let normalized = normalize_delimiters(source);
    assert!(normalized.contains("```html\n<sup>2</sup> &amp; \\[x\\]\n```"));
    assert!(normalized.contains(r"`\(literal\)`"));
    let count = Parser::new_ext(&normalized, Options::ENABLE_MATH)
        .filter(|event| matches!(event, Event::InlineMath(_) | Event::DisplayMath(_)))
        .count();
    assert_eq!(count, 2);
}

#[test]
fn streaming_and_escaped_input_remains_readable() {
    for source in [
        r"Formula \[\frac{1}{",
        r"Literal \\(x\\)",
        r"$x^2$ and $$y=1$$",
    ] {
        assert_eq!(normalize_delimiters(source), source);
    }
    assert_eq!(normalize_delimiters("α \\(x\\)"), "α $x$");
}

#[test]
fn ph_fractions_and_aligned_equations_produce_pixels() {
    let font = MathFont::stix_two_math().unwrap();
    for source in [
        r"\mathrm{pH}=-\log_{10}[\mathrm{H}^{+}]=-\log_{10}(10^{-3})=3.00",
        r"\frac{[\mathrm{H}^{+}][\mathrm{A}^{-}]}{[\mathrm{HA}]}",
        r"\boxed{\mathrm{pH}=3.00}",
        r"\begin{aligned}x&=10^{-3}\\\mathrm{pH}&=-\log_{10}(x)\end{aligned}",
    ] {
        let bytes = math_png(source, false, Color32::WHITE, &font).unwrap();
        let image = image::load_from_memory(&bytes).unwrap().to_rgba8();
        assert!(image.width() > 10 && image.height() > 10);
        assert!(image.pixels().any(|pixel| pixel[3] > 0));
    }
}

#[test]
fn invalid_or_oversized_equations_fail_explicitly() {
    let font = MathFont::stix_two_math().unwrap();
    assert!(math_png(r"\not_a_real_command{x}", false, Color32::WHITE, &font).is_err());
    assert!(math_png(&"x".repeat(4097), false, Color32::WHITE, &font).is_err());
}

#[test]
fn combined_markdown_and_math_has_a_native_layout() {
    let context = egui::Context::default();
    let output = context.run(egui::RawInput::default(), |context| {
        egui::CentralPanel::default().show(context, |ui| {
            show(ui, include_str!("../../src/ui/previews/formatting.md"))
        });
    });
    assert!(!output.shapes.is_empty());
    assert!(!output.textures_delta.set.is_empty());
}

#[test]
fn wide_table_scrolls_without_moving_the_surrounding_prose() {
    let context = egui::Context::default();
    let source = format!(
        "Before\n\n| Left | Right |\n| --- | --- |\n| {} | End |\n\nAfter",
        "wide cell ".repeat(30)
    );
    let frame = |events| {
        context.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    vec2(300.0, 300.0),
                )),
                events,
                ..Default::default()
            },
            |context| {
                egui::CentralPanel::default().show(context, |ui| show(ui, &source));
            },
        )
    };
    let locate = |frame: &egui::FullOutput, label| {
        frame.shapes.iter().find_map(|entry| match &entry.shape {
            egui::Shape::Text(text) if text.galley.text() == label => {
                Some((text.pos, entry.clip_rect))
            }
            _ => None,
        })
    };
    frame(vec![]);
    let initial = frame(vec![]);
    let (before, clip) = locate(&initial, "Left").unwrap();
    let prose = locate(&initial, "After").unwrap().0;
    assert!(clip.right() <= 300.0);
    let mut scrolled = frame(vec![
        egui::Event::PointerMoved(egui::pos2(100.0, before.y)),
        egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: vec2(-3000.0, 0.0),
            modifiers: egui::Modifiers::NONE,
        },
    ]);
    for _ in 0..4 {
        scrolled = frame(vec![]);
    }
    let (end, clip) = locate(&scrolled, "End").expect("last cell after scrolling");
    assert!(clip.x_range().contains(end.x), "last cell is clipped");
    assert_eq!(locate(&scrolled, "After").unwrap().0, prose);
}
