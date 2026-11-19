use super::*;

#[test]
fn studio_link_reads_a_fresh_document_path() {
    let directory =
        std::env::temp_dir().join(format!("redox-studio-link-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&directory).unwrap();
    let document_path = directory.join("live.redoxskin");
    Document::blank().save(&document_path).unwrap();
    let link_path = directory.join("live-link.json");
    std::fs::write(
        &link_path,
        serde_json::json!({"version": 1, "path": document_path}).to_string(),
    )
    .unwrap();
    assert_eq!(linked_document(&link_path), Some(document_path));
    std::fs::write(&link_path, b"unfinished").unwrap();
    assert!(linked_document(&link_path).is_none());
    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
fn unfinished_skin_keeps_the_builtin_ui_available() {
    assert!(!has_live_shell(
        &redox_studio::originals::load("codex").unwrap()
    ));
    assert!(has_live_shell(
        &redox_studio::originals::load("wii-live-workspace").unwrap()
    ));
}

#[test]
fn imported_skin_fits_the_workspace_and_preserves_control_positions() {
    use redox_studio::document::{Kind, Node};

    let ctx = egui::Context::default();
    let mut document = Document::blank();
    let mut surface = Node::new(1, Kind::Frame, 0., 0.);
    surface.width = 1100.;
    surface.height = 900.;
    let mut composer = Node::new(2, Kind::Input, 100., 700.);
    composer.width = 600.;
    composer.height = 50.;
    composer.ui_script = vec![UiAction::Composer];
    document.pages[0].nodes = vec![surface, composer];

    let mut control_rect = None;
    let _ = ctx.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(600., 400.),
            )),
            ..Default::default()
        },
        |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                interactive::show_fitted_with_values(
                    ui,
                    &document,
                    Default::default(),
                    &mut |_, control| control_rect = Some(control.response.rect),
                );
            });
        },
    );
    let rect = control_rect.expect("authored composer remains interactive");
    assert!(rect.width() > 200. && rect.width() < 300.);
    assert!(rect.max.x <= 600. && rect.max.y <= 400.);
}

#[test]
fn live_reload_applies_saved_changes_and_preserves_last_valid_skin() {
    let path = std::env::temp_dir().join(format!("redox-live-{}.redoxskin", uuid::Uuid::new_v4()));
    let mut doc = Document::blank();
    doc.save(&path).unwrap();
    let mut skin = LiveSkin::load(&path);
    doc.name = "Live change".into();
    doc.save(&path).unwrap();
    skin.refresh(&path, true);
    assert_eq!(skin.document.as_ref().unwrap().name, "Live change");
    std::fs::write(&path, b"unfinished edit").unwrap();
    skin.refresh(&path, true);
    assert_eq!(skin.document.as_ref().unwrap().name, "Live change");
    assert!(skin.warning.is_some());
    doc.name = "Recovered design".into();
    doc.save(&path).unwrap();
    skin.refresh(&path, true);
    assert_eq!(skin.document.as_ref().unwrap().name, "Recovered design");
    assert!(skin.warning.is_none());
    std::fs::remove_file(path).unwrap();
}

#[test]
fn authored_conversation_renders_active_chat_inside_its_panel_and_restores_theme() {
    use redox::domain::{Message, MessageStatus, Role};
    use redox_studio::document::{Kind, Node};
    let ctx = egui::Context::default();
    super::super::theme::apply(&ctx, "codex");
    let mut document = Document::blank();
    let mut panel = Node::new(1, Kind::Frame, 0., 0.);
    panel.width = 500.;
    panel.height = 230.;
    panel.padding = 18.;
    panel.fill = [252, 252, 252, 255];
    panel.text_color = [40, 50, 60, 255];
    panel.ui_script = vec![UiAction::Conversation];
    panel.text = "No messages yet".into();
    document.pages[0].nodes = vec![panel];
    let mut app = ReDox::new_internal(
        &eframe::CreationContext::_new_kittest(ctx.clone()),
        None,
        Default::default(),
        Some("codex"),
    );
    let base = super::super::theme::palette();
    let run = |app: &mut ReDox| {
        ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(600., 600.),
                )),
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    interactive::show_with_values(
                        ui,
                        &document,
                        Default::default(),
                        &mut |ui, control| {
                            assert_eq!(control.action, UiAction::Conversation);
                            conversation(ui, control, app);
                        },
                    );
                });
            },
        )
    };
    let empty = run(&mut app);
    assert!(empty.shapes.iter().any(
        |shape| matches!(&shape.shape, egui::Shape::Text(t) if t.galley.text() == "No messages yet")
    ));
    for (role, content) in [
        (Role::User, "My active prompt"),
        (Role::Assistant, "My active answer"),
    ] {
        app.session.active_mut().unwrap().messages.push(Message {
            id: uuid::Uuid::new_v4(),
            role,
            content: content.into(),
            status: MessageStatus::Complete,
            error: None,
            model: String::new(),
            attachments: vec![],
            activity: Default::default(),
        });
    }
    run(&mut app);
    let populated = run(&mut app);
    for label in ["My active prompt", "My active answer"] {
        let shape = populated
            .shapes
            .iter()
            .find(|shape| matches!(&shape.shape, egui::Shape::Text(t) if t.galley.text() == label))
            .expect("Active chat is visible");
        assert!(shape.clip_rect.height() < 260.);
    }
    assert!(!populated.shapes.iter().any(
        |shape| matches!(&shape.shape, egui::Shape::Text(t) if t.galley.text() == "No messages yet")
    ));
    assert!(super::super::theme::palette() == base);
}

#[test]
fn local_skin_palette_is_restored_when_rendering_unwinds() {
    let base = super::super::theme::palette();
    let mut local = base;
    local.text = egui::Color32::RED;
    let result = std::panic::catch_unwind(|| {
        super::super::theme::with_palette(local, || {
            assert_eq!(super::super::theme::text(), egui::Color32::RED);
            panic!("render interrupted");
        })
    });
    assert!(result.is_err());
    assert!(super::super::theme::palette() == base);
}

#[test]
#[cfg(feature = "design-export")]
fn saved_studio_welcome_edits_reload_on_the_native_startup_surface() {
    use redox_studio::{
        document::{Kind, Node},
        scenes::{SceneId, Source},
    };

    let context = egui::Context::default();
    let mut app = ReDox::design_preview(&context, "win98");
    app.prepare_design_scene(SceneId::Welcome);
    app.ui_preferences.follow_studio = true;
    let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1100., 900.));
    let directory =
        std::env::temp_dir().join(format!("redox-welcome-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&directory).unwrap();
    let path = directory.join("welcome.redoxskin");
    let mut document = Document::blank();
    document.source = Some(Source {
        theme: "win98".into(),
        scene: SceneId::Welcome,
    });
    let mut frame = Node::new(1, Kind::Frame, 0., 0.);
    frame.width = viewport.width();
    frame.height = viewport.height();
    let mut label = Node::new(2, Kind::Text, 50., 50.);
    label.parent = Some(1);
    label.text = "Welcome authored in Studio".into();
    let mut composer = Node::new(3, Kind::Input, 240., 700.);
    composer.parent = Some(1);
    composer.ui_script = vec![UiAction::Composer];
    let window_control = |id, action, x, width| {
        let mut node = Node::new(id, Kind::Button, x, 8.);
        node.parent = Some(1);
        node.width = width;
        node.height = 20.;
        node.fill = [0; 4];
        node.text.clear();
        node.ui_script = vec![action];
        node
    };
    document.pages[0].nodes = vec![
        frame,
        label,
        composer,
        window_control(4, UiAction::WindowDrag, 50., 400.),
        window_control(5, UiAction::WindowMinimize, 980., 25.),
        window_control(6, UiAction::WindowMaximize, 1010., 25.),
        window_control(7, UiAction::WindowClose, 1040., 25.),
    ];
    document.save(&path).unwrap();
    let live_link = directory.join("live-link.json");
    std::fs::write(
        &live_link,
        serde_json::json!({"version": 1, "path": path}).to_string(),
    )
    .unwrap();
    app.sync_studio_link_from(&context, &live_link);
    assert_eq!(app.ui_preferences.studio_skin.as_ref(), Some(&path));

    let render = |app: &mut ReDox, viewport, events| {
        context.run(
            egui::RawInput {
                screen_rect: Some(viewport),
                events,
                ..Default::default()
            },
            |context| app.render_design(context),
        )
    };
    fn has_label(output: &egui::FullOutput, text: &str) -> bool {
        fn contains(shape: &egui::Shape, text: &str) -> bool {
            match shape {
                egui::Shape::Text(shape) => shape.galley.text() == text,
                egui::Shape::Vec(shapes) => shapes.iter().any(|shape| contains(shape, text)),
                _ => false,
            }
        }
        output
            .shapes
            .iter()
            .any(|shape| contains(&shape.shape, text))
    }

    let first = render(&mut app, viewport, vec![]);
    document.pages[0].nodes[1].text = "New appearance from Studio".into();
    document.save(&path).unwrap();
    let cache_key = egui::Id::new(("studio_document", &path));
    context.data_mut(|data| {
        let mut cache = data.get_temp::<LiveSkin>(cache_key).unwrap();
        cache.checked -= std::time::Duration::from_secs(1);
        data.insert_temp(cache_key, cache);
    });
    let second = render(&mut app, viewport, vec![]);
    let button = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::default(),
    };
    for (x, command) in [
        (990., egui::ViewportCommand::Minimized(true)),
        (1020., egui::ViewportCommand::Maximized(true)),
        (1050., egui::ViewportCommand::Close),
    ] {
        let pos = egui::pos2(x, 18.);
        let _ = render(
            &mut app,
            viewport,
            vec![egui::Event::PointerMoved(pos), button(pos, true)],
        );
        let click = render(&mut app, viewport, vec![button(pos, false)]);
        assert!(
            click.viewport_output[&egui::ViewportId::ROOT]
                .commands
                .contains(&command)
        );
    }
    let pos = egui::pos2(120., 18.);
    let press = render(
        &mut app,
        viewport,
        vec![egui::Event::PointerMoved(pos), button(pos, true)],
    );
    let drag = render(
        &mut app,
        viewport,
        vec![egui::Event::PointerMoved(egui::pos2(260., 18.))],
    );
    assert!(
        press.viewport_output[&egui::ViewportId::ROOT]
            .commands
            .iter()
            .chain(&drag.viewport_output[&egui::ViewportId::ROOT].commands)
            .any(|command| matches!(command, egui::ViewportCommand::StartDrag))
    );
    let resized_viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000., 700.));
    let resized = render(
        &mut app,
        resized_viewport,
        vec![button(egui::pos2(260., 18.), false)],
    );
    let zoom = resized_viewport.height() / viewport.height();
    let inset = (resized_viewport.width() - viewport.width() * zoom) / 2.;
    let resized_pos = |x, y| egui::pos2(inset + x * zoom, y * zoom);
    let click_resized = |app: &mut ReDox, pos| {
        let _ = render(
            app,
            resized_viewport,
            vec![egui::Event::PointerMoved(pos), button(pos, true)],
        );
        render(app, resized_viewport, vec![button(pos, false)])
    };
    let window_clip = click_resized(&mut app, resized_pos(990., 18.));
    assert!(
        window_clip.viewport_output[&egui::ViewportId::ROOT]
            .commands
            .contains(&egui::ViewportCommand::Minimized(true))
    );
    let _ = click_resized(&mut app, resized_pos(290., 720.));
    let _ = render(
        &mut app,
        resized_viewport,
        vec![egui::Event::Text("Draft typed after resize".into())],
    );
    assert_eq!(
        app.session.active().unwrap().draft,
        "Draft typed after resize"
    );
    app.sidebar_visible = true;
    let with_sidebar = render(&mut app, viewport, vec![]);
    std::fs::remove_file(&path).unwrap();
    std::fs::remove_file(&live_link).unwrap();
    context.data_mut(|data| {
        let mut link = data
            .get_temp::<StudioLink>(egui::Id::new("studio_live_link"))
            .unwrap();
        link.checked -= std::time::Duration::from_secs(1);
        data.insert_temp(egui::Id::new("studio_live_link"), link);
    });
    app.sync_studio_link_from(&context, &live_link);
    std::fs::remove_dir(directory).unwrap();

    assert!(has_label(&first, "Welcome authored in Studio"));
    assert!(has_label(&second, "New appearance from Studio"));
    assert!(!has_label(&second, "Welcome authored in Studio"));
    assert!(has_label(&resized, "New appearance from Studio"));
    assert!(!has_label(&with_sidebar, "New appearance from Studio"));
    assert!(app.ui_preferences.studio_skin.is_none());
    assert!(app.sidebar_visible);
    assert!(!has_label(
        &second,
        "Studio skin preview — add a conversation and composer binding to activate"
    ));
}
