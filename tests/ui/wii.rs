use super::*;

#[test]
fn light_controls_keep_wii_rounding_and_do_not_use_classic_scrollbars() {
    let context = egui::Context::default();
    theme::apply(&context, ID);
    let _ = context.run(egui::RawInput::default(), |context| {
        egui::CentralPanel::default().show(context, |ui| {
            assert!(!ui.visuals().dark_mode);
            assert!(!theme::is_classic(ui));
            assert!(theme::is_wii(context));
            assert_eq!(ui.spacing().scroll.bar_width, 8.0);
            assert_eq!(ui.visuals().widgets.hovered.bg_stroke.color, BLUE);
        });
    });
}

#[test]
fn exported_player_sprites_rasterize_with_transparency_and_distinct_tints() {
    let mut images = Vec::new();
    for sprite in [
        include_bytes!("../../src/ui/assets/wii/cursor-blue.svg").as_slice(),
        include_bytes!("../../src/ui/assets/wii/cursor-red.svg").as_slice(),
        include_bytes!("../../src/ui/assets/wii/cursor-green.svg").as_slice(),
        include_bytes!("../../src/ui/assets/wii/cursor-gold.svg").as_slice(),
    ] {
        let image = egui_extras::image::load_svg_bytes(sprite, &Default::default()).unwrap();
        assert_eq!(image.size, [85, 126]);
        assert!(image.pixels.iter().any(|pixel| pixel.a() == 0));
        assert!(image.pixels.iter().any(|pixel| pixel.a() == 255));
        images.push(image.pixels);
    }
    for index in 1..images.len() {
        assert_ne!(images[0], images[index]);
    }
}

#[test]
fn wii_controls_support_keyboard_activation_and_ignore_disabled_clicks() {
    let context = egui::Context::default();
    theme::apply(&context, ID);
    let mut activated = false;
    for frame in 0..2 {
        let events = if frame == 1 {
            vec![egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }]
        } else {
            Vec::new()
        };
        let _ = context.run(
            egui::RawInput {
                events,
                ..Default::default()
            },
            |context| {
                egui::CentralPanel::default().show(context, |ui| {
                    let response = button(ui, "New chat", 110.0, false);
                    if frame == 0 {
                        response.request_focus();
                    }
                    activated |= response.clicked();
                    let disabled = ui
                        .add_enabled_ui(false, |ui| button(ui, "Unavailable", 110.0, false))
                        .inner;
                    assert!(!disabled.enabled());
                    assert!(!disabled.clicked());
                });
            },
        );
    }
    assert!(activated);
}

#[test]
fn wii_preferences_round_trip_without_affecting_other_layouts() {
    let mut prefs = super::super::Preferences::default();
    prefs.wii.sounds = false;
    prefs.wii.motion = false;
    let restored: super::super::Preferences =
        serde_json::from_str(&serde_json::to_string(&prefs).unwrap()).unwrap();
    assert!(!restored.wii.sounds);
    assert!(!restored.wii.motion);
    assert_eq!(restored.ui, "codex");
}

#[test]
fn blue_pointer_preserves_text_and_resize_cursors() {
    let context = egui::Context::default();
    theme::apply(&context, ID);
    for cursor in [
        egui::CursorIcon::Default,
        egui::CursorIcon::PointingHand,
        egui::CursorIcon::Text,
        egui::CursorIcon::ResizeHorizontal,
    ] {
        let output = context.run(
            egui::RawInput {
                events: vec![egui::Event::PointerMoved(egui::pos2(60.0, 60.0))],
                ..Default::default()
            },
            |context| {
                egui::CentralPanel::default().show(context, |ui| {
                    ui.label("Pointer preview");
                });
                context.set_cursor_icon(cursor);
                finish_frame(context, false, None, false, true);
            },
        );
        let expected = if matches!(
            cursor,
            egui::CursorIcon::Default | egui::CursorIcon::PointingHand
        ) {
            egui::CursorIcon::None
        } else {
            cursor
        };
        assert_eq!(output.platform_output.cursor_icon, expected);
    }
}

#[test]
fn channels_fit_the_minimum_main_area_and_scroll_in_narrow_project_windows() {
    for size in [vec2(559.0, 300.0), vec2(228.0, 300.0)] {
        let context = egui::Context::default();
        theme::apply(&context, ID);
        let _ = context.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, size)),
                ..Default::default()
            },
            |context| {
                egui::CentralPanel::default()
                    .frame(Frame::NONE)
                    .show(context, |ui| {
                        home_channels(ui, "What can I help with?");
                        assert!(ui.min_rect().width() <= size.x + 0.5);
                        assert!(ui.min_rect().height() <= size.y + 0.5);
                    });
            },
        );
    }
}

#[test]
fn channel_hover_reuses_the_filled_contour_at_each_scale() {
    for (width, scale) in [(244.0, 1.0), (244.0, 2.0), (158.0, 1.5)] {
        let context = egui::Context::default();
        theme::apply(&context, ID);
        context.data_mut(|data| {
            data.insert_temp(
                egui::Id::new(SETTINGS_KEY),
                Settings {
                    sounds: false,
                    motion: false,
                },
            );
        });

        let draw = |events| {
            let mut input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(300.0, 180.0))),
                events,
                ..Default::default()
            };
            input
                .viewports
                .get_mut(&egui::ViewportId::ROOT)
                .unwrap()
                .native_pixels_per_point = Some(scale);
            let output = context.run(input, |context| {
                egui::CentralPanel::default()
                    .frame(Frame::NONE)
                    .show(context, |ui| {
                        channel(ui, "Open project", Icon::Folder, width);
                    });
            });
            assert_eq!(output.pixels_per_point, scale);
            let frames: Vec<_> = output
                .shapes
                .into_iter()
                .filter_map(|shape| match shape.shape {
                    egui::Shape::Path(path) if path.fill == Color32::from_rgb(249, 251, 252) => {
                        Some(path)
                    }
                    _ => None,
                })
                .collect();
            let [frame]: [_; 1] = frames
                .try_into()
                .expect("one path for both the fill and the border");
            frame
        };

        let idle = draw(Vec::new());
        let hovered = draw(vec![egui::Event::PointerMoved(pos2(width * 0.5, 35.0))]);
        assert!(idle.closed && hovered.closed);
        assert_eq!(
            idle.stroke,
            Stroke::new(idle.stroke.width, Color32::from_gray(180)).into()
        );
        assert_eq!(hovered.stroke, Stroke::new(idle.stroke.width, BLUE).into());
        assert_eq!(idle.points.len(), hovered.points.len());
        for (idle, hovered) in idle.points.iter().zip(&hovered.points) {
            assert!(((*idle - vec2(0.0, 2.0)) - *hovered).length() < 0.01);
        }
    }
}
