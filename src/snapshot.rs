use crate::{model::*, platform};
use sha2::{Digest, Sha256};
#[cfg(unix)]
use std::os::{
    fd::{AsRawFd, FromRawFd},
    unix::fs::{MetadataExt, OpenOptionsExt},
};
use std::{
    collections::HashSet,
    fs::{self, OpenOptions},
    io::Read,
    path::{Path, PathBuf},
};

pub fn relative_parts(path: &str) -> Outcome<Vec<&str>> {
    let parts: Vec<_> = path.split('/').collect();
    let forbidden = |part: &&str| {
        let p = part.to_lowercase();
        [
            ".git",
            ".gemini",
            ".codex",
            ".claude",
            ".ssh",
            ".aws",
            ".npmrc",
            "node_modules",
            "id_rsa",
            "id_ed25519",
            "credentials",
        ]
        .contains(&p.as_str())
            || p == ".env"
            || p.starts_with(".env.")
            || p.starts_with("credentials.")
            || [".pem", ".key", ".p12", ".pfx"]
                .iter()
                .any(|x| p.ends_with(x))
    };
    if path.is_empty()
        || Path::new(path).is_absolute()
        || path.contains('\\')
        || path.contains(':')
        || path.chars().any(char::is_control)
        || parts
            .iter()
            .any(|p| p.is_empty() || *p == "." || *p == ".." || forbidden(p))
    {
        return Err(Failure::new(
            "INPUT_SCOPE_INVALID",
            "Input paths must be relative regular files without excluded credential or configuration paths.",
        ));
    }
    Ok(parts)
}
pub fn allowed_root(config: &Config, root: Option<&Path>) -> Outcome<PathBuf> {
    let root = root.filter(|p| p.is_absolute()).ok_or_else(|| {
        Failure::new(
            "INPUT_SCOPE_INVALID",
            "The working directory must be absolute.",
        )
    })?;
    let canonical = fs::canonicalize(root)
        .map_err(|_| Failure::new("INPUT_SCOPE_INVALID", "Cannot resolve the selected root."))?;
    if !canonical.is_dir() {
        return Err(Failure::new(
            "INPUT_SCOPE_INVALID",
            "The selected working directory is not a directory.",
        ));
    }
    if !config.allowed_roots.is_empty() && !config.allowed_roots.contains(&canonical) {
        return Err(Failure::new(
            "INPUT_SCOPE_INVALID",
            "The selected root is not allowed by the server configuration.",
        ));
    }
    Ok(canonical)
}
#[cfg(unix)]
fn open_scoped(root: &Path, parts: &[&str]) -> Outcome<fs::File> {
    let dir_flags = libc::O_DIRECTORY | libc::O_NOFOLLOW;
    let mut dir = OpenOptions::new()
        .read(true)
        .custom_flags(dir_flags)
        .open(root)
        .map_err(io_failure)?;
    for part in &parts[..parts.len() - 1] {
        dir = open_at(&dir, part, dir_flags)?;
    }
    open_at(
        &dir,
        parts[parts.len() - 1],
        libc::O_NOFOLLOW | libc::O_NONBLOCK,
    )
}
#[cfg(unix)]
fn open_at(dir: &fs::File, name: &str, flags: i32) -> Outcome<fs::File> {
    let name = std::ffi::CString::new(name).map_err(io_failure)?;
    let fd = unsafe {
        libc::openat(
            dir.as_raw_fd(),
            name.as_ptr(),
            flags | libc::O_RDONLY | libc::O_CLOEXEC,
        )
    };
    if fd < 0 {
        return Err(io_failure(std::io::Error::last_os_error()));
    }
    Ok(unsafe { fs::File::from_raw_fd(fd) })
}
#[cfg(windows)]
fn open_scoped(root: &Path, parts: &[&str]) -> Outcome<fs::File> {
    use std::os::windows::{fs::OpenOptionsExt, io::AsRawHandle};
    use windows_sys::Win32::Storage::FileSystem::*;
    let mut path = root.to_path_buf();
    for part in parts {
        path.push(part);
        if platform::is_reparse(&fs::symlink_metadata(&path).map_err(io_failure)?) {
            return Err(Failure::new(
                "INPUT_SCOPE_INVALID",
                "Input reparse points are not allowed.",
            ));
        }
    }
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
        .map_err(io_failure)?;
    let mut name = vec![0u16; 32768];
    let count = unsafe {
        GetFinalPathNameByHandleW(
            file.as_raw_handle(),
            name.as_mut_ptr(),
            name.len() as u32,
            FILE_NAME_NORMALIZED,
        )
    };
    if count == 0 || count as usize >= name.len() {
        return Err(io_failure("input handle path"));
    }
    let actual = PathBuf::from(String::from_utf16(&name[..count as usize]).map_err(io_failure)?);
    if !actual.starts_with(root) || platform::is_reparse(&file.metadata().map_err(io_failure)?) {
        return Err(Failure::new(
            "INPUT_SCOPE_INVALID",
            "Input handle is outside the allowed root.",
        ));
    }
    Ok(file)
}
fn single_link(file: &fs::File, meta: &fs::Metadata) -> Outcome<bool> {
    #[cfg(unix)]
    {
        let _ = file;
        Ok(meta.nlink() == 1)
    }
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::Storage::FileSystem::{
            BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle,
        };
        let _ = meta;
        let mut info = BY_HANDLE_FILE_INFORMATION::default();
        if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } == 0 {
            return Err(io_failure("input handle metadata"));
        }
        Ok(info.nNumberOfLinks == 1)
    }
}
fn read_scoped(root: &Path, parts: &[&str]) -> Outcome<Vec<u8>> {
    let file = open_scoped(root, parts)?;
    let before = file.metadata().map_err(io_failure)?;
    if !before.is_file() || !single_link(&file, &before)? || before.len() > 262144 {
        return Err(Failure::new(
            "INPUT_SCOPE_INVALID",
            "Inputs must be regular non-hardlinked text files of at most 256 KiB.",
        ));
    }
    let mut bytes = vec![];
    file.try_clone()
        .map_err(io_failure)?
        .take(262145)
        .read_to_end(&mut bytes)
        .map_err(io_failure)?;
    let after = file.metadata().map_err(io_failure)?;
    if bytes.len() > 262144 || !platform::unchanged(&before, &after) {
        return Err(Failure::new(
            "INPUT_CHANGED",
            "Input changed during capture or exceeded its size limit.",
        ));
    }
    if bytes.contains(&0) || std::str::from_utf8(&bytes).is_err() {
        return Err(Failure::new(
            "INPUT_FORMAT_INVALID",
            "Inputs must be UTF-8 text.",
        ));
    }
    Ok(bytes)
}
pub fn capture(
    config: &Config,
    input: &Submission,
    destination: &Path,
) -> Outcome<Vec<ManifestEntry>> {
    fs::create_dir(destination).map_err(io_failure)?;
    platform::private_mode(destination, 0o700)?;
    if input.files.is_empty() {
        if input.root.is_some() {
            allowed_root(config, input.root.as_deref())?;
        }
        return Ok(vec![]);
    }
    let root = allowed_root(config, input.root.as_deref())?;
    let mut paths = HashSet::new();
    let mut total = 0;
    let mut manifest = vec![];
    for path in &input.files {
        if !paths.insert(path) {
            return Err(Failure::new(
                "INPUT_SCOPE_INVALID",
                "Duplicate input files are not allowed.",
            ));
        }
        let parts = relative_parts(path)?;
        let bytes = read_scoped(&root, &parts).map_err(|e| {
            if e.code == "IO_FAILED" {
                Failure::new(
                    "INPUT_SCOPE_INVALID",
                    "Cannot securely capture a selected file.",
                )
            } else {
                e
            }
        })?;
        total += bytes.len();
        if total > 4194304 {
            return Err(Failure::new(
                "INPUT_TOO_LARGE",
                "Selected inputs exceed 4 MiB.",
            ));
        }
        let output = destination.join(path);
        fs::create_dir_all(output.parent().unwrap()).map_err(io_failure)?;
        let mut options = OpenOptions::new();
        options.create_new(true).write(true);
        #[cfg(unix)]
        options.mode(0o600);
        let mut file = options.open(&output).map_err(io_failure)?;
        std::io::Write::write_all(&mut file, &bytes).map_err(io_failure)?;
        manifest.push(ManifestEntry {
            path: path.clone(),
            sha256: format!("{:x}", Sha256::digest(&bytes)),
            bytes: bytes.len(),
            lines: if bytes.is_empty() {
                0
            } else {
                bytes.iter().filter(|&&b| b == b'\n').count() + 1
            },
        });
    }
    Ok(manifest)
}
pub fn copy_tree(source: &Path, target: &Path) -> Outcome<()> {
    fs::create_dir(target).map_err(io_failure)?;
    for entry in fs::read_dir(source).map_err(io_failure)? {
        let entry = entry.map_err(io_failure)?;
        let path = entry.path();
        let out = target.join(entry.file_name());
        if entry.file_type().map_err(io_failure)?.is_dir() {
            copy_tree(&path, &out)?;
        } else {
            fs::copy(&path, &out).map_err(io_failure)?;
        }
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn traversal_and_credentials() {
        for p in [
            "../x",
            "/etc/passwd",
            ".env",
            "a/.git/config",
            "a\\b",
            "a//b",
            "private.pem",
        ] {
            assert!(relative_parts(p).is_err());
        }
    }
}
