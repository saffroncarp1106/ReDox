use super::super::Preferences;

#[test]
fn discord_preferences_default_off_and_preserve_custom_text() {
    let old: Preferences = serde_json::from_str("{}").unwrap();
    assert!(!old.discord_presence.enabled);
    let mut preferences = old;
    preferences.discord_presence.enabled = true;
    preferences.discord_presence.details = "Building ReDox".into();
    preferences.discord_presence.state = "Enjoying life".into();
    let restored: Preferences =
        serde_json::from_str(&serde_json::to_string(&preferences).unwrap()).unwrap();
    assert!(restored.discord_presence.enabled);
    assert_eq!(restored.discord_presence.details, "Building ReDox");
    assert_eq!(restored.discord_presence.state, "Enjoying life");
}

#[test]
fn windows_98_is_the_default_and_retired_skins_fall_back() {
    let preferences = Preferences::default();
    assert_eq!(preferences.ui, "win98");

    for retired in ["claude", "gemini"] {
        let mut saved: Preferences =
            serde_json::from_str(&format!(r#"{{"ui":"{retired}"}}"#)).unwrap();
        saved.normalize();
        assert_eq!(saved.ui, "win98");
    }

    let mut dark: Preferences = serde_json::from_str(r#"{"ui":"win98-dark"}"#).unwrap();
    dark.normalize();
    assert_eq!(dark.ui, "win98-dark");
}
