use super::*;
use redox::activity::TurnActivity;

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
        rows,
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
fn shows_an_empty_prompt_and_keeps_terminal_state_in_the_current_window() {
    let context = egui::Context::default();
    let mut app = terminal_app(&context);
    assert!(!is_open(&context));
    assert!(terminal_rows(app.session.active(), plain_lines).is_empty());
    set_open(&context, true);
    assert!(is_open(&context));
    assert!(!is_open(&egui::Context::default()));
    let rect = Rect::from_min_size(egui::pos2(20.0, 30.0), vec2(400.0, 300.0));
    let close = egui::pos2(rect.right() - 14.0, rect.top() + 13.0);
    let mut render = |context: &egui::Context| {
        egui::CentralPanel::default().show(context, |ui| show(ui, rect, &mut app));
    };
    let _ = context.run(egui::RawInput::default(), &mut render);
    let result = context.run(
        egui::RawInput {
            events: click(close),
            ..Default::default()
        },
        render,
    );
    assert_eq!(result.platform_output.cursor_icon, egui::CursorIcon::None);
    assert!(!is_open(&context));
    assert!(app.focus_prompt);
}

#[test]
fn scales_the_bundled_font_without_resizing_the_window() {
    let context = egui::Context::default();
    let mut app = terminal_app(&context);
    set_open(&context, true);
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
    let zoom_in = egui::pos2(available.right() - 54.0, available.top() + 13.0);
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
fn terminal_sits_below_the_chat_caption_and_covers_the_chat_body() {
    let context = egui::Context::default();
    let mut app = terminal_app(&context);
    let frame = |app: &mut ReDox| {
        context.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(900.0, 600.0))),
                ..Default::default()
            },
            |context| app.win98_conversation(context),
        )
    };
    let text_position = |frame: &egui::FullOutput, label| {
        frame.shapes.iter().find_map(|shape| match &shape.shape {
            egui::Shape::Text(text) if text.galley.text() == label => Some(text.pos),
            _ => None,
        })
    };

    assert!(text_position(&frame(&mut app), "Begin").is_some());
    set_open(&context, true);
    let terminal = frame(&mut app);
    let chat_caption = text_position(&terminal, "Chat").unwrap();
    let terminal_caption = text_position(&terminal, "ReDox Terminal").unwrap();
    assert!(terminal_caption.y - chat_caption.y > ROW_HEIGHT);
    assert!(text_position(&terminal, "Begin").is_none());
    let panels: Vec<_> = terminal
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            egui::Shape::Rect(panel)
                if panel.fill == theme::sidebar() && panel.rect.width() > 450.0 =>
            {
                Some(panel.rect)
            }
            _ => None,
        })
        .collect();
    let [chat, terminal] = panels.as_slice() else {
        panic!("expected Chat and Terminal frames");
    };
    assert_eq!(chat.right() - terminal.right(), 10.0);
    assert_eq!(chat.bottom() - terminal.bottom(), 10.0);
}

#[test]
fn fullscreen_fills_the_root_viewport_and_restores_the_chat_layout() {
    let context = egui::Context::default();
    let mut app = terminal_app(&context);
    set_open(&context, true);
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
    assert!(shows_chat(&frame(&mut app, vec![])));
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
    assert!(!shows_chat(&fullscreen));
    assert!(fullscreen.shapes.iter().any(|shape| {
        matches!(&shape.shape, egui::Shape::Rect(panel) if panel.rect == screen && panel.fill == theme::sidebar())
    }));

    let restore_button = egui::pos2(screen.right() - 34.0, screen.top() + 13.0);
    let restored = frame(&mut app, click(restore_button));
    let commands = &restored.viewport_output[&egui::ViewportId::ROOT].commands;
    assert!(commands.contains(&egui::ViewportCommand::Fullscreen(false)));
    assert!(shows_chat(&frame(&mut app, vec![])));
    assert_eq!(app.session.active().unwrap().draft, "Keep typing here");
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
    set_open(&context, true);
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

    frame(&mut app, none, vec![enter(none, true), enter(none, false)]);
    let chat = app.session.active_mut().unwrap();
    assert!(chat.draft.is_empty());
    assert_eq!(chat.messages.len(), 2);
    assert_eq!(chat.messages[0].content, "Change x\nto y");
    assert_eq!(chat.messages[1].status, MessageStatus::Streaming);
    chat.messages[1].content = "Updated the file.".into();
    chat.messages[1].activity.replace_diff(
        "diff --git a/name.txt b/name.txt\n--- a/name.txt\n+++ b/name.txt\n@@ -1 +1 @@\n-x\n+y\n",
    );
    assert_eq!(
        terminal_rows(Some(chat), plain_lines),
        [
            "> Change x",
            "to y",
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
    app
}

#[test]
fn slash_commands_run_locally_from_the_prompt() {
    let context = egui::Context::default();
    let mut app = terminal_app(&context);
    set_open(&context, true);
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
    assert!(!commands::is_command("/src/main.rs"));
    assert!(!commands::is_command("Read /src/main.rs"));
    for (typed, completed) in [
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
    set_open(&context, true);
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
    set_open(&context, true);
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
    set_open(&context, true);
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
    let halo = filtered.shapes.iter().find_map(|entry| match &entry.shape {
        egui::Shape::Vec(layers) if matches!(layers.last(), Some(egui::Shape::Text(text)) if text.galley.text() == "CRT test") => {
            Some(layers)
        }
        _ => None,
    }).unwrap();
    assert_eq!(halo.len(), 7);
    assert!(
        matches!(&halo[3], egui::Shape::Text(text) if text.override_text_color == Some(Color32::from_rgb(255, 45, 40)))
    );
    assert!(matches!(&halo[6], egui::Shape::Text(text) if text.override_text_color.is_none()));
    let scanlines = filtered
        .shapes
        .iter()
        .find_map(|entry| match &entry.shape {
            egui::Shape::Mesh(mesh)
                if mesh.vertices.len() > 200
                    && mesh.vertices.iter().all(|vertex| vertex.color == BLACK) =>
            {
                Some((mesh, entry.clip_rect))
            }
            _ => None,
        })
        .unwrap();
    assert!(scanlines.1.top() > rect.top() + 20.0);
    assert_eq!(
        scanlines.0.vertices[4].pos.y - scanlines.0.vertices[0].pos.y,
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
    set_open(&context, true);
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
    set_open(&context, true);
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

fn plain_lines(text: &str) -> Vec<String> {
    text.lines().map(str::to_owned).collect()
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
