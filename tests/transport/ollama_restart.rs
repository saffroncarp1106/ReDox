use super::supported;

#[test]
fn process_restart_is_limited_to_the_direct_windows_endpoint() {
    for endpoint in [
        "https://127.0.0.1:11434",
        "http://127.0.0.1:11435",
        "http://127.0.0.1:11434/proxy",
        "http://192.168.1.5:11434",
    ] {
        assert!(!supported(endpoint), "{endpoint}");
    }
    assert_eq!(supported("http://127.0.0.1:11434"), cfg!(windows));
    assert_eq!(supported("http://[::1]:11434"), cfg!(windows));
}
