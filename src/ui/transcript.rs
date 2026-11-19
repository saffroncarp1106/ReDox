use eframe::egui::{self, Align, Id, Layout, Rect, pos2, vec2};
use redox::domain::{Message, Role};
use uuid::Uuid;

const PADDING: f32 = 16.0;
const OVERSCAN: f32 = 48.0;
const MAX_CACHED_TRANSCRIPTS: usize = 8;

#[derive(Clone, Copy, Default, PartialEq)]
struct Geometry {
    width: f32,
    scale: f32,
    line_height: f32,
    gap: f32,
    show_activity: bool,
}

#[derive(Clone)]
struct Row {
    id: Uuid,
    end: f32,
}

#[derive(Clone, Default)]
struct Transcript {
    geometry: Geometry,
    rows: Vec<Row>,
    last_offset: Option<(f32, bool)>,
    jump_to_latest: bool,
}

impl Transcript {
    fn row_at(&self, offset: f32) -> usize {
        self.rows.partition_point(|row| row.end <= offset)
    }

    fn row_start(&self, index: usize) -> f32 {
        if index == 0 {
            PADDING
        } else {
            self.rows[index - 1].end
        }
    }

    fn anchor_offset(&self, index: usize, inset: f32) -> f32 {
        let start = self.row_start(index);
        start + inset.min((self.rows[index].end - start - 1.0).max(0.0))
    }

    fn height(&self) -> f32 {
        self.row_start(self.rows.len()) + PADDING
    }

    fn update(&mut self, messages: &[Message], geometry: Geometry) -> bool {
        let prefix = if self.geometry == geometry {
            self.rows
                .iter()
                .zip(messages)
                .take_while(|(row, message)| row.id == message.id)
                .count()
        } else {
            0
        };
        if self.geometry == geometry && prefix == self.rows.len() && prefix == messages.len() {
            return false;
        }

        let mut end = self.row_start(prefix);
        self.geometry = geometry;
        self.rows.truncate(prefix);
        self.rows.reserve(messages.len() - prefix);
        for message in &messages[prefix..] {
            let wrap_width = if message.role == Role::User {
                geometry.width * 0.72
            } else {
                geometry.width
            };
            let columns = (wrap_width / (geometry.line_height * 0.5)).max(1.0);
            let lines = (message.content.len() as f32 / columns).ceil().max(1.0);
            let images = message.content.matches("![").count()
                + message
                    .attachments
                    .iter()
                    .filter(|file| file.text.is_none())
                    .count();
            end += lines * geometry.line_height
                + geometry.gap
                + 28.0
                + if message.is_editable_prompt() { 30.0 } else { 0.0 }
                + images as f32 * super::media::MAX_IMAGE_HEIGHT;
            self.rows.push(Row {
                id: message.id,
                end,
            });
        }
        true
    }
}

pub(super) fn show(
    ui: &mut egui::Ui,
    messages: &[Message],
    scroll_id: impl std::hash::Hash,
    horizontal_padding: f32,
    show_activity: bool,
    follow_latest: bool,
    mut render: impl FnMut(&mut egui::Ui, &Message),
) {
    let view_id = ui
        .make_persistent_id(&scroll_id)
        .with(ui.ctx().viewport_id());
    let cache_id = Id::new("transcript_layouts");
    let mut cache = ui
        .data_mut(|data| data.remove_temp::<Vec<(Id, Transcript)>>(cache_id))
        .unwrap_or_default();
    let mut transcript = cache
        .iter()
        .position(|(id, _)| *id == view_id)
        .map(|index| cache.remove(index).1)
        .unwrap_or_default();

    let output = super::scroll::ScrollArea::vertical()
        .id_salt(scroll_id)
        .horizontal_scroll_offset(0.0)
        .stick_to_bottom(true)
        .animated(false)
        .auto_shrink([false, false])
        .show_viewport(ui, |ui, viewport| {
            let available = ui.available_rect_before_wrap();
            let width = (available.width() - horizontal_padding)
                .clamp(260.0, 820.0)
                .min(available.width());
            let geometry = Geometry {
                width,
                scale: ui.pixels_per_point(),
                line_height: ui.text_style_height(&egui::TextStyle::Body),
                gap: ui.spacing().item_spacing.y + 26.0,
                show_activity,
            };

            let offset = viewport.top();
            let followed = transcript
                .last_offset
                .map_or(offset <= 1.0, |(previous, followed)| {
                    followed && (offset - previous).abs() <= 1.0
                });
            let follow =
                std::mem::take(&mut transcript.jump_to_latest) || follow_latest || followed;
            let anchor_index = transcript.row_at(offset);
            let anchor = transcript
                .rows
                .get(anchor_index)
                .map(|row| (row.id, offset - transcript.row_start(anchor_index)));
            let changed = transcript.update(messages, geometry);
            let end_offset = |layout: &Transcript| (layout.height() - viewport.height()).max(0.0);
            let anchor = match (changed && !follow, anchor) {
                (true, Some((id, inset))) => transcript
                    .rows
                    .iter()
                    .position(|row| row.id == id)
                    .map(|index| (index, inset)),
                _ => None,
            };
            let reading_offset = |layout: &Transcript| {
                anchor.map_or(offset, |(index, inset)| layout.anchor_offset(index, inset))
            };
            let top = if follow {
                end_offset(&transcript)
            } else {
                reading_offset(&transcript)
            }
            .clamp(0.0, end_offset(&transcript));

            let first = transcript.row_at(top);
            let mut y = transcript.row_start(first);
            let mut old_end = y;
            let mut end = first;
            let left = available.center().x - width * 0.5;
            let right = left + width;
            let clip = ui.clip_rect();
            let row_clip = clip.intersect(Rect::from_min_max(
                pos2(left, clip.top()),
                pos2(right, clip.bottom()),
            ));

            for (message, row) in messages[first..].iter().zip(&mut transcript.rows[first..]) {
                if y >= top + viewport.height() + OVERSCAN {
                    break;
                }
                let row_top = available.top() + offset + y - top;
                let bounds = Rect::from_min_max(
                    pos2(left, row_top),
                    pos2(right, available.bottom().max(row_top)),
                );
                let mut child = ui.new_child(
                    egui::UiBuilder::new()
                        .id((view_id, message.id))
                        .max_rect(bounds)
                        .layout(Layout::top_down(Align::Min)),
                );
                child.set_clip_rect(row_clip);
                render(&mut child, message);
                y += child.min_rect().height() + geometry.gap;
                old_end = row.end;
                row.end = y;
                end += 1;
            }
            let shift = y - old_end;
            if shift != 0.0 {
                for row in &mut transcript.rows[end..] {
                    row.end += shift;
                }
            }

            ui.set_min_height(transcript.height());
            let target = if follow {
                end_offset(&transcript)
            } else {
                reading_offset(&transcript).clamp(0.0, end_offset(&transcript))
            };
            if (target - offset).abs() > 0.5 {
                ui.scroll_with_delta(vec2(0.0, offset - target));
            }
            top
        });

    let offset = output.state.offset.y;
    let bottom = (output.content_size.y - output.inner_rect.height()).max(0.0);
    let following = bottom - offset <= 1.0;
    transcript.last_offset = Some((offset, following));
    if (offset - output.inner).abs() > 1.0 {
        ui.ctx().request_repaint();
    }
    if bottom - offset > 48.0 {
        let rect = Rect::from_center_size(
            output.inner_rect.center_bottom() - vec2(0.0, 19.0),
            vec2(118.0, 26.0),
        );
        let mut overlay = ui.new_child(egui::UiBuilder::new().id_salt(view_id).max_rect(rect));
        if overlay
            .put(rect, egui::Button::new("Jump to latest"))
            .clicked()
        {
            transcript.jump_to_latest = true;
            ui.ctx().request_repaint();
        }
    }

    if cache.len() == MAX_CACHED_TRANSCRIPTS {
        cache.remove(0);
    }
    cache.push((view_id, transcript));
    ui.data_mut(|data| data.insert_temp(cache_id, cache));
}

#[cfg(test)]
#[path = "../../tests/ui/virtual_transcript.rs"]
mod tests;
