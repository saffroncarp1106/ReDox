use std::sync::Arc;

use eframe::egui::{
    self, Color32, Mesh, Painter, Pos2, Rect, Shape, Stroke, Ui,
    epaint::{Galley, TextShape, Vertex, WHITE_UV},
    pos2, vec2,
};
use serde::{Deserialize, Serialize};

const MAX_SPACING: u8 = 12;
const MAX_OFFSET: u8 = 8;
const SEGMENTS: u32 = 12;
const PHOSPHORS: [[u8; 3]; 3] = [[255, 45, 40], [40, 255, 72], [45, 105, 255]];

#[derive(Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default)]
pub(super) struct Settings {
    pub enabled: bool,
    pub scanline_strength: u8,
    pub scanline_spacing: u8,
    pub scanline_brightness: u8,
    pub scanline_bloom: u8,
    pub halo_strength: u8,
    pub rgb_offset: u8,
    pub curvature: u8,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            enabled: true,
            scanline_strength: 40,
            scanline_spacing: 3,
            scanline_brightness: 7,
            scanline_bloom: 10,
            halo_strength: 32,
            rgb_offset: 2,
            curvature: 85,
        }
    }
}

#[derive(Clone)]
struct Scanlines {
    settings: Settings,
    screen: Rect,
    pixels: f32,
    meshes: [Arc<Mesh>; 2],
}

impl Settings {
    pub(super) fn controls(&mut self, ui: &mut Ui) {
        ui.checkbox(&mut self.enabled, "CRT filter");
        ui.add_enabled_ui(self.enabled, |ui| {
            ui.add(
                egui::Slider::new(&mut self.scanline_strength, 0..=100)
                    .suffix("%")
                    .text("Scanline strength"),
            );
            ui.add(
                egui::Slider::new(&mut self.scanline_spacing, 2..=MAX_SPACING)
                    .suffix(" px")
                    .text("Scanline spacing"),
            );
            ui.add(
                egui::Slider::new(&mut self.scanline_brightness, 0..=100)
                    .suffix("%")
                    .text("RGB scanline brightness"),
            );
            ui.add(
                egui::Slider::new(&mut self.scanline_bloom, 0..=100)
                    .suffix("%")
                    .text("RGB scanline bloom"),
            );
            ui.add(
                egui::Slider::new(&mut self.halo_strength, 0..=100)
                    .suffix("%")
                    .text("RGB halo strength"),
            );
            ui.add(
                egui::Slider::new(&mut self.rgb_offset, 0..=MAX_OFFSET)
                    .suffix(" px")
                    .text("RGB separation"),
            );
            ui.add(
                egui::Slider::new(&mut self.curvature, 0..=100)
                    .suffix("%")
                    .text("Screen curvature"),
            );
        });
    }

    pub(super) fn paint_background(self, ui: &Ui, screen: Rect) {
        let screen = screen.intersect(ui.clip_rect());
        if !self.enabled || !screen.is_positive() {
            return;
        }
        let pixels = ui.ctx().pixels_per_point();
        let curvature = self.curvature(screen);
        let painter = ui.painter().with_clip_rect(screen);
        if self.scanline_strength > 0 || self.scanline_brightness > 0 || self.scanline_bloom > 0 {
            painter.rect_filled(screen, 0, Color32::from_gray(10));
            let id = ui.id().with("crt_scanlines");
            let cached = ui.data(|data| data.get_temp::<Scanlines>(id));
            let scanlines = cached
                .filter(|cache| {
                    cache.settings == self && cache.screen == screen && cache.pixels == pixels
                })
                .unwrap_or_else(|| {
                    let cache = Scanlines {
                        settings: self,
                        screen,
                        pixels,
                        meshes: self
                            .scanline_meshes(screen, pixels, curvature)
                            .map(Arc::new),
                    };
                    ui.data_mut(|data| data.insert_temp(id, cache.clone()));
                    cache
                });
            for mesh in scanlines.meshes {
                if !mesh.is_empty() {
                    painter.add(Shape::Mesh(mesh));
                }
            }
        }
        if let Some(curve) = curvature {
            curve.paint_rim(&painter);
        }
    }

    pub(super) fn draw_galley(
        self,
        painter: &Painter,
        origin: Pos2,
        galley: Arc<Galley>,
        curve: Option<Curvature>,
    ) {
        if galley.is_empty() {
            return;
        }
        let mut text = TextShape::new(origin, galley, Color32::WHITE);
        if let Some(curve) = curve {
            curve.bend_text(&mut text);
        }
        let shape = if self.enabled && self.halo_strength > 0 && self.rgb_offset > 0 {
            let separation =
                f32::from(self.rgb_offset.min(MAX_OFFSET)) / painter.ctx().pixels_per_point();
            let opacity = f32::from(self.halo_strength.min(100)) / 100.0;
            with_halo(text, separation, opacity)
        } else {
            Shape::Text(text)
        };
        painter.add(shape);
    }

    fn scanline_meshes(self, screen: Rect, pixels: f32, curve: Option<Curvature>) -> [Mesh; 2] {
        let mut lines = Mesh::default();
        let mut light = Mesh::default();
        let spacing = self.scanline_spacing.clamp(2, MAX_SPACING) as usize;
        let alpha = (u16::from(self.scanline_strength.min(100)) * 255 / 100) as u8;
        let brightness = f32::from(self.scanline_brightness.min(100)) / 100.0;
        let bloom = f32::from(self.scanline_bloom.min(100)) / 100.0;
        let spread = (1.0 + 3.0 * bloom) / pixels;
        let top = (screen.top() * pixels).ceil() as i32;
        let bottom = (screen.bottom() * pixels).floor() as i32;
        for (index, y) in (top..bottom).step_by(spacing).enumerate() {
            let line = Rect::from_min_max(
                pos2(screen.left(), y as f32 / pixels),
                pos2(screen.right(), (y + 1) as f32 / pixels),
            );
            if alpha > 0 {
                let ink = Color32::from_black_alpha(alpha);
                add_band(&mut lines, line, curve, ink, ink);
            }
            let phosphor = PHOSPHORS[index % PHOSPHORS.len()];
            if bloom > 0.0 {
                let glow = phosphor_light(phosphor, bloom * 0.16);
                let center = line.center().y;
                let upper = Rect::from_min_max(
                    pos2(line.left(), center - spread),
                    pos2(line.right(), center),
                )
                .intersect(screen);
                let lower = Rect::from_min_max(
                    pos2(line.left(), center),
                    pos2(line.right(), center + spread),
                )
                .intersect(screen);
                add_band(&mut light, upper, curve, Color32::TRANSPARENT, glow);
                add_band(&mut light, lower, curve, glow, Color32::TRANSPARENT);
            }
            if brightness > 0.0 {
                let core = phosphor_light(phosphor, brightness * 0.32);
                add_band(&mut light, line, curve, core, core);
            }
        }
        [lines, light]
    }

    pub(super) fn curvature(self, screen: Rect) -> Option<Curvature> {
        (self.enabled && self.curvature > 0 && screen.is_positive()).then(|| {
            let strength = f32::from(self.curvature.min(100)) / 100.0;
            Curvature {
                screen,
                inset: (screen.size().min_elem() * 0.03).min(7.0) * strength,
            }
        })
    }
}

#[derive(Clone, Copy)]
pub(super) struct Curvature {
    screen: Rect,
    inset: f32,
}

impl Curvature {
    fn bend(self, position: Pos2) -> Pos2 {
        let center = self.screen.center();
        let x = ((position.x - center.x) / (self.screen.width() * 0.5)).clamp(-1.0, 1.0);
        let y = ((position.y - center.y) / (self.screen.height() * 0.5)).clamp(-1.0, 1.0);
        position - self.inset * vec2(x * y * y, y * x * x)
    }

    fn bend_text(self, text: &mut TextShape) {
        let galley = Arc::make_mut(&mut text.galley);
        galley.mesh_bounds = Rect::NOTHING;
        for placed in &mut galley.rows {
            let origin = text.pos.to_vec2() + placed.pos.to_vec2();
            let row = Arc::make_mut(&mut placed.row);
            for vertex in &mut row.visuals.mesh.vertices {
                vertex.pos = self.bend(vertex.pos + origin) - origin;
            }
            row.visuals.mesh_bounds = row.visuals.mesh.calc_bounds();
            galley.mesh_bounds = galley
                .mesh_bounds
                .union(row.visuals.mesh_bounds.translate(placed.pos.to_vec2()));
        }
    }

    fn paint_rim(self, painter: &Painter) {
        let mut rim = Mesh::default();
        let mut edge = Vec::with_capacity((SEGMENTS * 4) as usize);
        for (start, end) in [
            (self.screen.left_top(), self.screen.right_top()),
            (self.screen.right_top(), self.screen.right_bottom()),
            (self.screen.right_bottom(), self.screen.left_bottom()),
            (self.screen.left_bottom(), self.screen.left_top()),
        ] {
            let mut previous = start;
            for segment in 1..=SEGMENTS {
                let current = start.lerp(end, segment as f32 / SEGMENTS as f32);
                let inside = self.bend(current);
                add_quad(
                    &mut rim,
                    [previous, current, inside, self.bend(previous)],
                    [Color32::from_gray(12); 4],
                );
                edge.push(inside);
                previous = current;
            }
        }
        painter.add(Shape::mesh(rim));
        painter.add(Shape::closed_line(
            edge,
            Stroke::new(1.0, Color32::from_gray(35)),
        ));
    }
}

fn phosphor_light(rgb: [u8; 3], intensity: f32) -> Color32 {
    let [r, g, b] = rgb.map(|channel| (f32::from(channel) * intensity).round() as u8);
    Color32::from_rgba_premultiplied(r, g, b, 0)
}

fn add_band(mesh: &mut Mesh, band: Rect, curve: Option<Curvature>, top: Color32, bottom: Color32) {
    let segments = if curve.is_some() { SEGMENTS } else { 1 };
    let mut left = band.left();
    for segment in 1..=segments {
        let right = band.left() + band.width() * segment as f32 / segments as f32;
        let corners = [
            pos2(left, band.top()),
            pos2(right, band.top()),
            pos2(right, band.bottom()),
            pos2(left, band.bottom()),
        ];
        let corners = if let Some(curve) = curve {
            corners.map(|point| curve.bend(point))
        } else {
            corners
        };
        add_quad(mesh, corners, [top, top, bottom, bottom]);
        left = right;
    }
}

fn add_quad(mesh: &mut Mesh, corners: [Pos2; 4], colors: [Color32; 4]) {
    let start = mesh.vertices.len() as u32;
    mesh.vertices
        .extend(corners.into_iter().zip(colors).map(|(pos, color)| Vertex {
            pos,
            uv: WHITE_UV,
            color,
        }));
    mesh.add_triangle(start, start + 1, start + 2);
    mesh.add_triangle(start, start + 2, start + 3);
}

fn with_halo(text: TextShape, separation: f32, opacity: f32) -> Shape {
    let mut layers = Vec::with_capacity(7);
    for (spread, strength) in [(1.8, 0.22), (1.0, 0.75)] {
        let shift = separation * spread;
        for (offset, [r, g, b]) in [
            vec2(-shift, 0.0),
            vec2(0.0, -shift * 0.65),
            vec2(shift, 0.0),
        ]
        .into_iter()
        .zip(PHOSPHORS)
        {
            let mut ghost = text
                .clone()
                .with_override_text_color(Color32::from_rgb(r, g, b));
            ghost.pos += offset;
            ghost.opacity_factor *= opacity * strength;
            layers.push(Shape::Text(ghost));
        }
    }
    layers.push(Shape::Text(text));
    Shape::Vec(layers)
}
