use super::*;
use eframe::egui::{Event, Modifiers, MouseWheelUnit, PointerButton, RawInput, Vec2};
use redox::domain::MessageStatus;

fn messages(count: usize) -> Vec<Message> {
    (0..count)
        .map(|i| Message {
            id: Uuid::new_v4(),
            role: Role::Assistant,
            content: format!("Message {i}"),
            status: MessageStatus::Complete,
            error: None,
            model: String::new(),
            attachments: vec![],
            activity: Default::default(),
        })
        .collect()
}

fn frame(context: &egui::Context, items: &[Message], width: f32, events: Vec<Event>) -> Vec<Uuid> {
    let mut painted = Vec::new();
    let _ = context.run(
        RawInput {
            screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(width, 600.0))),
            events,
            ..Default::default()
        },
        |context| {
            egui::CentralPanel::default().show(context, |ui| {
                show(ui, items, "test", 0.0, false, false, |ui, message| {
                    painted.push(message.id);
                    ui.label(&message.content);
                    if message.status == MessageStatus::Streaming {
                        ui.allocate_space(Vec2::new(30.0, 175.0));
                    }
                });
            });
        },
    );
    painted
}

fn layout(context: &egui::Context) -> Transcript {
    context.data(|data| {
        data.get_temp::<Vec<(Id, Transcript)>>(Id::new("transcript_layouts"))
            .unwrap()
            .pop()
            .unwrap()
            .1
    })
}

fn anchor(context: &egui::Context) -> Uuid {
    let layout = layout(context);
    layout.rows[layout.row_at(layout.last_offset.unwrap().0)].id
}

#[test]
fn long_transcripts_follow_the_last_reply_and_draw_only_visible_rows() {
    let context = egui::Context::default();
    let mut items = messages(1_000);
    let tail = items.last_mut().unwrap();
    tail.status = MessageStatus::Streaming;
    let tail_id = tail.id;

    for _ in 0..4 {
        let painted = frame(&context, &items, 750.0, vec![]);
        assert!(painted.contains(&tail_id));
        assert!(painted.len() < 25, "laid out {} rows", painted.len());
    }

    items.extend(messages(1));
    let painted = frame(&context, &items, 750.0, vec![]);
    assert!(painted.contains(&items.last().unwrap().id));
    assert!(layout(&context).last_offset.unwrap().1);
}

#[test]
fn scroll_anchor_survives_loading_older_history_and_resizing() {
    let context = egui::Context::default();
    let mut items = messages(120);
    for _ in 0..3 {
        frame(&context, &items, 750.0, vec![]);
    }
    frame(
        &context,
        &items,
        750.0,
        vec![
            Event::PointerMoved(pos2(500.0, 300.0)),
            Event::MouseWheel {
                unit: MouseWheelUnit::Point,
                delta: vec2(0.0, 730.0),
                modifiers: Modifiers::NONE,
            },
        ],
    );
    for _ in 0..60 {
        frame(&context, &items, 750.0, vec![]);
    }
    assert!(!layout(&context).last_offset.unwrap().1);
    let reading = anchor(&context);
    let mut older = messages(35);
    older.append(&mut items);
    frame(&context, &older, 750.0, vec![]);
    assert_eq!(anchor(&context), reading);
    frame(&context, &older, 620.0, vec![]);
    assert_eq!(anchor(&context), reading);

    let latest = older.last().unwrap().id;
    let pos = pos2(310.0, 574.0);
    for pressed in [true, false] {
        frame(
            &context,
            &older,
            620.0,
            vec![
                Event::PointerMoved(pos),
                Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::NONE,
                },
            ],
        );
    }
    let painted = frame(&context, &older, 620.0, vec![]);
    assert!(painted.contains(&latest));
    assert!(layout(&context).last_offset.unwrap().1);
}
