use super::*;
use egui::{LayerId, RawInput, Shape, ViewportId, load::Bytes};
use std::borrow::Cow;

fn input(scale: f32) -> RawInput {
    let mut input = RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(128.0, 128.0))),
        ..RawInput::default()
    };
    input
        .viewports
        .get_mut(&ViewportId::ROOT)
        .unwrap()
        .native_pixels_per_point = Some(scale);
    input
}

#[test]
fn same_size_icons_share_a_texture_across_tints_and_positions() {
    for scale in [1.0, 1.25, 1.5, 2.0] {
        let context = egui::Context::default();
        egui_extras::install_image_loaders(&context);
        context.options_mut(|options| options.reduce_texture_memory = true);
        let colors = [Color32::from_gray(45), Color32::from_gray(220)];
        let draw = |ctx: &egui::Context| {
            let painter = ctx.layer_painter(LayerId::background());
            for (offset, color) in [0.2, 24.7].into_iter().zip(colors) {
                let rect = Rect::from_min_size(pos2(4.0 + offset, 3.3), vec2(18.0, 16.0));
                paint(&painter, rect, Icon::Edit, color);
            }
        };

        let first = context.run(input(scale), draw);
        let icons: Vec<_> = first
            .shapes
            .iter()
            .filter_map(|clipped| {
                if let Shape::Mesh(mesh) = &clipped.shape {
                    Some(mesh)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(icons.len(), 2);
        let id = icons[0].texture_id;
        let (texture, delta) = first
            .textures_delta
            .set
            .iter()
            .find(|(texture, _)| *texture == id)
            .expect("SVG should upload its rasterized texture");
        let pixels = (16.0 * scale) as usize;
        assert_eq!(delta.image.size(), [pixels, pixels]);
        assert_eq!(delta.options, TextureOptions::NEAREST);

        for (mesh, color) in icons.into_iter().zip(colors) {
            assert_eq!(mesh.texture_id, *texture);
            assert_eq!(mesh.vertices.len(), 4);
            for vertex in &mesh.vertices {
                assert_eq!(vertex.color, color);
                for coordinate in [vertex.pos.x, vertex.pos.y] {
                    let physical = coordinate * scale;
                    assert!((physical - physical.round()).abs() < 1e-4);
                }
            }
        }

        let next = context.run(input(scale), draw);
        assert!(next.textures_delta.set.iter().all(|(key, _)| *key != id));
    }
}

#[test]
fn bundled_svgs_rasterize_to_nonempty_transparent_images() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/ui/assets/icons");
    let context = egui::Context::default();
    egui_extras::install_image_loaders(&context);
    let mut ids = Vec::new();
    let output = context.run(input(1.0), |context| {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("svg") {
                continue;
            }
            let source = egui::ImageSource::Bytes {
                uri: Cow::Owned(format!(
                    "bytes://{}",
                    path.file_name().unwrap().to_string_lossy()
                )),
                bytes: Bytes::Shared(std::fs::read(&path).unwrap().into()),
            };
            let hint = SizeHint::Size {
                width: 24,
                height: 24,
                maintain_aspect_ratio: true,
            };
            let loaded = source
                .load(context, TextureOptions::NEAREST, hint)
                .unwrap_or_else(|error| panic!("Could not load {}: {error}", path.display()));
            let TexturePoll::Ready { texture } = loaded else {
                panic!("SVG rasterization delayed for {}", path.display());
            };
            ids.push((texture.id, path));
        }
    });
    assert!(!ids.is_empty());
    for (id, path) in ids {
        let (_, delta) = output
            .textures_delta
            .set
            .iter()
            .find(|(key, _)| *key == id)
            .unwrap();
        let egui::ImageData::Color(image) = &delta.image;
        assert!(
            image.pixels.iter().any(|pixel| pixel.a() >= 128),
            "{}",
            path.display()
        );
        assert!(
            image.pixels.iter().any(|pixel| pixel.a() == 0),
            "{}",
            path.display()
        );
    }
}
