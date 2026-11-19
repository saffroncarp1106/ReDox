#[cfg(windows)]
pub fn response_complete() {
    std::thread::spawn(|| {
        if let Err(error) = show_response_complete() {
            eprintln!("Could not show the response notification: {error}");
        }
    });
}

#[cfg(not(windows))]
pub fn response_complete() {}

#[cfg(windows)]
fn show_response_complete() -> Result<(), String> {
    use std::sync::OnceLock;
    use winrt_toast_reborn::{Toast, ToastManager};

    static REGISTRATION: OnceLock<Result<(), String>> = OnceLock::new();
    REGISTRATION
        .get_or_init(|| {
            let directory = dirs::data_local_dir()
                .ok_or("Local application data directory is unavailable.")?
                .join("ReDox");
            std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
            let icon = directory.join("notification.ico");
            std::fs::write(&icon, include_bytes!("../icon.ico"))
                .map_err(|error| error.to_string())?;
            winrt_toast_reborn::register("ReDox", "ReDox", Some(&icon))
                .map_err(|error| error.to_string())
        })
        .as_ref()
        .map_err(Clone::clone)?;

    ToastManager::new("ReDox")
        .show(&Toast::new().text1("ReDox has a response."))
        .map_err(|error| error.to_string())
}
