use super::*;

#[test]
fn codex_is_registered_as_the_legacy_layout() {
    let renderer = resolve("codex");
    assert_eq!(renderer.id, "codex");
    assert!(std::ptr::fn_addr_eq(
        renderer.rail,
        ReDox::navigation_rail as fn(&mut ReDox, &egui::Context)
    ));
    assert!(std::ptr::fn_addr_eq(
        renderer.conversation,
        ReDox::conversation as fn(&mut ReDox, &egui::Context)
    ));
}

#[test]
fn unknown_layout_uses_default_renderer() {
    assert_eq!(resolve("my-new-ui").id, "win98");
    assert_eq!(resolve("").id, "win98");
    assert_eq!(resolve("claude").id, "win98");
    assert_eq!(resolve("gemini").id, "win98");
}

#[test]
fn windows_98_is_the_first_builtin() {
    assert_eq!(RENDERERS[0].id, super::super::win98::ID);
}

#[test]
fn embedded_configs_have_unique_ids_and_matching_renderers() {
    let mut ids = std::collections::BTreeSet::new();
    for renderer in RENDERERS {
        assert!(ids.insert(renderer.id));
        let config: super::super::packs::Config = serde_json::from_str(renderer.config).unwrap();
        assert_eq!(config.layout, renderer.id);
        config.validate(std::path::Path::new("ui.json")).unwrap();
    }
}
