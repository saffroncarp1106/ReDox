use super::*;

#[test]
fn desktop_text_families_cover_arrows_math_and_box_drawing() {
    let context = egui::Context::default();
    apply(&context, "codex");

    let _ = context.run(egui::RawInput::default(), |context| {
        context.fonts_mut(|fonts| {
            for id in [
                egui::FontId::proportional(14.0),
                egui::FontId::monospace(14.0),
                semibold(14.0),
                egui::FontId::new(14.0, egui::FontFamily::Name("serif".into())),
            ] {
                for glyph in "←→⇒⟶∈∉∇ℝ𝑥⟨⟩✓─│┌".chars() {
                    assert!(fonts.has_glyph(&id, glyph), "{} lacks {glyph}", id.family);
                }
            }
        });
    });
}
