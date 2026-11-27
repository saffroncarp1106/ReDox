use std::io::{self, Read, Seek, Write};
use std::path::{Path, PathBuf};

use cap_std::ambient_authority;
use cap_std::fs::{Dir, File, Metadata, OpenOptions};
use serde_json::{Value, json};

use super::tools::definition;

const FILE_LIMIT: usize = 128 * 1024;

pub(super) fn open_folder(path: &Path) -> Result<Dir, String> {
    #[cfg(windows)]
    {
        use std::path::{Component, Prefix};
        use windows_sys::Win32::Storage::FileSystem::GetDriveTypeW;
        use windows_sys::Win32::System::WindowsProgramming::{
            DRIVE_FIXED, DRIVE_RAMDISK, DRIVE_REMOVABLE,
        };

        let drive = match path.components().next() {
            Some(Component::Prefix(prefix)) => match prefix.kind() {
                Prefix::VerbatimDisk(drive) => Some(drive),
                _ => None,
            },
            _ => None,
        }
        .ok_or("Locked SaffyOS needs a local drive folder.")?;
        let root = [u16::from(drive), u16::from(b':'), u16::from(b'\\'), 0];
        // GetDriveTypeW reads the NUL-terminated drive root.
        let drive_type = unsafe { GetDriveTypeW(root.as_ptr()) };
        if !matches!(drive_type, DRIVE_FIXED | DRIVE_RAMDISK | DRIVE_REMOVABLE) {
            return Err("Locked SaffyOS needs a local drive folder.".into());
        }
    }
    let canonical = path
        .canonicalize()
        .map_err(|error| format!("Cannot resolve the locked SaffyOS folder: {error}"))?;
    if canonical != path {
        return Err("The locked SaffyOS folder moved or resolves to a different location.".into());
    }
    if super::saffyos_python::runtime_folder()
        .is_some_and(|runtime| runtime.starts_with(path) || path.starts_with(&runtime))
    {
        return Err("Choose a SaffyOS folder separate from the Python runtime directory.".into());
    }
    Dir::open_ambient_dir(path, ambient_authority())
        .map_err(|error| format!("Cannot open the locked SaffyOS folder: {error}"))
}

pub(super) fn definitions() -> Value {
    json!([
        definition(
            "run_python",
            "Run a workspace Python script in an offline sandbox and report its exit code, stdout, and stderr. Use inputs to include any other workspace files it reads. The working directory is the snapshot root and file changes are discarded. Standard-library Python only with a 15-second execution limit.",
            &["path"],
            json!({
                "path":{"type":"string"},
                "inputs":{"type":"array","items":{"type":"string"},"maxItems":16},
                "args":{"type":"array","items":{"type":"string"},"maxItems":16},
                "stdin":{"type":"string"}
            }),
        ),
        definition(
            "list_files",
            "List file and directory names in the locked workspace. Omit path or use '.' for the root, or use a relative directory path such as Pictures.",
            &[],
            json!({"path":{"type":"string"}}),
        ),
        definition(
            "make_directory",
            "Create a directory and any missing parent directories. Use a workspace-relative path such as Pictures/Experiments.",
            &["path"],
            json!({"path":{"type":"string"}}),
        ),
        definition(
            "create_file",
            "Create a new UTF-8 text file without overwriting anything. Use a workspace-relative name such as Pictures/experiment.py. Missing parent directories are created automatically. Use write_file to revise an existing file instead.",
            &["name", "content"],
            json!({"name":{"type":"string"},"content":{"type":"string"}}),
        ),
        definition(
            "read_file",
            "Read a UTF-8 text file up to 128 KiB. Use a workspace-relative path such as Pictures/notes.md.",
            &["path"],
            json!({"path":{"type":"string"}}),
        ),
        definition(
            "write_file",
            "Replace the contents of an existing UTF-8 text file up to 128 KiB. Use a workspace-relative path. To create a new file, use create_file.",
            &["path", "content"],
            json!({"path":{"type":"string"},"content":{"type":"string"}}),
        ),
    ])
}

fn valid_name(name: &str) -> bool {
    if name.is_empty()
        || name.ends_with(['.', ' '])
        || !name
            .chars()
            .all(|character| !character.is_control() && !"<>:\"/\\|?*".contains(character))
    {
        return false;
    }
    let stem = name
        .split('.')
        .next()
        .unwrap_or_default()
        .trim_end_matches(' ')
        .to_ascii_uppercase();
    !matches!(
        stem.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
    ) && !["COM", "LPT"].iter().any(|prefix| {
        stem.strip_prefix(prefix).is_some_and(|suffix| {
            let mut characters = suffix.chars();
            matches!(characters.next(), Some('0'..='9' | '¹' | '²' | '³'))
                && characters.next().is_none()
        })
    })
}

pub(super) fn relative_path(path: &str) -> Result<PathBuf, String> {
    let mut relative = PathBuf::new();
    for part in path.split(['/', '\\']) {
        if !valid_name(part) {
            return Err(
                "Use a workspace-relative path without empty, '.', '..', or reserved components."
                    .into(),
            );
        }
        relative.push(part);
    }
    Ok(relative)
}

fn is_link(metadata: &Metadata) -> bool {
    #[cfg(windows)]
    {
        use cap_std::fs::MetadataExt;
        use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;
        metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    }
    #[cfg(not(windows))]
    {
        metadata.is_symlink()
    }
}

fn open_subdirectory(folder: &Dir, name: &Path) -> io::Result<Dir> {
    #[cfg(windows)]
    {
        use cap_std::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::{
            FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ,
            FILE_SHARE_WRITE,
        };
        let mut options = OpenOptions::new();
        options
            .read(true)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE);
        let file = folder.open_with(name, &options)?;
        let metadata = file.metadata()?;
        if metadata.is_dir() && !is_link(&metadata) {
            return Ok(Dir::from_std_file(file.into_std()));
        }
    }
    #[cfg(not(windows))]
    {
        let metadata = folder.symlink_metadata(name)?;
        if metadata.is_dir() && !is_link(&metadata) {
            return folder.open_dir(name);
        }
    }
    Err(io::Error::new(
        io::ErrorKind::InvalidInput,
        "Only ordinary directories are allowed, not links or junctions.",
    ))
}

fn open_workspace_path(folder: &Dir, path: &Path, create: bool) -> Result<Vec<Dir>, String> {
    let mut directories = Vec::new();
    for name in path.iter().map(Path::new) {
        let parent = directories.last().unwrap_or(folder);
        let dir = match open_subdirectory(parent, name) {
            Err(error) if create && error.kind() == io::ErrorKind::NotFound => {
                match parent.create_dir(name) {
                    Ok(()) => (),
                    Err(error) if error.kind() == io::ErrorKind::AlreadyExists => (),
                    Err(error) => {
                        return Err(format!(
                            "Cannot create directory {}: {error}",
                            name.display()
                        ));
                    }
                }
                open_subdirectory(parent, name)
            }
            result => result,
        }
        .map_err(|error| format!("Cannot open directory {}: {error}", name.display()))?;
        directories.push(dir);
    }
    Ok(directories)
}

fn single_link(file: &File, metadata: &Metadata) -> io::Result<bool> {
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::Storage::FileSystem::{
            BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle,
        };
        let _ = metadata;
        let mut info = BY_HANDLE_FILE_INFORMATION::default();
        if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(info.nNumberOfLinks == 1)
    }
    #[cfg(unix)]
    {
        use cap_std::fs::MetadataExt;
        let _ = file;
        Ok(metadata.nlink() == 1)
    }
}

fn read_text(file: &mut File) -> Result<String, String> {
    let mut bytes = Vec::new();
    file.take(FILE_LIMIT as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() > FILE_LIMIT {
        return Err("File content exceeds 128 KiB.".into());
    }
    String::from_utf8(bytes).map_err(|_| "Only UTF-8 text files are supported.".into())
}

fn path_argument(value: &Value) -> Result<&str, String> {
    value
        .as_str()
        .ok_or_else(|| "Provide a workspace-relative path.".into())
}

pub(super) fn stage_input(
    folder: &Dir,
    path: &Path,
    stage: &Path,
    remaining: &mut u64,
) -> Result<(), String> {
    let directories = open_workspace_path(folder, path.parent().unwrap(), false)?;
    let folder = directories.last().unwrap_or(folder);
    let filename = path.file_name().unwrap();
    let failure = |error| format!("Cannot stage {}: {error}", path.display());
    let metadata = folder.symlink_metadata(filename).map_err(failure)?;
    if !metadata.is_file() || is_link(&metadata) {
        return Err(format!("{} is not a regular input file.", path.display()));
    }
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use cap_std::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT;
        options
            .share_mode(0)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    }
    let mut file = folder.open_with(filename, &options).map_err(failure)?;
    let metadata = file.metadata().map_err(failure)?;
    if !metadata.is_file()
        || is_link(&metadata)
        || !single_link(&file, &metadata).map_err(failure)?
    {
        return Err(format!(
            "{} is not a regular input file without links.",
            path.display()
        ));
    }
    let limit_error = "Python inputs, including the script, may total up to 32 MiB.";
    if metadata.len() > *remaining {
        return Err(limit_error.into());
    }
    let destination = stage.join(path);
    std::fs::create_dir_all(destination.parent().unwrap()).map_err(failure)?;
    let mut output = std::fs::File::create(destination).map_err(failure)?;
    let bytes = io::copy(&mut (&mut file).take(*remaining + 1), &mut output).map_err(failure)?;
    *remaining = remaining.checked_sub(bytes).ok_or(limit_error)?;
    Ok(())
}

pub(super) fn execute(folder: &Dir, name: &str, arguments: &Value) -> Result<String, String> {
    let parsed;
    let arguments = if let Some(text) = arguments.as_str() {
        parsed = serde_json::from_str(text).map_err(|_| "Use a JSON object for tool arguments.")?;
        &parsed
    } else {
        arguments
    };
    match name {
        "list_files" => {
            let path = match arguments.get("path") {
                None => PathBuf::new(),
                Some(value) if value.as_str() == Some(".") => PathBuf::new(),
                Some(value) => relative_path(path_argument(value)?)?,
            };
            let directories = open_workspace_path(folder, &path, false)?;
            let folder = directories.last().unwrap_or(folder);
            let mut files = Vec::new();
            let mut subdirectories = Vec::new();
            for entry in folder
                .entries()
                .map_err(|error| error.to_string())?
                .take(256)
            {
                let entry = entry.map_err(|error| error.to_string())?;
                let filename = entry.file_name();
                if let Some(name) = filename.to_str().filter(|name| valid_name(name)) {
                    let metadata = folder
                        .symlink_metadata(name)
                        .map_err(|error| error.to_string())?;
                    if !is_link(&metadata) {
                        files.push(name.to_owned());
                        if metadata.is_dir() {
                            subdirectories.push(name.to_owned());
                        }
                    }
                }
            }
            files.sort_unstable();
            subdirectories.sort_unstable();
            Ok(json!({"names":files,"directories":subdirectories,"scan_limit":256}).to_string())
        }
        "make_directory" => {
            let path = relative_path(path_argument(&arguments["path"])?)?;
            open_workspace_path(folder, &path, true)?;
            Ok(format!("Directory ready: {}.", path.display()))
        }
        "create_file" | "read_file" | "write_file" => {
            let create = name == "create_file";
            let read = name == "read_file";
            let path = relative_path(path_argument(
                &arguments[if create { "name" } else { "path" }],
            )?)?;
            let content = if read {
                ""
            } else {
                arguments["content"]
                    .as_str()
                    .ok_or("Provide text content.")?
            };
            if content.len() > FILE_LIMIT {
                return Err("File content exceeds 128 KiB.".into());
            }
            let filename = path.file_name().unwrap();
            let directories = open_workspace_path(folder, path.parent().unwrap(), create)?;
            let folder = directories.last().unwrap_or(folder);
            let display = path.display();
            let file_error = |error| format!("Cannot access {display}: {error}");
            let regular_file_error =
                "Only regular files without symlinks or hard links are allowed.";
            if !create {
                let metadata = folder.symlink_metadata(filename).map_err(file_error)?;
                if !metadata.is_file() || is_link(&metadata) {
                    return Err(regular_file_error.into());
                }
            }
            let mut options = OpenOptions::new();
            options.read(!create).write(!read).create_new(create);
            #[cfg(windows)]
            {
                use cap_std::fs::OpenOptionsExt;
                use windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT;
                options.share_mode(0);
                if !create {
                    options.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
                }
            }
            let mut file = folder.open_with(filename, &options).map_err(|error| {
                if create && error.kind() == io::ErrorKind::AlreadyExists {
                    format!(
                        "{display} already exists. To revise it, use read_file, then write_file on the same path."
                    )
                } else {
                    file_error(error)
                }
            })?;
            if !create {
                let metadata = file.metadata().map_err(file_error)?;
                if !metadata.is_file()
                    || is_link(&metadata)
                    || !single_link(&file, &metadata).map_err(file_error)?
                {
                    return Err(regular_file_error.into());
                }
                let current = read_text(&mut file)?;
                if read {
                    return Ok(current);
                }
                file.rewind().map_err(file_error)?;
            }
            file.write_all(content.as_bytes()).map_err(|error| {
                format!("Writing {display} failed and may have left a partial file: {error}")
            })?;
            if !create {
                file.set_len(content.len() as u64).map_err(file_error)?;
            }
            let action = if create { "Created" } else { "Updated" };
            Ok(format!("{action} {display} ({} bytes).", content.len()))
        }
        _ => Err("This tool is unavailable in locked SaffyOS.".into()),
    }
}
