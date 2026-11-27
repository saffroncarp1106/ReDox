use super::*;
use redox::activity::TurnActivity;

#[test]
fn theme_commands_switch_live_without_leaving_fullscreen() {
    let context = egui::Context::default();
    let mut app = terminal_app(&context);
    set_open(&mut app, &context, true);
    set_fullscreen(&context, true);
    let chat = app.session.active().unwrap().id;
    app.session.active_mut().unwrap().draft = "Keep the draft".into();
    assert_eq!(
        commands::suggestions(&app, "/theme d")[0].input,
        "/theme dark"
    );
    let _ = context.run(egui::RawInput::default(), |_| {});

    for (command, theme_id) in [
        ("/dark", win98::DARK_ID),
        ("/light mode", win98::ID),
        ("/theme dark", win98::DARK_ID),
        ("/theme light", win98::ID),
    ] {
        let frame = context.run(egui::RawInput::default(), |context| {
            commands::run(&mut app, context, command);
        });
        assert_eq!(app.active_ui.id, theme_id);
        assert_eq!(app.ui_preferences.ui, theme_id);
        assert_eq!(win98::high_contrast(), theme_id == win98::DARK_ID);
        assert!(is_fullscreen(&context));
        assert!(frame.viewport_output[&egui::ViewportId::ROOT].commands.is_empty());
    }
    assert_eq!(app.session.active().unwrap().id, chat);
    assert_eq!(app.session.active().unwrap().draft, "Keep the draft");
    commands::run(&mut app, &context, "/theme");
    assert!(
        commands::output(&context, chat)
            .unwrap()
            .contains("Current theme: Windows 98")
    );
    commands::run(&mut app, &context, "/theme purple");
    assert!(
        commands::output(&context, chat)
            .unwrap()
            .contains("Usage: /theme")
    );
    assert_eq!(app.active_ui.id, win98::ID);
}

#[cfg(windows)]
#[test]
fn fullscreen_uses_the_native_cursor_without_hiding_it() {
    let context = egui::Context::default();
    let mut app = terminal_app(&context);
    set_open(&mut app, &context, true);
    let screen = Rect::from_min_size(egui::Pos2::ZERO, vec2(1100.0, 800.0));
    let frame = |app: &mut ReDox| {
        context.run(
            egui::RawInput {
                screen_rect: Some(screen),
                events: vec![egui::Event::PointerMoved(screen.center())],
                ..Default::default()
            },
            |context| app.draw_ui(context),
        )
    };
    frame(&mut app);
    assert_eq!(
        frame(&mut app).platform_output.cursor_icon,
        egui::CursorIcon::None
    );
    set_fullscreen(&context, true);
    frame(&mut app);
    assert_ne!(
        frame(&mut app).platform_output.cursor_icon,
        egui::CursorIcon::None
    );
}

#[test]
fn crt_scanline_meshes_are_reused_until_geometry_or_settings_change() {
    let context = egui::Context::default();
    let frame = |size, filter: crt::Settings| {
        context.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, size)),
                ..Default::default()
            },
            |context| {
                egui::CentralPanel::default().show(context, |ui| {
                    filter.paint_background(ui, ui.max_rect());
                });
            },
        )
    };
    let meshes = |output: egui::FullOutput| {
        output
            .shapes
            .into_iter()
            .filter_map(|entry| match entry.shape {
                egui::Shape::Mesh(mesh) if mesh.vertices.len() > 5_000 => Some(mesh),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    let size = vec2(1920.0, 1080.0);
    let settings = crt::Settings::default();
    let cold = meshes(frame(size, settings));
    let warm = meshes(frame(size, settings));
    assert_eq!(cold.len(), 2);
    assert_eq!(warm.len(), cold.len());
    assert!(
        cold.iter()
            .zip(&warm)
            .all(|(old, new)| std::sync::Arc::ptr_eq(old, new))
    );

    let resized = meshes(frame(vec2(2560.0, 1440.0), settings));
    assert_eq!(resized.len(), 2);
    assert!(resized[0].vertices.len() > warm[0].vertices.len());
    let updated = meshes(frame(
        vec2(2560.0, 1440.0),
        crt::Settings {
            scanline_strength: 100,
            ..settings
        },
    ));
    assert!(
        updated[0]
            .vertices
            .iter()
            .all(|vertex| vertex.color == BLACK)
    );
}

#[test]
#[ignore = "offscreen CPU benchmark for CRT scanline caching"]
fn crt_fullscreen_frame_benchmark() {
    let context = egui::Context::default();
    let screen = Rect::from_min_size(egui::Pos2::ZERO, vec2(3840.0, 2160.0));
    let render = |brightness| {
        let output = context.run(
            egui::RawInput {
                screen_rect: Some(screen),
                ..Default::default()
            },
            |context| {
                egui::CentralPanel::default().show(context, |ui| {
                    crt::Settings {
                        scanline_brightness: brightness,
                        ..Default::default()
                    }
                    .paint_background(ui, ui.max_rect());
                });
            },
        );
        std::hint::black_box(context.tessellate(output.shapes, output.pixels_per_point));
    };
    let measure = |invalidate: bool| {
        render(7);
        let start = std::time::Instant::now();
        for frame in 0..120 {
            render(7 + u8::from(invalidate && frame % 2 == 0));
        }
        start.elapsed().as_secs_f64() * 1000.0 / 120.0
    };
    let rebuilt = measure(true);
    let cached = measure(false);
    println!("4K CPU frame: {rebuilt:.3} ms rebuilt, {cached:.3} ms cached");
}

#[test]
fn top_row_icon_toggles_are_independent_and_persist() {
    let context = egui::Context::default();
    let mut app = terminal_app(&context);
    app.sidebar_visible = false;
    let screen = Rect::from_min_size(egui::Pos2::ZERO, vec2(1100.0, 800.0));
    let frame = |app: &mut ReDox, events| {
        context.run(
            egui::RawInput {
                screen_rect: Some(screen),
                events,
                ..Default::default()
            },
            |context| app.draw_ui(context),
        )
    };
    let locate = |output: &egui::FullOutput, label: &str| {
        output.shapes.iter().find_map(|entry| match &entry.shape {
            egui::Shape::Text(text) if text.galley.text() == label => {
                Some(text.visual_bounding_rect().center())
            }
            _ => None,
        })
    };
    let has_icon = |output: &egui::FullOutput| {
        locate(output, "My Chats").is_some_and(|point| point.x < 100.0)
    };

    set_open(&mut app, &context, false);
    let chat = frame(&mut app, vec![]);
    assert!(has_icon(&chat));
    frame(&mut app, click(locate(&chat, "Hide icons").unwrap()));
    assert!(!has_icon(&frame(&mut app, vec![])));

    set_open(&mut app, &context, true);
    let terminal = frame(&mut app, vec![]);
    assert!(!has_icon(&terminal));
    frame(&mut app, click(locate(&terminal, "Show icons").unwrap()));
    assert!(has_icon(&frame(&mut app, vec![])));
    toggle_docked(&mut app, &context);
    assert!(has_icon(&frame(&mut app, vec![])));

    set_fullscreen(&context, true);
    let fullscreen = frame(&mut app, vec![]);
    assert!(has_icon(&fullscreen));
    frame(&mut app, click(locate(&fullscreen, "Hide icons").unwrap()));
    let hidden = frame(&mut app, vec![]);
    assert!(!has_icon(&hidden));
    assert!(is_fullscreen(&context));
    frame(&mut app, click(locate(&hidden, "Show icons").unwrap()));
    let visible = frame(&mut app, vec![]);
    assert!(has_icon(&visible));
    let start = locate(&visible, "       Start").unwrap();
    frame(&mut app, click(start));
    assert!(locate(&frame(&mut app, vec![]), "Usage limits").is_some());

    set_open(&mut app, &context, false);
    assert!(!has_icon(&frame(&mut app, vec![])));
    let saved = serde_json::to_string(&app.ui_preferences).unwrap();
    let restored: crate::app::ui::Preferences = serde_json::from_str(&saved).unwrap();
    assert!(!restored.win98_icons_visible(false));
    assert!(restored.win98_icons_visible(true));
}

#[test]
fn fresh_chat_keeps_access_visible_and_attaches_a_folder_without_losing_the_draft() {
    let context = egui::Context::default();
    let mut app = terminal_app(&context);
    app.new_chat();
    let chat = app.session.active_mut().unwrap();
    let id = chat.id;
    chat.draft = "Keep this draft".into();
    assert!(chat.workspace.is_none());
    let screen = Rect::from_min_size(egui::Pos2::ZERO, vec2(900.0, 600.0));
    for terminal in [false, true] {
        set_open(&mut app, &context, terminal);
        let output = context.run(
            egui::RawInput {
                screen_rect: Some(screen),
                ..Default::default()
            },
            |context| app.draw_ui(context),
        );
        assert!(
            output.shapes.iter().any(|entry| {
                matches!(&entry.shape, egui::Shape::Text(text)
                    if text.galley.text().starts_with("Access")
                        && entry.clip_rect.contains(text.visual_bounding_rect().center()))
            }),
            "Access control is missing with Terminal open: {terminal}"
        );
    }
    let folder = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    app.sender
        .send(crate::app::Event::Workspace(Some(id), Ok(Some(folder.clone()))))
        .unwrap();
    app.receive_events(&context);
    let chat = app.session.active().unwrap();
    assert_eq!(chat.id, id);
    assert_eq!(chat.workspace.as_ref(), Some(&folder));
    assert!(chat.mode == redox::domain::Mode::Code);
    assert_eq!(chat.draft, "Keep this draft");
}

#[test]
fn hidden_terminal_caption_keeps_window_controls_visible_after_resize() {
    let context = egui::Context::default();
    let mut app = terminal_app(&context);
    set_open(&mut app, &context, true);
    app.ui_preferences.win98_terminal_docked = false;
    app.ui_preferences.win98_terminal_appearance.caption = settings::Caption::Hidden;
    for size in [
        vec2(1400.0, 900.0),
        vec2(540.0, 420.0),
        vec2(1100.0, 750.0),
    ] {
        let screen = Rect::from_min_size(egui::Pos2::ZERO, size);
        let output = context.run(
            egui::RawInput {
                screen_rect: Some(screen),
                ..Default::default()
            },
            |context| app.draw_ui(context),
        );
        assert!(output.shapes.iter().any(|entry| {
            matches!(&entry.shape, egui::Shape::Text(text)
                if text.galley.text() == "File"
                    && screen.contains(text.visual_bounding_rect().center())
                    && entry.clip_rect.contains(text.visual_bounding_rect().center()))
        }));
    }
}

#[test]
fn renders_each_changed_segment_with_matching_markers_and_counts() {
    let mut activity = TurnActivity::default();
    activity.replace_diff(concat!(
        "diff --git a/src/main.rs b/src/main.rs\n",
        "--- a/src/main.rs\n",
        "+++ b/src/main.rs\n",
        "@@ -1,4 +1,5 @@\n",
        "-    old();\n",
        "+    first();\n",
        "+    second();\n",
        "     keep();\n",
        "---count;\n",
        "+++count;\n",
        " unchanged\n",
        "@@ -9,1 +10,0 @@\n",
        "-last();\n",
        "\\ No newline at end of file\n",
    ));
    let mut rows = Vec::new();
    append_sections(&mut rows, &activity.files[0]);
    assert_eq!(
        rows.iter().map(TranscriptRow::text).collect::<Vec<_>>(),
        [
            "----- remove -----",
            "",
            "    old();",
            "",
            "1 line",
            "----- remove -----",
            "",
            "----- add -----",
            "",
            "    first();",
            "    second();",
            "",
            "2 lines",
            "----- add -----",
            "",
            "----- remove -----",
            "",
            "--count;",
            "",
            "1 line",
            "----- remove -----",
            "",
            "----- add -----",
            "",
            "++count;",
            "",
            "1 line",
            "----- add -----",
            "",
            "----- remove -----",
            "",
            "last();",
            "",
            "1 line",
            "----- remove -----",
            "",
        ]
    );
}

#[test]
fn terminal_close_requires_confirmation_and_cancel_preserves_the_draft() {
    let context = egui::Context::default();
    let mut app = terminal_app(&context);
    assert!(!app.ui_preferences.win98_terminal_open);
    assert!(
        terminal_rows(app.session.active(), plain_lines)
            .0
            .is_empty()
    );
    set_open(&mut app, &context, true);
    assert!(app.ui_preferences.win98_terminal_open);
    let rect = Rect::from_min_size(egui::pos2(20.0, 30.0), vec2(400.0, 300.0));
    let close = egui::pos2(rect.right() - 14.0, rect.top() + 13.0);
    app.session.active_mut().unwrap().draft = "Keep my draft".into();
    let frame = |app: &mut ReDox, events| {
        context.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(800.0, 600.0))),
                events,
                ..Default::default()
            },
            |context| {
                egui::CentralPanel::default().show(context, |ui| show(ui, rect, app));
            },
        )
    };
    let closed = |result: &egui::FullOutput| {
        result.viewport_output[&egui::ViewportId::ROOT]
            .commands
            .contains(&egui::ViewportCommand::Close)
    };
    let button = |result: &egui::FullOutput, label| {
        result
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text() == label => {
                    Some(text.visual_bounding_rect().center())
                }
                _ => None,
            })
            .expect("confirmation button")
    };
    frame(&mut app, vec![]);
    assert!(!closed(&frame(&mut app, click(close))));
    let dialog = frame(&mut app, vec![]);
    assert!(!closed(&frame(&mut app, click(button(&dialog, "Cancel")))));
    assert!(app.ui_preferences.win98_terminal_open);
    assert_eq!(app.session.active().unwrap().draft, "Keep my draft");

    frame(&mut app, vec![]);
    assert!(!closed(&frame(&mut app, click(close))));
    frame(&mut app, vec![]);
    assert!(!closed(&frame(&mut app, vec![escape_pressed()])));
    assert!(!context.data(|data| {
        data.get_temp(egui::Id::new(CLOSE_CONFIRMATION))
            .unwrap_or(false)
    }));

    frame(&mut app, vec![]);
    assert!(!closed(&frame(&mut app, click(close))));
    let dialog = frame(&mut app, vec![]);
    assert!(closed(&frame(
        &mut app,
        click(button(&dialog, "Close ReDox"))
    )));
    assert!(app.ui_preferences.win98_terminal_open);
}

#[test]
fn terminal_caption_minimizes_without_changing_size_or_layout() {
    for (docked, fullscreen) in [(false, false), (true, false), (false, true), (true, true)] {
        let context = egui::Context::default();
        let mut app = terminal_app(&context);
        set_open(&mut app, &context, true);
        app.ui_preferences.win98_terminal_docked = docked;
        if fullscreen {
            set_fullscreen(&context, true);
        }
        let rect = Rect::from_min_size(egui::pos2(20.0, 30.0), vec2(400.0, 300.0));
        let control = egui::pos2(rect.right() - 54.0, rect.top() + 13.0);
        let mut render = |context: &egui::Context| {
            egui::CentralPanel::default().show(context, |ui| show(ui, rect, &mut app));
        };
        let _ = context.run(egui::RawInput::default(), &mut render);
        let result = context.run(
            egui::RawInput {
                events: click(control),
                ..Default::default()
            },
            render,
        );
        assert_eq!(
            result.viewport_output[&egui::ViewportId::ROOT].commands,
            [egui::ViewportCommand::Minimized(true)]
        );
        assert_eq!(app.ui_preferences.win98_terminal_zoom, 100);
        assert!(app.ui_preferences.win98_terminal_open);
        assert_eq!(app.ui_preferences.win98_terminal_docked, docked);
        assert_eq!(is_fullscreen(&context), fullscreen);
    }
}

#[test]
fn terminal_attachment_tray_keeps_the_transcript_in_place() {
    let context = egui::Context::default();
    let mut app = terminal_app(&context);
    set_open(&mut app, &context, true);
    let chat = app.session.active_mut().unwrap();
    chat.set_provider(redox::domain::Provider::Ollama, "local-model".into());
    let reply = uuid::Uuid::new_v4();
    chat.messages.push(redox::domain::Message {
        id: reply,
        role: Role::Assistant,
        content: format!("{}Latest output", "Earlier output\n".repeat(100)),
        status: MessageStatus::Complete,
        error: None,
        model: "Ollama · local-model".into(),
        attachments: vec![],
        activity: Default::default(),
    });
    let frame = |app: &mut ReDox, time| {
        context.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(500.0, 300.0))),
                time: Some(time),
                ..Default::default()
            },
            |context| {
                app.receive_events(context);
                egui::CentralPanel::default().show(context, |ui| {
                    show(ui, ui.available_rect_before_wrap(), app);
                });
            },
        )
    };
    let locate = |frame: &egui::FullOutput, label: &str| {
        frame.shapes.iter().find_map(|entry| match &entry.shape {
            egui::Shape::Text(text) if text.galley.text() == label => {
                let center = text.visual_bounding_rect().center();
                entry.clip_rect.contains(center).then_some(center)
            }
            _ => None,
        })
    };
    frame(&mut app, 0.0);
    let empty = frame(&mut app, 0.1);
    assert!(locate(&empty, "No attachments").is_none());
    assert!(locate(&empty, "Jump to latest").is_none());
    let zero = locate(&empty, "Context usage unavailable").unwrap();
    let latest = locate(&empty, "Latest output").unwrap();
    let prompt = locate(&empty, ">").unwrap();
    app.session.active_mut().unwrap().ollama_usage.context = Some((
        reply,
        ContextUsage {
            tokens: 16_000,
            limit: 64_000,
            compressions: 9,
            estimated: false,
            load_seconds: None,
            prompt_seconds: None,
            cached_tokens: None,
        },
    ));
    let font = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/ui/assets/fonts/safont/SafFont-Clean.ttf");
    let result = crate::app::read_attachments(vec![font]);
    assert!(result.is_err());
    let chat_id = app.session.active_chat.unwrap();
    app.sender
        .send(crate::app::Event::Attachments(chat_id, result))
        .unwrap();
    let rejected = frame(&mut app, 0.15);
    assert!(app.attachment_error.is_none());
    assert!(app.session.active().unwrap().attachments.is_empty());
    assert_eq!(locate(&rejected, "Latest output"), Some(latest));
    assert_eq!(locate(&rejected, ">"), Some(prompt));
    for index in 0..redox::attachments::MAX_ATTACHMENTS {
        app.session
            .active_mut()
            .unwrap()
            .attachments
            .push(Attachment {
                id: uuid::Uuid::new_v4(),
                path: format!("example-{index}.rs").into(),
                text: Some("fn example() {}".into()),
            });
    }
    let attached = frame(&mut app, 0.2);
    let first_chip = locate(&attached, "example-0.rs").unwrap();
    let meter = locate(&attached, "75% context left · 9 compressions [||       ]").unwrap();
    assert_eq!(meter.y, zero.y);
    assert!(latest.y < meter.y && meter.y < first_chip.y && first_chip.y < prompt.y);
    for index in 1..redox::attachments::MAX_ATTACHMENTS {
        let chip = locate(&attached, &format!("example-{index}.rs")).unwrap();
        assert!(chip.x > first_chip.x);
        assert_eq!(chip.y, first_chip.y);
    }
    assert_eq!(locate(&attached, "Latest output"), Some(latest));
    assert_eq!(locate(&attached, ">"), Some(prompt));

    let id = app.session.active_chat.unwrap();
    app.sender
        .send(crate::app::Event::Attachments(
            id,
            Ok(vec![Attachment {
                id: uuid::Uuid::new_v4(),
                path: "ninth.rs".into(),
                text: Some(String::new()),
            }]),
        ))
        .unwrap();
    let warning = frame(&mut app, 1.0);
    let message = attachment_limit_message();
    let status = locate(&warning, message.as_str()).unwrap();
    assert!(first_chip.y < status.y && status.y < prompt.y);
    assert!(app.attachment_error.is_none());
    assert_eq!(
        app.session.active().unwrap().attachments.len(),
        redox::attachments::MAX_ATTACHMENTS,
    );
    let warning_alpha = |frame: &egui::FullOutput| {
        frame.shapes.iter().find_map(|entry| match &entry.shape {
            egui::Shape::Text(text) if text.galley.text() == message => {
                Some(text.fallback_color.a())
            }
            _ => None,
        })
    };
    assert_eq!(warning_alpha(&warning), Some(255));
    let fading = frame(&mut app, 5.8);
    assert!((1..=254).contains(&warning_alpha(&fading).unwrap()));
    let expired = frame(&mut app, 6.0);
    assert_eq!(warning_alpha(&expired), None);
    for output in [&warning, &fading, &expired] {
        assert_eq!(locate(output, "Latest output"), Some(latest));
        assert_eq!(locate(output, ">"), Some(prompt));
    }
    app.session.active_mut().unwrap().attachments.clear();
    let cleared = frame(&mut app, 6.1);
    assert!(locate(&cleared, "No attachments").is_none());
    assert_eq!(locate(&cleared, "Latest output"), Some(latest));
}

#[test]
fn scales_the_bundled_font_without_resizing_the_window() {
    let context = egui::Context::default();
    let mut app = terminal_app(&context);
    set_open(&mut app, &context, true);
    let available = Rect::from_min_size(egui::pos2(20.0, 30.0), vec2(550.0, 360.0));
    let frame = |app: &mut ReDox, events| {
        context.run(
            egui::RawInput {
                events,
                ..Default::default()
            },
            |context| {
                egui::CentralPanel::default().show(context, |ui| show(ui, available, app));
            },
        )
    };
    let prompt_size = |frame: &egui::FullOutput| {
        frame
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text() == ">" => Some(text.galley.size()),
                _ => None,
            })
            .unwrap()
    };
    let before = frame(&mut app, vec![]);
    context.fonts_mut(|fonts| {
        let font = terminal_font(FONT_SIZE);
        assert_eq!(fonts.definitions().families[&font.family][0], FONT_FILE);
        assert_eq!(fonts.glyph_width(&font, 'M'), 8.0);
    });
    let zoom_in = egui::pos2(available.left() + 136.0, available.top() + 13.0);
    let _ = frame(&mut app, click(zoom_in));
    assert_eq!(app.ui_preferences.win98_terminal_zoom, 125);
    assert_eq!(context.zoom_factor(), 1.0);
    let after = frame(&mut app, vec![]);
    assert!(prompt_size(&after).y > prompt_size(&before).y);
    for frame in [before, after] {
        assert!(frame.shapes.iter().any(|shape| {
            matches!(&shape.shape, egui::Shape::Rect(panel) if panel.rect == available && panel.fill == theme::sidebar())
        }));
    }
}

#[test]
fn ainsley_replies_use_safont_for_prose_and_ibm_for_code() {
    let context = egui::Context::default();
    let mut app = terminal_app(&context);
    let chat = app.session.active_mut().unwrap();
    chat.ollama_model = "qwen3.6:35b".into();
    for (model, text) in [
        (
            "Ollama · Aisnley-Ox-1.2-bbb:35b",
            "Run `cargo test` now.\n\n```rust\nfn main() {}\n```",
        ),
        ("Ollama · qwen3.6:35b", "Qwen reply."),
    ] {
        chat.messages.push(redox::domain::Message {
            id: uuid::Uuid::new_v4(),
            role: Role::Assistant,
            content: text.into(),
            status: MessageStatus::Complete,
            error: None,
            model: model.into(),
            attachments: vec![],
            activity: Default::default(),
        });
    }
    let output = context.run(egui::RawInput::default(), |context| {
        egui::CentralPanel::default().show(context, |ui| {
            show(ui, ui.available_rect_before_wrap(), &mut app);
        });
    });
    let formats = |label: &str| {
        let galley = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text() == label => Some(&text.galley),
                _ => None,
            })
            .unwrap();
        galley
            .job
            .sections
            .iter()
            .map(|section| {
                (
                    &galley.job.text[section.byte_range.clone()],
                    section.format.font_id.family.to_string(),
                )
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(
        formats("Run `cargo test` now."),
        [
            ("Run ", SAFONT_FAMILY.into()),
            ("`cargo test`", FONT_FAMILY.into()),
            (" now.", SAFONT_FAMILY.into()),
        ]
    );
    assert_eq!(
        formats("fn main() {}"),
        [("fn main() {}", FONT_FAMILY.into())]
    );
    assert_eq!(
        formats("Qwen reply."),
        [("Qwen reply.", FONT_FAMILY.into())]
    );
    context.fonts(|fonts| {
        let safont = egui::FontFamily::Name(SAFONT_FAMILY.into());
        assert_eq!(fonts.definitions().families[&safont][0], SAFONT_FILE);
    });
}

#[test]
fn wrapping_keeps_inline_and_streaming_code_in_ibm() {
    let context = egui::Context::default();
    let _app = terminal_app(&context);
    let source = "Try café with `println!(\"λ\")`.\n\n~~~rust\nfn main() {";
    let _ = context.run(egui::RawInput::default(), |context| {
        egui::CentralPanel::default().show(context, |ui| {
            let ibm = terminal_font(FONT_SIZE);
            let safont = FontId::new(SAFONT_SIZE, egui::FontFamily::Name(SAFONT_FAMILY.into()));
            let rows = wrap_ainsley(ui, ainsley_layout(source, &ibm, &safont, ROW_HEIGHT, 35.0));
            let mut code = String::new();
            let mut prose = String::new();
            for row in rows {
                let TranscriptRow::Mixed(job) = row else {
                    panic!("expected styled row")
                };
                for section in job.sections {
                    let text = &job.text[section.byte_range];
                    if section.format.font_id == ibm {
                        code.push_str(text);
                    } else {
                        prose.push_str(text);
                    }
                }
            }
            assert_eq!(code, "`println!(\"λ\")`~~~rustfn main() {");
            assert_eq!(prose, "Try café with .");
        });
    });
}

#[test]
fn terminal_opens_at_window_size_and_toggles_sidebars_from_the_prompt() {
    let context = egui::Context::default();
    let mut app = terminal_app(&context);
    app.sidebar_visible = false;
    let screen = Rect::from_min_size(egui::Pos2::ZERO, vec2(900.0, 600.0));
    let frame = |app: &mut ReDox, events| {
        context.run(
            egui::RawInput {
                screen_rect: Some(screen),
                events,
                ..Default::default()
            },
            |context| app.draw_ui(context),
        )
    };
    let shows_text = |frame: &egui::FullOutput, label| {
        frame.shapes.iter().any(
            |shape| matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text() == label),
        )
    };
    let fills_window = |frame: &egui::FullOutput| {
        frame.shapes.iter().any(|shape| {
            matches!(&shape.shape, egui::Shape::Rect(panel) if panel.rect == screen && panel.fill == theme::sidebar())
        })
    };

    let chat = frame(&mut app, vec![]);
    let terminal_button = chat
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Text(text) if text.galley.text() == "Terminal" && text.pos.y > 550.0 => {
                Some(text.pos + text.galley.size() / 2.0)
            }
            _ => None,
        })
        .expect("Terminal taskbar button");
    frame(&mut app, click(terminal_button));
    assert!(app.ui_preferences.win98_terminal_open);
    let terminal = frame(&mut app, vec![]);
    assert!(shows_text(&terminal, "ReDox Terminal"));
    assert!(!shows_text(&terminal, "Chat"));
    assert!(fills_window(&terminal));
    assert!(!is_fullscreen(&context));
    let input = app.prompt_input_id.unwrap();
    assert!(context.memory(|memory| memory.has_focus(input)));

    let sidebar_command = || {
        vec![
            egui::Event::Text("/sidebar".into()),
            egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            },
        ]
    };
    frame(&mut app, sidebar_command());
    let with_sidebar = frame(&mut app, vec![]);
    assert!(shows_text(&with_sidebar, "ReDox Terminal"));
    assert!(shows_text(&with_sidebar, "Projects"));
    assert!(!fills_window(&with_sidebar));
    assert!(!app.sidebar_visible);

    frame(&mut app, sidebar_command());
    let expanded = frame(&mut app, vec![]);
    assert!(fills_window(&expanded));

    let button_center = |frame: &egui::FullOutput, label| {
        frame
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text() == label => {
                    Some(text.visual_bounding_rect().center())
                }
                _ => None,
            })
            .expect("Terminal layout button")
    };
    app.session.active_mut().unwrap().draft = "Draft stays here".into();
    let dock = button_center(&expanded, "Dock");
    assert!(dock.x < button_center(&expanded, "ReDox Terminal").x);
    assert!(button_center(&expanded, "100%").x < button_center(&expanded, "ReDox Terminal").x);
    frame(&mut app, click(dock));
    assert!(app.ui_preferences.win98_terminal_docked);
    let docked = frame(&mut app, vec![]);
    assert!(shows_text(&docked, "ReDox Terminal"));
    assert!(shows_text(&docked, "Chat"));
    assert!(!fills_window(&docked));
    assert_eq!(app.session.active().unwrap().draft, "Draft stays here");

    let minimize = egui::pos2(screen.right() - 49.0, screen.top() + 13.0);
    let minimized = frame(&mut app, click(minimize));
    assert!(
        minimized.viewport_output[&egui::ViewportId::ROOT]
            .commands
            .contains(&egui::ViewportCommand::Minimized(true))
    );
    assert!(app.ui_preferences.win98_terminal_docked);

    commands::run(&mut app, &context, "/fullscreen");
    assert!(shows_text(&frame(&mut app, vec![]), "       Start"));
    frame(&mut app, vec![escape_pressed()]);
    let docked = frame(&mut app, vec![]);
    assert!(shows_text(&docked, "Undock"));
    assert!(!fills_window(&docked));

    frame(&mut app, click(button_center(&docked, "Undock")));
    assert!(!app.ui_preferences.win98_terminal_docked);
    assert!(fills_window(&frame(&mut app, vec![])));
    assert_eq!(app.session.active().unwrap().draft, "Draft stays here");
    commands::run(&mut app, &context, "/chat");
    assert!(shows_text(&frame(&mut app, vec![]), "Chat"));
    assert!(!app.sidebar_visible);
}

#[test]
fn fullscreen_prompt_takes_first_keystroke_on_entry_and_focus_return() {
    let context = egui::Context::default();
    let mut app = terminal_app(&context);
    app.startup.finish();
    set_open(&mut app, &context, true);
    let frame = |app: &mut ReDox, focused, events| {
        context.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(1100.0, 800.0))),
                focused,
                events,
                ..Default::default()
            },
            |context| app.draw_ui(context),
        )
    };
    app.focus_prompt = false;
    frame(&mut app, true, vec![]);
    frame(
        &mut app,
        true,
        vec![
            egui::Event::Key {
                key: egui::Key::F11,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            },
            egui::Event::Text("first".into()),
        ],
    );
    assert!(is_fullscreen(&context));
    assert_eq!(app.session.active().unwrap().draft, "first");

    frame(&mut app, false, vec![egui::Event::WindowFocused(false)]);
    context.memory_mut(|memory| memory.surrender_focus(app.prompt_input_id.unwrap()));
    frame(
        &mut app,
        true,
        vec![
            egui::Event::WindowFocused(true),
            egui::Event::Text(" again".into()),
        ],
    );
    assert_eq!(app.session.active().unwrap().draft, "first again");
    assert!(is_fullscreen(&context));
}

#[test]
fn fullscreen_leaves_modified_system_shortcuts_and_ignores_repeated_f11() {
    let context = egui::Context::default();
    let mut app = terminal_app(&context);
    app.startup.finish();
    set_open(&mut app, &context, true);
    let modified_escape = |modifiers| egui::Event::Key {
        key: egui::Key::Escape,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    };
    let f11 = egui::Event::Key {
        key: egui::Key::F11,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    };
    let frame = |app: &mut ReDox, event| {
        context.run(
            egui::RawInput {
                events: vec![event],
                ..Default::default()
            },
            |context| app.draw_ui(context),
        )
    };
    frame(&mut app, f11.clone());
    for event in [
        f11,
        modified_escape(egui::Modifiers::CTRL | egui::Modifiers::COMMAND),
        modified_escape(egui::Modifiers::ALT),
        modified_escape(egui::Modifiers::SHIFT),
    ] {
        let output = frame(&mut app, event);
        assert!(is_fullscreen(&context));
        assert!(!output.viewport_output[&egui::ViewportId::ROOT]
            .commands
            .contains(&egui::ViewportCommand::Fullscreen(false)));
    }
}

#[test]
fn fullscreen_returns_to_window_sized_terminal_on_restore() {
    let context = egui::Context::default();
    let mut app = terminal_app(&context);
    set_open(&mut app, &context, true);
    app.session.active_mut().unwrap().draft = "Keep typing here".into();
    let screen = Rect::from_min_size(egui::Pos2::ZERO, vec2(900.0, 600.0));
    let frame = |app: &mut ReDox, events| {
        context.run(
            egui::RawInput {
                screen_rect: Some(screen),
                events,
                ..Default::default()
            },
            |context| app.draw_ui(context),
        )
    };
    let shows_chat = |frame: &egui::FullOutput| {
        frame.shapes.iter().any(
            |shape| matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text() == "Chat"),
        )
    };
    assert!(!shows_chat(&frame(&mut app, vec![])));
    let fullscreen = frame(
        &mut app,
        vec![egui::Event::Key {
            key: egui::Key::F11,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
    );
    let commands = &fullscreen.viewport_output[&egui::ViewportId::ROOT].commands;
    assert!(commands.contains(&egui::ViewportCommand::Fullscreen(true)));
    let terminal_rect = Rect::from_min_max(screen.min, screen.max - vec2(0.0, 34.0));
    assert!(fullscreen.shapes.iter().any(|shape| {
        matches!(&shape.shape, egui::Shape::Rect(panel) if panel.rect == terminal_rect && panel.fill == theme::sidebar())
    }));

    let restored = frame(&mut app, vec![escape_pressed()]);
    let commands = &restored.viewport_output[&egui::ViewportId::ROOT].commands;
    assert!(commands.contains(&egui::ViewportCommand::Fullscreen(false)));
    assert!(!shows_chat(&frame(&mut app, vec![])));
    assert_eq!(app.session.active().unwrap().draft, "Keep typing here");
}

fn escape_pressed() -> egui::Event {
    egui::Event::Key {
        key: egui::Key::Escape,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }
}

#[test]
fn restores_terminal_mode_from_saved_preferences() {
    #[derive(Default)]
    struct MemoryStorage(HashMap<String, String>);

    impl eframe::Storage for MemoryStorage {
        fn get_string(&self, key: &str) -> Option<String> {
            self.0.get(key).cloned()
        }

        fn set_string(&mut self, key: &str, value: String) {
            self.0.insert(key.into(), value);
        }

        fn flush(&mut self) {}
    }

    let context = egui::Context::default();
    let mut app = terminal_app(&context);
    let mut storage = MemoryStorage::default();
    let restart = |storage: &MemoryStorage| {
        let mut creation = eframe::CreationContext::_new_kittest(egui::Context::default());
        creation.storage = Some(storage);
        let app = ReDox::new_internal(&creation, None, Default::default(), None);
        (
            app.terminal_is_open(&creation.egui_ctx),
            app.ui_preferences.win98_terminal_docked,
        )
    };
    set_open(&mut app, &context, true);
    eframe::App::save(&mut app, &mut storage);
    assert_eq!(restart(&storage), (true, false));
    commands::run(&mut app, &context, "/dock");
    eframe::App::save(&mut app, &mut storage);
    assert_eq!(restart(&storage), (true, true));

    let preferences_key = crate::app::ui::PREFERENCES_KEY;
    let preferences = eframe::Storage::get_string(&storage, preferences_key).unwrap();
    let legacy = format!(
        "(studio_skin:Some(\"retired.redoxskin\"),follow_studio:true,{}",
        preferences.trim_start().strip_prefix('(').unwrap(),
    );
    eframe::Storage::set_string(&mut storage, preferences_key, legacy);
    assert_eq!(restart(&storage), (true, true));
    commands::run(&mut app, &context, "/chat");
    eframe::App::save(&mut app, &mut storage);
    assert_eq!(restart(&storage), (false, true));
}

#[test]
fn sends_the_shared_draft_and_shows_replies_and_changes_in_order() {
    let context = egui::Context::default();
    let mut app = terminal_app(&context);
    app.runtime = Some(
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap(),
    );
    set_open(&mut app, &context, true);
    app.focus_prompt = true;

    let frame = |app: &mut ReDox, modifiers, events| {
        let _ = context.run(
            egui::RawInput {
                modifiers,
                events,
                ..Default::default()
            },
            |context| {
                egui::CentralPanel::default().show(context, |ui| {
                    show(ui, ui.available_rect_before_wrap(), app);
                });
            },
        );
    };
    let enter = |modifiers, pressed| egui::Event::Key {
        key: egui::Key::Enter,
        physical_key: None,
        pressed,
        repeat: false,
        modifiers,
    };
    let none = egui::Modifiers::NONE;
    let shift = egui::Modifiers::SHIFT;
    frame(&mut app, none, vec![]);
    assert_eq!(
        app.prompt_input_id,
        context.memory(|memory| memory.focused())
    );
    frame(&mut app, none, vec![egui::Event::Text("Change x".into())]);
    frame(
        &mut app,
        shift,
        vec![enter(shift, true), enter(shift, false)],
    );
    frame(&mut app, none, vec![egui::Event::Text("to y".into())]);
    assert_eq!(app.session.active().unwrap().draft, "Change x\nto y");
    assert!(app.session.active().unwrap().messages.is_empty());

    let attachment_id = uuid::Uuid::new_v4();
    app.session
        .active_mut()
        .unwrap()
        .attachments
        .push(Attachment {
            id: attachment_id,
            path: "example.rs".into(),
            text: Some("fn example() {}".into()),
        });
    frame(&mut app, none, vec![enter(none, true), enter(none, false)]);
    let escaped = context.run(
        egui::RawInput {
            events: vec![escape_pressed()],
            ..Default::default()
        },
        |context| eframe::App::update(&mut app, context, &mut eframe::Frame::_new_kittest()),
    );
    assert!(app.terminal_is_open(&context));
    assert!(app.running.is_some());
    assert!(
        !escaped.viewport_output[&egui::ViewportId::ROOT]
            .commands
            .contains(&egui::ViewportCommand::Fullscreen(false))
    );
    let chat = app.session.active_mut().unwrap();
    assert!(chat.draft.is_empty());
    assert!(chat.attachments.is_empty());
    assert_eq!(chat.messages.len(), 2);
    assert_eq!(chat.messages[0].content, "Change x\nto y");
    assert_eq!(chat.messages[0].attachments[0].id, attachment_id);
    assert_eq!(chat.messages[1].status, MessageStatus::Streaming);
    chat.messages[1].content = "Updated the file.".into();
    chat.messages[1].activity.replace_diff(
        "diff --git a/name.txt b/name.txt\n--- a/name.txt\n+++ b/name.txt\n@@ -1 +1 @@\n-x\n+y\n",
    );
    let (rows, attachments) = terminal_rows(Some(chat), plain_lines);
    assert_eq!(attachments[&2].id, attachment_id);
    assert_eq!(
        rows.iter().map(TranscriptRow::text).collect::<Vec<_>>(),
        [
            "> Change x",
            "to y",
            "[Attachment: example.rs]",
            "",
            "name.txt",
            "----- remove -----",
            "",
            "x",
            "",
            "1 line",
            "----- remove -----",
            "",
            "----- add -----",
            "",
            "y",
            "",
            "1 line",
            "----- add -----",
            "",
            "----- redox -----",
            "",
            "Updated the file.",
            "",
        ]
    );

    app.ui_preferences.win98_terminal_zoom = ZOOM_MAX;
    let narrow = Rect::from_min_size(egui::pos2(20.0, 30.0), vec2(300.0, 360.0));
    let output = context.run(egui::RawInput::default(), |context| {
        egui::CentralPanel::default().show(context, |ui| show(ui, narrow, &mut app));
    });
    for label in ["Queue", "Stop"] {
        let (bounds, clip_rect) = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text() == label => {
                    Some((text.visual_bounding_rect(), shape.clip_rect))
                }
                _ => None,
            })
            .unwrap();
        assert!(clip_rect.contains_rect(bounds), "{label} is clipped");
    }
    let running_chat = app.session.active_chat;
    app.new_chat();
    commands::run(&mut app, &context, "/stop");
    assert!(app.running.is_some());
    let current = app.session.active().unwrap().id;
    assert!(
        commands::output(&context, current)
            .unwrap()
            .contains("No response is running in this chat.")
    );
    app.session.active_chat = running_chat;
    commands::run(&mut app, &context, "/stop");
    assert!(app.running.is_none());
}

fn terminal_app(context: &egui::Context) -> ReDox {
    let mut app = ReDox::new_internal(
        &eframe::CreationContext::_new_kittest(context.clone()),
        None,
        Default::default(),
        Some(win98::ID),
    );
    app.ui_preferences.win98_terminal_appearance.crt.enabled = false;
    app.ui_preferences.win98_terminal_zoom = 100;
    app
}

#[test]
fn slash_commands_run_locally_from_the_prompt() {
    let context = egui::Context::default();
    let mut app = terminal_app(&context);
    set_open(&mut app, &context, true);
    app.focus_prompt = true;
    let chat = app.session.active_mut().unwrap();
    chat.draft = "/help".into();
    let id = chat.id;
    let render = |context: &egui::Context| {
        egui::CentralPanel::default().show(context, |ui| {
            show(ui, ui.available_rect_before_wrap(), &mut app);
        });
    };
    let mut render = render;
    let _ = context.run(egui::RawInput::default(), &mut render);
    let output = context.run(
        egui::RawInput {
            events: vec![egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            ..Default::default()
        },
        render,
    );
    let help = commands::output(&context, id).unwrap();
    assert!(help.starts_with("> /help\n"));
    assert!(help.contains("/model effort"));
    assert!(output.shapes.iter().any(|shape| {
        matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text().contains("List commands"))
    }));
    let chat = app.session.active().unwrap();
    assert!(chat.draft.is_empty() && chat.messages.is_empty());
    assert!(app.running.is_none());

    commands::run(&mut app, &context, "/ollama");
    assert!(app.ui_preferences.agents_open);
    commands::run(&mut app, &context, "/usage check");
    assert!(app.usage_open);
    commands::run(&mut app, &context, "/settings models");
    assert!(app.settings_open && app.settings_page == super::super::SettingsPage::Models);
    app.session.active_mut().unwrap().title = "Earlier chat".into();
    commands::run(&mut app, &context, "/newchat");
    assert_ne!(app.session.active_chat, Some(id));
}

#[test]
fn saffyos_loop_varies_prompts_and_stop_disarms_the_next_turn() {
    use redox::ollama::Model;
    use redox::run::{RunEvent, Update};

    let context = egui::Context::default();
    let mut app = terminal_app(&context);
    app.startup.finish();
    app.runtime = Some(
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap(),
    );
    app.ui_preferences.saffyos_folder = std::env::current_dir().unwrap().display().to_string();
    commands::run(&mut app, &context, "/saffyOS loop");
    let chat = app.session.active_chat.unwrap();
    app.ollama_models = vec![Model {
        name: "Aisnley-Ox-1.6-bbb:35b".into(),
        details: Default::default(),
    }];
    app.ollama_redox_chat = None;
    app.finish_terminal_redox(&context, chat, true);
    assert!(app.saffyos_loop.as_ref().unwrap().due.is_none());

    app.session.active_mut().unwrap().draft = "Create a script".into();
    app.send_prompt(&context);
    app.session.active_mut().unwrap().draft = "Unsent note".into();

    let complete = |app: &mut ReDox| {
        app.run_sender
            .try_send(RunEvent {
                run: app.running.as_ref().unwrap().id,
                update: Update::Complete,
            })
            .unwrap();
        app.receive_events(&context);
    };

    app.new_chat();
    let other_chat = app.session.active_chat;
    let failed = app.running.as_ref().unwrap().id;
    app.run_sender
        .try_send(RunEvent {
            run: failed,
            update: Update::Failed("Ollama did not finish.".into()),
        })
        .unwrap();
    app.receive_events(&context);
    assert!(app.running.is_none());
    assert!(app.status.contains("auto-loop will retry"));
    app.saffyos_loop.as_mut().unwrap().due = Some(std::time::Instant::now());
    app.dispatch_saffyos_continuation(&context);
    let retried = app.running.as_ref().unwrap().id;
    assert_eq!(retried.message, failed.message);
    assert_ne!(retried.id, failed.id);
    assert_eq!(app.session.active_chat, other_chat);
    let locked = app
        .session
        .chats
        .iter()
        .find(|entry| entry.id == chat)
        .unwrap();
    assert_eq!(locked.messages.len(), 2);
    assert_eq!(locked.messages[0].content, "Create a script");

    for expected in crate::app::SAFFYOS_CONTINUATIONS.iter().take(2) {
        complete(&mut app);
        app.dispatch_saffyos_continuation(&context);
        assert!(app.running.is_none());
        let loop_state = app.saffyos_loop.as_mut().unwrap();
        assert!(loop_state.due.is_some());
        loop_state.due = Some(std::time::Instant::now());
        app.dispatch_saffyos_continuation(&context);
        assert_eq!(app.session.active_chat, other_chat);
        assert_eq!(app.running.as_ref().unwrap().id.chat, chat);
        let locked = app
            .session
            .chats
            .iter()
            .find(|entry| entry.id == chat)
            .unwrap();
        assert_eq!(
            locked.messages[locked.messages.len() - 2].content,
            *expected
        );
        assert_eq!(locked.draft, "Unsent note");
    }

    complete(&mut app);
    app.session.active_chat = Some(chat);
    commands::run(&mut app, &context, "/stop");
    assert!(app.saffyos_loop.is_none());
    assert!(app.session.active().unwrap().is_saffyos_locked());
    assert!(
        commands::output(&context, chat)
            .unwrap()
            .contains("auto-loop stopped")
    );
}

#[test]
fn saffyos_prefers_ainsley_1_6_and_falls_back_to_1_5() {
    use crate::app::Event;
    use redox::{domain::Provider, ollama::Model};

    let context = egui::Context::default();
    let mut app = terminal_app(&context);
    let previous_chat = app.session.active_chat;
    let folder = std::env::current_dir().unwrap().canonicalize().unwrap();
    app.ui_preferences.saffyos_folder = folder.display().to_string();
    let endpoint = app.session.ollama.endpoint.clone();
    let deliver_models = |app: &mut ReDox, names: &[&str]| {
        let models = names
            .iter()
            .map(|name| Model {
                name: (*name).into(),
                details: Default::default(),
            })
            .collect();
        app.sender
            .send(Event::OllamaModels(
                app.ollama_generation,
                endpoint.clone(),
                Ok(models),
            ))
            .unwrap();
        app.receive_events(&context);
    };

    // Use synthetic discovery events so the test does not depend on a running Ollama server.
    app.runtime = None;
    commands::run(&mut app, &context, "/saffos locked forever");
    let chat = app.session.active_chat.unwrap();
    assert_ne!(Some(chat), previous_chat);
    assert_eq!(
        app.session.active().unwrap().workspace.as_ref(),
        Some(&folder)
    );
    assert_eq!(app.session.workspace.as_ref(), Some(&folder));
    assert!(
        commands::output(&context, chat)
            .unwrap()
            .contains("runtime is unavailable")
    );
    app.ollama_redox_chat = Some(chat);
    deliver_models(
        &mut app,
        &["Aisnley-Ox-1.2-bbb:35b", "Aisnley-Ox-1.5-bbb:exp-test"],
    );
    let locked = app.session.active().unwrap();
    assert!(locked.is_saffyos_locked());
    assert_eq!(locked.provider, Provider::Ollama);
    assert!(locked.ollama_model.is_empty());
    assert!(
        commands::output(&context, chat)
            .unwrap()
            .contains("Ainsley Ox BBB 1.6 or 1.5 was not found")
    );

    app.ollama_redox_chat = Some(chat);
    deliver_models(
        &mut app,
        &[
            "Aisnley-Ox-1.5-bbb:exp-loop-check",
            "Aisnley-Ox-1.5-bbb:exp-invariants",
            "Aisnley-Ox-1.5-bbb:exp-coding-sampling",
        ],
    );
    let active = app.session.active().unwrap();
    assert_eq!(active.ollama_model, "Aisnley-Ox-1.5-bbb:exp-invariants");

    app.ollama_redox_chat = Some(chat);
    deliver_models(
        &mut app,
        &[
            "ainsley-1.9-ox.bbb:latest",
            "Aisnley-Ox-1.5-bbb:35b",
            "Aisnley-Ox-1.5-bbb:exp-invariants",
            "ainsley-1.5-ox.bbb:latest",
            "ainsley-1.99-ox.other:latest",
        ],
    );
    let active = app.session.active().unwrap();
    assert_eq!(active.id, chat);
    assert_eq!(active.provider, Provider::Ollama);
    assert_eq!(active.ollama_model, "Aisnley-Ox-1.5-bbb:35b");
    assert!(
        commands::output(&context, chat)
            .unwrap()
            .contains("Ready for your request")
    );

    app.ollama_redox_chat = Some(chat);
    deliver_models(
        &mut app,
        &["Aisnley-Ox-1.5-bbb:35b", "Aisnley-Ox-1.6-bbb:35b"],
    );
    let active = app.session.active().unwrap();
    assert_eq!(active.id, chat);
    assert_eq!(active.ollama_model, "Aisnley-Ox-1.6-bbb:35b");

    app.ui_preferences.saffyos_model = "other-model:latest".into();
    commands::run(&mut app, &context, "saffyos locked");
    assert_eq!(app.session.active_chat, Some(chat));
    app.ollama_redox_chat = Some(chat);
    deliver_models(&mut app, &["Aisnley-Ox-1.6-bbb:35b"]);
    assert!(
        commands::output(&context, chat)
            .unwrap()
            .contains("other-model:latest was not found")
    );

    app.ollama_redox_chat = Some(chat);
    deliver_models(&mut app, &["Aisnley-Ox-1.6-bbb:35b", "other-model:latest"]);
    assert_eq!(
        app.session.active().unwrap().ollama_model,
        "other-model:latest"
    );

    commands::run(&mut app, &context, "/saffyOS");
    let workspace = app.session.active().unwrap();
    assert_ne!(workspace.id, chat);
    assert!(!workspace.is_saffyos_locked());
    assert_eq!(workspace.workspace.as_ref(), Some(&folder));
    assert!(workspace.mode == redox::domain::Mode::FullAccess);
    assert!(
        app.session
            .chats
            .iter()
            .find(|entry| entry.id == chat)
            .unwrap()
            .is_saffyos_locked()
    );
    let workspace_id = workspace.id;
    app.ollama_redox_chat = Some(workspace_id);
    deliver_models(&mut app, &["other-model:latest"]);
    assert_eq!(app.session.active().unwrap().provider, Provider::Ollama);
    assert!(
        commands::output(&context, workspace_id)
            .unwrap()
            .contains("Ready for your request")
    );

    commands::run(&mut app, &context, "/settings saffyos");
    assert!(app.settings_open);
    assert!(app.settings_page == crate::app::ui::SettingsPage::SaffyOs);
}

#[test]
fn access_command_reports_and_switches_the_current_chat_mode() {
    use redox::domain::Mode;

    let context = egui::Context::default();
    let mut app = terminal_app(&context);
    let folderless = app.session.active_chat.unwrap();
    commands::run(&mut app, &context, "/access full");
    assert!(app.session.active().unwrap().mode == Mode::Chat);
    assert!(
        commands::output(&context, folderless)
            .unwrap()
            .contains("/project")
    );

    app.session.new_project_chat("project".into());
    let project = app.session.active_chat.unwrap();
    commands::run(&mut app, &context, "/access");
    assert!(
        commands::output(&context, project)
            .unwrap()
            .contains("Current access: Full access")
    );
    assert_eq!(
        commands::suggestions(&app, "/access w")[0].input,
        "/access workspace"
    );
    for (command, mode) in [
        ("/access workspace", Mode::Code),
        ("/access read-only", Mode::Chat),
        ("/access full", Mode::FullAccess),
    ] {
        commands::run(&mut app, &context, command);
        assert!(app.session.active().unwrap().mode == mode);
    }
}

#[test]
fn slash_commands_set_only_supported_models_and_effort() {
    let context = egui::Context::default();
    let mut app = terminal_app(&context);
    let id = app.session.active().unwrap().id;
    configure_model(&mut app);
    commands::run(&mut app, &context, "/model test-model");
    assert_eq!(app.effective_model(), "test-model");
    commands::run(&mut app, &context, "/model effort high");
    assert_eq!(app.selected_effort().as_deref(), Some("high"));
    commands::run(&mut app, &context, "/model effort unsupported");
    assert_eq!(app.selected_effort().as_deref(), Some("high"));
    assert!(
        commands::output(&context, id)
            .unwrap()
            .contains("Unknown effort")
    );
    commands::run(&mut app, &context, "/model effort default");
    assert_eq!(app.selected_effort().as_deref(), Some("low"));
    assert!(commands::is_command("  /USAGE check "));
    assert!(commands::is_command("saffyos"));
    assert!(commands::is_command("SAFFYOS loop"));
    assert!(!commands::is_command("saffyos is the default"));
    assert!(!commands::is_command("/src/main.rs"));
    assert!(!commands::is_command("Read /src/main.rs"));
    for (typed, completed) in [
        ("/SAF", "/saffyOS"),
        ("/saffyos lo", "/saffyOS locked"),
        ("/saffyos loo", "/saffyOS loop"),
        ("/saffyos p", "/saffyOS project"),
        ("/settings saff", "/settings saffyos"),
        ("/model test", "/model test-model"),
        ("/model effort h", "/model effort high"),
        ("/setting wor", "/settings workspace"),
        ("/ollama u", "/ollama use"),
        ("/help /model e", "/help model effort"),
    ] {
        let suggestions = commands::suggestions(&app, typed);
        assert_eq!(suggestions[0].input, completed);
    }
    assert!(commands::suggestions(&app, "/model effort ultra").is_empty());
}

fn configure_model(app: &mut ReDox) {
    app.models = vec!["test-model".into()];
    app.default_model = Some("test-model".into());
    app.model_details = vec![redox::codex::ModelInfo {
        model: "test-model".into(),
        display_name: None,
        default_reasoning_effort: Some("low".into()),
        supported_reasoning_efforts: ["low", "high"]
            .into_iter()
            .map(|level| redox::codex::EffortOption {
                reasoning_effort: level.into(),
                description: String::new(),
            })
            .collect(),
    }];
}

#[test]
fn terminal_suggests_commands_and_completes_without_running_them() {
    let context = egui::Context::default();
    let mut app = terminal_app(&context);
    configure_model(&mut app);
    set_open(&mut app, &context, true);
    app.focus_prompt = true;
    let frame = |app: &mut ReDox, events| {
        context.run(
            egui::RawInput {
                events,
                ..Default::default()
            },
            |context| {
                egui::CentralPanel::default().show(context, |ui| {
                    show(ui, ui.available_rect_before_wrap(), app);
                });
            },
        )
    };
    let draft = |app: &ReDox| app.session.active().unwrap().draft.clone();
    let _ = frame(&mut app, vec![]);
    let typed = frame(&mut app, vec![egui::Event::Text("/mo".into())]);
    assert!(typed.shapes.iter().any(|shape| {
        matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text() == "/model effort")
    }));
    let _ = frame(
        &mut app,
        vec![egui::Event::Key {
            key: egui::Key::Tab,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
    );
    assert_eq!(draft(&app), "/model");
    let continued = frame(&mut app, vec![egui::Event::Text(" ".into())]);
    assert_eq!(draft(&app), "/model ");
    let suggestion = continued
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Text(text) if text.galley.text() == "/model effort" => {
                Some(text.visual_bounding_rect().center())
            }
            _ => None,
        })
        .unwrap();
    let _ = frame(&mut app, click(suggestion));
    assert_eq!(draft(&app), "/model effort");
    let _ = frame(&mut app, vec![egui::Event::Text(" h".into())]);
    let _ = frame(
        &mut app,
        vec![egui::Event::Key {
            key: egui::Key::Tab,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
    );
    assert_eq!(draft(&app), "/model effort high");
    assert_eq!(app.selected_effort().as_deref(), Some("low"));
    let _ = frame(
        &mut app,
        vec![egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
    );
    assert_eq!(app.selected_effort().as_deref(), Some("high"));
    assert!(app.session.active().unwrap().messages.is_empty());
    assert!(app.running.is_none());
}

#[test]
fn terminal_help_accepts_a_command_name_and_omits_unrelated_entries() {
    let context = egui::Context::default();
    let mut app = terminal_app(&context);
    let chat = app.session.active().unwrap().id;
    commands::run(&mut app, &context, "/help /model effort");
    let help = commands::output(&context, chat).unwrap();
    assert!(help.contains("\n/model effort [level]\n"));
    assert!(!help.contains("/usage check"));
    commands::run(&mut app, &context, "/fullscreen invalid");
    assert!(
        commands::output(&context, chat)
            .unwrap()
            .contains("Usage: /fullscreen")
    );
    assert!(!is_fullscreen(&context));
}

#[test]
fn terminal_settings_apply_a_hex_color_and_can_hide_the_title_bar() {
    let context = egui::Context::default();
    let mut app = terminal_app(&context);
    set_open(&mut app, &context, true);
    let frame = |app: &mut ReDox, events| {
        context.run(
            egui::RawInput {
                events,
                ..Default::default()
            },
            |context| {
                egui::CentralPanel::default().show(context, |ui| {
                    show(ui, ui.available_rect_before_wrap(), app);
                });
            },
        )
    };
    commands::run(&mut app, &context, "/setting");
    assert!(settings::is_open(&context));
    assert!(!app.settings_open);
    let _ = frame(&mut app, vec![]);
    let _ = frame(
        &mut app,
        vec![
            egui::Event::Key {
                key: egui::Key::A,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::COMMAND,
            },
            egui::Event::Text("#800080".into()),
        ],
    );
    let control = |rendered: &egui::FullOutput, label| {
        rendered
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text() == label => {
                    Some(text.visual_bounding_rect().center())
                }
                _ => None,
            })
            .unwrap()
    };
    let rendered = frame(&mut app, vec![]);
    let _ = frame(&mut app, click(control(&rendered, "CRT filter")));
    let preview = frame(&mut app, vec![]);
    for label in [
        "Screen curvature",
        "RGB scanline brightness",
        "RGB scanline bloom",
    ] {
        assert!(preview.shapes.iter().any(|shape| {
            matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text() == label)
        }));
    }
    assert!(preview.shapes.iter().any(|shape| {
        matches!(&shape.shape, egui::Shape::Vec(layers) if
            matches!(layers.last(), Some(egui::Shape::Text(text)) if text.galley.text() == "ReDox Terminal"))
    }));
    let apply = control(&preview, "Apply");
    let _ = frame(&mut app, click(apply));
    assert!(!settings::is_open(&context));
    assert_eq!(
        app.ui_preferences.win98_terminal_appearance.caption,
        settings::Caption::Solid([128, 0, 128])
    );
    assert!(app.ui_preferences.win98_terminal_appearance.crt.enabled);
    assert!(frame(&mut app, vec![]).shapes.iter().any(|shape| {
        matches!(&shape.shape, egui::Shape::Rect(rect) if rect.fill == Color32::from_rgb(128, 0, 128))
    }));

    assert_eq!(
        settings::Caption::parse("000000"),
        Some(settings::Caption::Solid([0, 0, 0]))
    );
    assert_eq!(settings::Caption::parse("#12345z"), None);
    app.ui_preferences.win98_terminal_appearance.caption = settings::Caption::parse(" ").unwrap();
    assert!(!frame(&mut app, vec![]).shapes.iter().any(|shape| {
        matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text() == "ReDox Terminal")
    }));
    commands::run(&mut app, &context, "/settings");
    assert!(settings::is_open(&context));
}

#[test]
fn terminal_centers_the_prompt_text_with_the_send_button() {
    let context = egui::Context::default();
    let mut app = terminal_app(&context);
    app.session.active_mut().unwrap().draft = "draft".into();
    let frame = context.run(egui::RawInput::default(), |context| {
        egui::CentralPanel::default().show(context, |ui| {
            show(ui, ui.available_rect_before_wrap(), &mut app);
        });
    });
    let center = |label| {
        frame
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text() == label => {
                    Some(text.pos.y + text.galley.size().y / 2.0)
                }
                _ => None,
            })
            .unwrap()
    };
    assert!((center(">") - center("draft")).abs() <= 1.0);
    assert!((center("draft") - center("Send")).abs() <= 1.0);
}

#[test]
fn crt_filter_is_scoped_to_terminal_text_and_supports_extreme_settings() {
    let context = egui::Context::default();
    let mut app = terminal_app(&context);
    set_open(&mut app, &context, true);
    app.prepare_test_scene(true);
    app.session.active_mut().unwrap().draft = "CRT test".into();
    let rect = Rect::from_min_size(egui::pos2(20.0, 20.0), vec2(500.0, 320.0));
    let draw = |app: &mut ReDox| {
        context.run(egui::RawInput::default(), |context| {
            egui::CentralPanel::default().show(context, |ui| show(ui, rect, app));
        })
    };
    let original_text = |frame: &egui::FullOutput| {
        frame.shapes.iter().any(|entry| {
            matches!(&entry.shape, egui::Shape::Text(text) if text.galley.text() == "CRT test")
        })
    };
    assert!(original_text(&draw(&mut app)));
    app.ui_preferences.win98_terminal_appearance.crt = crt::Settings {
        enabled: true,
        scanline_strength: 100,
        scanline_spacing: 2,
        halo_strength: 100,
        rgb_offset: 8,
        curvature: 0,
        scanline_brightness: 0,
        scanline_bloom: 0,
    };
    let filtered = draw(&mut app);
    assert!(filtered.shapes.iter().any(|entry| {
        matches!(&entry.shape, egui::Shape::Text(text) if text.galley.text() == "ReDox Terminal")
    }));
    assert!(original_text(&filtered));
    let (text_index, halo) = filtered.shapes.iter().enumerate().find_map(|(index, entry)| {
        let egui::Shape::Vec(layers) = &entry.shape else { return None };
        matches!(layers.last(), Some(egui::Shape::Text(text)) if text.galley.text() == "----- redox -----")
            .then_some((index, layers))
    }).unwrap();
    assert_eq!(halo.len(), 7);
    assert!(
        matches!(&halo[3], egui::Shape::Text(text) if text.override_text_color == Some(Color32::from_rgb(255, 45, 40)))
    );
    assert!(matches!(&halo[6], egui::Shape::Text(text) if text.override_text_color.is_none()));
    let (mesh_index, scanlines, clip) = filtered
        .shapes
        .iter()
        .enumerate()
        .find_map(|(index, entry)| match &entry.shape {
            egui::Shape::Mesh(mesh)
                if mesh.vertices.len() > 200
                    && mesh.vertices.iter().all(|vertex| vertex.color == BLACK) =>
            {
                Some((index, mesh, entry.clip_rect))
            }
            _ => None,
        })
        .unwrap();
    assert!(mesh_index < text_index, "opaque scanlines must be behind text");
    assert!(clip.top() > rect.top() + 20.0);
    assert_eq!(
        scanlines.vertices[4].pos.y - scanlines.vertices[0].pos.y,
        2.0
    );
    assert!(
        !context
            .tessellate(filtered.shapes, filtered.pixels_per_point)
            .is_empty()
    );

    let appearance = &mut app.ui_preferences.win98_terminal_appearance;
    let stored = serde_json::to_string(appearance).unwrap();
    let restored: Appearance = serde_json::from_str(&stored).unwrap();
    assert!(restored.crt.enabled);
    assert_eq!(restored.crt.rgb_offset, 8);
    appearance.crt.scanline_strength = 0;
    appearance.crt.halo_strength = 0;
    assert!(original_text(&draw(&mut app)));
}

#[test]
fn crt_curvature_bows_the_transcript_and_scanlines_at_the_edges() {
    let context = egui::Context::default();
    let mut app = terminal_app(&context);
    set_open(&mut app, &context, true);
    let chat = app.session.active_mut().unwrap();
    chat.draft = "Straight input".into();
    chat.messages.push(redox::domain::Message {
        id: uuid::Uuid::new_v4(),
        role: Role::User,
        status: MessageStatus::Complete,
        content: "Curved transcript".into(),
        error: None,
        model: String::new(),
        attachments: Vec::new(),
        activity: Default::default(),
    });
    app.ui_preferences.win98_terminal_appearance.crt = crt::Settings {
        enabled: true,
        scanline_strength: 100,
        halo_strength: 0,
        curvature: 0,
        scanline_brightness: 0,
        scanline_bloom: 0,
        ..Default::default()
    };
    let window = Rect::from_min_size(egui::pos2(20.0, 20.0), vec2(500.0, 320.0));
    let frame = |app: &mut ReDox| {
        context.run(egui::RawInput::default(), |context| {
            egui::CentralPanel::default().show(context, |ui| show(ui, window, app));
        })
    };
    let text_bounds = |frame: &egui::FullOutput, label| {
        frame
            .shapes
            .iter()
            .find_map(|entry| match &entry.shape {
                egui::Shape::Text(text) if text.galley.text() == label => {
                    Some(text.visual_bounding_rect())
                }
                _ => None,
            })
            .unwrap()
    };
    let flat = frame(&mut app);
    app.ui_preferences.win98_terminal_appearance.crt.curvature = 100;
    let curved = frame(&mut app);
    let before = text_bounds(&flat, "> Curved transcript");
    let after = text_bounds(&curved, "> Curved transcript");
    assert!(after.left() > before.left());
    assert_eq!(
        text_bounds(&flat, "Straight input"),
        text_bounds(&curved, "Straight input")
    );

    let lines = curved
        .shapes
        .iter()
        .find_map(|entry| match &entry.shape {
            egui::Shape::Mesh(mesh)
                if mesh.vertices.len() > 1_000
                    && mesh.vertices.iter().all(|vertex| vertex.color == BLACK) =>
            {
                Some(mesh)
            }
            _ => None,
        })
        .unwrap();
    let corner = lines.vertices[0].pos;
    let middle = lines.vertices[21].pos;
    assert!((6.0..=7.0).contains(&(corner.y - middle.y)));
    assert!(window.contains(corner) && window.contains(middle));
    assert!(
        !context
            .tessellate(curved.shapes, curved.pixels_per_point)
            .is_empty()
    );
}

#[test]
fn crt_defaults_match_the_release_preset() {
    let appearance = Appearance::default();
    assert_eq!(
        serde_json::to_value(appearance.crt).unwrap(),
        serde_json::json!({
            "enabled": true,
            "scanline_strength": 40,
            "scanline_spacing": 3,
            "scanline_brightness": 7,
            "scanline_bloom": 10,
            "halo_strength": 32,
            "rgb_offset": 2,
            "curvature": 85,
        })
    );
}

#[test]
fn crt_scanline_brightness_and_bloom_are_independent() {
    let context = egui::Context::default();
    let mut app = terminal_app(&context);
    set_open(&mut app, &context, true);
    app.ui_preferences.win98_terminal_appearance.crt = serde_json::from_str(
        r#"{"enabled":true,"scanline_strength":0,"halo_strength":0,"curvature":0,"scanline_brightness":0,"scanline_bloom":0}"#,
    )
    .unwrap();
    let rect = Rect::from_min_size(egui::pos2(20.0, 20.0), vec2(500.0, 320.0));
    let light = |app: &mut ReDox| {
        let output = context.run(egui::RawInput::default(), |context| {
            egui::CentralPanel::default().show(context, |ui| show(ui, rect, app));
        });
        output
            .shapes
            .into_iter()
            .find_map(|entry| match entry.shape {
                egui::Shape::Mesh(mesh)
                    if mesh.vertices.iter().any(|vertex| {
                        vertex.color.a() == 0 && vertex.color != Color32::TRANSPARENT
                    }) =>
                {
                    Some((mesh, entry.clip_rect))
                }
                _ => None,
            })
    };

    assert!(light(&mut app).is_none());
    app.ui_preferences
        .win98_terminal_appearance
        .crt
        .scanline_brightness = 25;
    let (dim, _) = light(&mut app).unwrap();
    app.ui_preferences
        .win98_terminal_appearance
        .crt
        .scanline_brightness = 100;
    let (bright, _) = light(&mut app).unwrap();
    assert!(bright.vertices[0].color.r() > dim.vertices[0].color.r());
    assert!(bright.vertices[4].color.g() > bright.vertices[4].color.r());
    assert!(bright.vertices[8].color.b() > bright.vertices[8].color.r());
    let core_height = bright.vertices[2].pos.y - bright.vertices[0].pos.y;
    assert_eq!(core_height, 1.0);

    app.ui_preferences
        .win98_terminal_appearance
        .crt
        .scanline_brightness = 0;
    app.ui_preferences
        .win98_terminal_appearance
        .crt
        .scanline_bloom = 100;
    let (glow, clip) = light(&mut app).unwrap();
    assert_eq!(glow.vertices[0].color, Color32::TRANSPARENT);
    assert_ne!(glow.vertices[2].color, Color32::TRANSPARENT);
    assert_eq!(glow.vertices[6].color, Color32::TRANSPARENT);
    assert!(glow.vertices[6].pos.y - glow.vertices[0].pos.y > core_height);
    assert!(rect.contains_rect(clip));

    app.ui_preferences
        .win98_terminal_appearance
        .crt
        .scanline_bloom = 0;
    assert!(light(&mut app).is_none());
}

fn plain_lines(text: &str, _ainsley: bool) -> Vec<TranscriptRow> {
    text.lines().map(TranscriptRow::from).collect()
}

fn click(pos: egui::Pos2) -> Vec<egui::Event> {
    vec![
        egui::Event::PointerMoved(pos),
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        },
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        },
    ]
}
