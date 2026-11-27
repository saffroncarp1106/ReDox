use super::*;

#[test]
fn unusable_saved_window_sizes_restore_to_default() {
    for size in [
        [237.0, 64.0],
        [1100.0, 64.0],
        [237.0, 900.0],
        [f32::NAN, 900.0],
    ] {
        let viewport =
            ReDox::validate_window_size(egui::ViewportBuilder::default().with_inner_size(size));
        assert_eq!(viewport.inner_size, Some(egui::vec2(1100.0, 900.0)));
    }
}

#[test]
fn valid_saved_window_sizes_and_maximized_state_are_preserved() {
    let viewport = ReDox::validate_window_size(
        egui::ViewportBuilder::default()
            .with_inner_size([1400.0, 1000.0])
            .with_maximized(true),
    );
    assert_eq!(viewport.inner_size, Some(egui::vec2(1400.0, 1000.0)));
    assert_eq!(viewport.maximized, Some(true));
}

#[cfg(windows)]
#[test]
fn a_half_screen_window_size_survives_restart() {
    let window = ReDox::validate_window_size(
        egui::ViewportBuilder::default().with_inner_size([680.0, 680.0]),
    );
    assert_eq!(window.inner_size, Some(egui::vec2(680.0, 680.0)));
}
#[cfg(windows)]
#[test]
fn terminal_fullscreen_suspends_and_restores_maximization() {
    use egui::{RawInput, ViewportCommand, ViewportId};

    let context = egui::Context::default();
    let mut input = RawInput::default();
    input
        .viewports
        .get_mut(&ViewportId::ROOT)
        .unwrap()
        .maximized = Some(true);
    let entered = context.run(input, |context| set_terminal_fullscreen(context, true));
    assert_eq!(
        entered.viewport_output[&ViewportId::ROOT].commands,
        [
            ViewportCommand::Maximized(false),
            ViewportCommand::Fullscreen(true)
        ]
    );
    let restored = context.run(RawInput::default(), |context| {
        set_terminal_fullscreen(context, false);
    });
    assert_eq!(
        restored.viewport_output[&ViewportId::ROOT].commands,
        [
            ViewportCommand::Fullscreen(false),
            ViewportCommand::Maximized(true)
        ]
    );
}
