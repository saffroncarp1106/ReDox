use super::*;
use redox::domain::{Message, Role};

const ANSWER: &str = "\
### Rendered response\n\n\
The results suggest that the caller can reuse the existing layout without making additional changes. The default path still handles missing files and retains the visible transcript.\n\n\
1. Open the workspace and verify the selected folder.\n\
2. Save the configuration and check the messages.\n\n\
```rust\nfn example(items: &[String]) -> usize {\n    items.iter().map(String::len).sum()\n}\n```\n\n";

#[test]
#[ignore = "offscreen release benchmark with synthetic messages"]
fn long_chat_benchmark() {
    let turns = std::env::var("REDOX_BENCH_TURNS")
        .ok()
        .map_or(150, |value| value.parse().unwrap());
    let context = egui::Context::default();
    let mut app = ReDox::test_preview(&context, "codex");
    app.prepare_test_scene(true);
    app.ui_preferences.hide_update_prompts = true;
    let chat = app.session.active_mut().unwrap();
    chat.messages.clear();
    let answer = ANSWER.repeat(12);
    for turn in 0..turns {
        for (role, content) in [
            (Role::User, format!("Can you review change {turn}?")),
            (Role::Assistant, format!("{answer}Result for turn {turn}.")),
        ] {
            chat.messages.push(Message {
                id: uuid::Uuid::new_v4(),
                role,
                content,
                status: MessageStatus::Complete,
                error: None,
                model: String::new(),
                attachments: Vec::new(),
                activity: Default::default(),
            });
        }
    }

    let mut times = Vec::new();
    for frame in 0..90 {
        let last = app
            .session
            .active_mut()
            .unwrap()
            .messages
            .last_mut()
            .unwrap();
        last.status = MessageStatus::Streaming;
        last.content.push_str(" More detail.");
        let start = Instant::now();
        let output = context.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1100.0, 900.0),
                )),
                time: Some(frame as f64 / 60.0),
                ..Default::default()
            },
            |context| app.render_test_frame(context),
        );
        std::hint::black_box(output);
        if frame >= 20 {
            times.push(start.elapsed().as_secs_f64() * 1_000.0);
        }
    }
    times.sort_by(f64::total_cmp);
    println!(
        "LONG_CHAT {turns} turns median_ms {:.3} p95_ms {:.3}",
        times[times.len() / 2],
        times[(times.len() * 95 / 100) - 1]
    );
}

#[test]
#[ignore = "release benchmark with synthetic UI packs"]
fn theme_catalog_benchmark() {
    let root = std::path::PathBuf::from(std::env::var_os("REDOX_BENCH_PACKS").unwrap());
    let mut times = Vec::new();
    for _ in 0..12 {
        let start = Instant::now();
        let catalog = std::hint::black_box(ui::packs::discover(&root));
        assert!(catalog.errors.is_empty(), "{:?}", catalog.errors);
        assert!(
            catalog
                .packs
                .iter()
                .any(|pack| pack.id == "large-background")
        );
        times.push(start.elapsed().as_secs_f64() * 1_000.0);
    }
    times.sort_by(f64::total_cmp);
    println!("THEME_SCAN median_ms {:.3}", times[times.len() / 2]);
}

#[test]
#[ignore = "offscreen release memory benchmark with synthetic images"]
fn image_chat_benchmark() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("temp/verification")
        .join(format!("image-chat-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let first = root.join("0.png");
    image::RgbaImage::from_pixel(1024, 1024, image::Rgba([16, 48, 96, 255]))
        .save(&first)
        .unwrap();

    let context = egui::Context::default();
    let mut app = ReDox::test_preview(&context, "codex");
    app.prepare_test_scene(true);
    app.ui_preferences.hide_update_prompts = true;
    let chat = app.session.active_mut().unwrap();
    chat.messages.clear();
    for index in 0..24 {
        let path = root.join(format!("{index}.png"));
        if index != 0 {
            std::fs::copy(&first, &path).unwrap();
        }
        chat.messages.push(Message {
            id: uuid::Uuid::new_v4(),
            role: Role::Assistant,
            content: format!("![Image {index}](<{}>)", path.display()),
            status: MessageStatus::Complete,
            error: None,
            model: String::new(),
            attachments: Vec::new(),
            activity: Default::default(),
        });
    }

    for frame in 0..64 {
        let output = context.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1100.0, 900.0),
                )),
                time: Some(frame as f64 / 60.0),
                ..Default::default()
            },
            |context| app.render_test_frame(context),
        );
        std::hint::black_box(output);
        std::thread::sleep(Duration::from_millis(8));
    }
    let manager = context.tex_manager();
    let images = manager
        .read()
        .allocated()
        .filter(|(_, meta)| meta.size == [1024, 1024])
        .count();
    assert!(images > 0, "at least one image was decoded and uploaded");
    assert!(!context.has_pending_images());
    println!("IMAGE_CHAT {images} of 24 image textures allocated");
    std::fs::remove_dir_all(root).unwrap();
}
