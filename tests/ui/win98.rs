use super::*;

#[test]
fn dark_win98_keeps_classic_controls_and_renders_chat_and_terminal() {
    let context = egui::Context::default();
    let mut app = ReDox::new_internal(
        &eframe::CreationContext::_new_kittest(context.clone()),
        None,
        Default::default(),
        Some(DARK_ID),
    );
    let palette = app.active_ui.config.colors().unwrap();
    assert_eq!(app.active_ui.config.layout, ID);
    assert!(context.style().visuals.dark_mode);
    assert_eq!(palette.background, BLACK);
    assert_eq!(palette.input_background, BLACK);
    assert_eq!(palette.text, WHITE);
    assert_eq!(
        context.style().visuals.extreme_bg_color,
        palette.input_background
    );
    for surface in [
        palette.background,
        palette.rail,
        palette.sidebar,
        palette.surface,
        palette.input_background,
    ] {
        assert!(contrast(palette.text, surface) >= 7.0);
    }
    assert!(contrast(palette.border, palette.sidebar) >= 3.0);
    assert!(contrast(palette.blue, palette.input_background) >= 7.0);
    for color in [
        palette.sidebar,
        palette.rail,
        palette.surface,
        palette.text,
        palette.menu_text,
        palette.muted,
        palette.border,
        palette.blue,
        context.style().visuals.selection.bg_fill,
        selected_fill(),
    ] {
        assert_eq!(color.r(), color.g());
        assert_eq!(color.g(), color.b());
    }

    let render = |app: &mut ReDox| {
        context.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1100.0, 700.0))),
                ..Default::default()
            },
            |context| app.draw_ui(context),
        )
    };
    app.session
        .active_mut()
        .unwrap()
        .messages
        .push(redox::domain::Message {
            id: uuid::Uuid::new_v4(),
            role: redox::domain::Role::User,
            content: "Chat bubble in dark mode".into(),
            status: redox::domain::MessageStatus::Complete,
            error: None,
            model: String::new(),
            attachments: Vec::new(),
            activity: Default::default(),
        });
    let chat = render(&mut app);
    assert!(chat.shapes.iter().any(|shape| {
        matches!(&shape.shape, egui::Shape::Rect(rect) if rect.fill == palette.input_background)
    }));
    assert!(chat.shapes.iter().any(|shape| {
        matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text() == "Chat")
    }));
    assert!(chat.shapes.iter().any(|shape| {
        matches!(&shape.shape, egui::Shape::Rect(rect) if rect.fill == BLACK && rect.rect.height() == 20.0 && rect.rect.width() > 700.0)
    }));
    let bubble_ink = chat
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Text(text) if text.galley.text() == "Chat bubble in dark mode" => {
                Some(text.visual_bounding_rect().center())
            }
            _ => None,
        })
        .unwrap();
    assert!(chat.shapes.iter().any(|shape| {
        matches!(&shape.shape, egui::Shape::Rect(rect) if rect.fill == palette.surface && rect.rect.contains(bubble_ink))
    }));

    super::super::win98_terminal::set_open(&mut app, &context, true);
    let terminal = render(&mut app);
    assert!(terminal.shapes.iter().any(|shape| {
        matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text() == "ReDox Terminal")
    }));
    assert!(
        terminal
            .shapes
            .iter()
            .any(|shape| { matches!(&shape.shape, egui::Shape::Rect(rect) if rect.fill == BLACK) })
    );

    let light = egui::Context::default();
    let _app = ReDox::new_internal(
        &eframe::CreationContext::_new_kittest(light.clone()),
        None,
        Default::default(),
        Some(ID),
    );
    assert!(!light.style().visuals.dark_mode);
    assert_eq!(light.style().visuals.extreme_bg_color, WHITE);
}

fn contrast(a: Color32, b: Color32) -> f32 {
    let luminance = |color| {
        let [r, g, b, _] = egui::Rgba::from(color).to_array();
        0.2126 * r + 0.7152 * g + 0.0722 * b
    };
    let (a, b) = (luminance(a), luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}
