#[allow(dead_code)]
mod app;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let [theme, scene, width, height, output, tail @ ..] = args.as_slice() else {
        return Err("Usage: redox-studio-native-preview <theme> <scene> <width> <height> <absolute-output.redoxskin> [appearance.json]".into());
    };
    let appearance = match tail {
        [] => Default::default(),
        [path] => {
            redox_studio::native_appearance::NativeAppearance::load(std::path::Path::new(path))?
        }
        _ => return Err("Preview accepts at most one appearance file".into()),
    };
    app::studio_native_preview(
        theme.to_str().ok_or("Theme must be UTF-8")?,
        scene.to_str().ok_or("Scene must be UTF-8")?,
        width.to_str().ok_or("Invalid preview width")?.parse()?,
        height.to_str().ok_or("Invalid preview height")?.parse()?,
        std::path::Path::new(output),
        appearance,
    )
}
