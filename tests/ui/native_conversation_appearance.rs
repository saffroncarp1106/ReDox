use super::*;
use crate::app::ReDox;
use redox_studio::scenes::SceneId;

#[test]
fn saved_native_transcript_surface_updates_paint_without_replacing_the_conversation() {
    fn contains(shapes: &[egui::Shape], predicate: &impl Fn(&egui::Shape) -> bool) -> bool {
        shapes.iter().any(|shape| {
            predicate(shape)
                || matches!(shape, egui::Shape::Vec(children) if contains(children, predicate))
        })
    }

    let path = std::env::temp_dir().join(format!("redox-appearance-{}.json", uuid::Uuid::new_v4()));
    let color = [31, 74, 105];
    let mut appearance = NativeAppearance::default();
    appearance.transcript_surface = Some(color);
    appearance.save(&path).unwrap();

    let context = egui::Context::default();
    let mut app = ReDox::design_preview(&context, "codex");
    app.prepare_design_scene(SceneId::Conversation);
    let install = |cached: Option<[u8; 3]>| {
        context.data_mut(|data| {
            data.insert_temp(
                cache_id(),
                AppearanceCache {
                    path: path.clone(),
                    checked: None,
                    appearance: {
                        let mut appearance = NativeAppearance::default();
                        appearance.transcript_surface = cached;
                        appearance
                    },
                },
            );
        });
    };
    let mut frame = || {
        (0..3)
            .map(|_| {
                let input = egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1100.0, 900.0),
                    )),
                    ..Default::default()
                };
                context
                    .run(input, |context| app.render_design(context))
                    .shapes
                    .into_iter()
                    .map(|entry| entry.shape)
                    .collect::<Vec<_>>()
            })
            .last()
            .unwrap()
    };
    let fill = |shape: &egui::Shape| matches!(shape, egui::Shape::Rect(rect) if rect.fill == Color32::from_rgb(color[0], color[1], color[2]));
    let message = |shape: &egui::Shape| matches!(shape, egui::Shape::Text(text) if text.galley.job.text.contains("Dummy text."));
    install(None);
    let updated = frame();
    assert!(contains(&updated, &fill));
    assert!(contains(&updated, &message));

    NativeAppearance::default().save(&path).unwrap();
    install(Some(color));
    let reset = frame();
    assert!(!contains(&reset, &fill));
    assert!(contains(&reset, &message));
    std::fs::remove_file(path).unwrap();
}

#[test]
fn saved_settings_navigation_changes_native_paint_and_preserves_controls() {
    let temp = std::env::temp_dir().join(format!("redox-settings-{}.json", uuid::Uuid::new_v4()));
    let mut appearance = NativeAppearance::default();
    let [r, g, b] = [39, 75, 101];
    appearance.settings_navigation = Some([r, g, b]);
    appearance.save(&temp).unwrap();
    let context = egui::Context::default();
    context.data_mut(|data| {
        data.insert_temp(
            cache_id(),
            AppearanceCache {
                path: temp.clone(),
                checked: None,
                appearance: NativeAppearance::default(),
            },
        );
    });
    let mut app = ReDox::design_preview(&context, "codex");
    app.prepare_design_scene(SceneId::Account);
    let output = (0..7)
        .map(|_| {
            context.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1100., 900.),
                    )),
                    ..Default::default()
                },
                |context| app.render_design(context),
            )
        })
        .last()
        .unwrap();

    assert!(output.shapes.iter().any(|shape| {
        matches!(&shape.shape, egui::Shape::Rect(rect) if rect.fill.to_srgba_unmultiplied()[..3] == [r,g,b])
    }));
    assert!(output.shapes.iter().any(|shape| {
        matches!(&shape.shape, egui::Shape::Text(text) if text.galley.job.text.contains("Workspace & access"))
    }));
    std::fs::remove_file(temp).unwrap();
}
