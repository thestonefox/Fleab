use anyhow::{Context, Result, bail, ensure};
use std::{
    fs,
    path::{Component, Path, PathBuf},
};

/// Resolve existing ancestors, including links, before comparing paths.
pub fn resolved(path: &Path) -> Result<PathBuf> {
    ensure!(
        path.is_absolute(),
        "Use an absolute path: {}",
        path.display()
    );
    let mut tail = Vec::new();
    let mut parent = path;
    while !parent.exists() {
        if fs::symlink_metadata(parent).is_ok() {
            bail!("Broken link in path: {}", parent.display());
        }
        tail.push(parent.file_name().context("Invalid path")?.to_owned());
        parent = parent.parent().context("Path has no existing ancestor")?;
    }
    let mut result =
        fs::canonicalize(parent).with_context(|| format!("Cannot access {}", parent.display()))?;
    for part in tail.into_iter().rev() {
        result.push(part);
    }
    Ok(result)
}

pub fn overlaps(a: &Path, b: &Path) -> bool {
    // Windows canonical paths compare case-insensitively. On macOS use the
    // same conservative policy even on a case-sensitive volume.
    #[cfg(any(windows, target_os = "macos"))]
    let (a, b) = (
        PathBuf::from(a.to_string_lossy().to_lowercase()),
        PathBuf::from(b.to_string_lossy().to_lowercase()),
    );
    #[cfg(any(windows, target_os = "macos"))]
    let (a, b) = (a.as_path(), b.as_path());
    a.starts_with(b) || b.starts_with(a)
}

pub fn safe_destination(path: &Path) -> Result<PathBuf> {
    ensure!(
        !path
            .components()
            .any(|c| matches!(c, Component::ParentDir | Component::CurDir)),
        "Destination must not contain . or .."
    );
    let p = resolved(path)?;
    ensure!(
        p.parent().is_some() && p.file_name().is_some(),
        "A filesystem root cannot be a destination"
    );
    if let Some(home) = directories::BaseDirs::new() {
        let h = resolved(home.home_dir())?;
        ensure!(
            !h.starts_with(&p),
            "A home directory or its ancestor cannot be a destination"
        );
        for protected in [home.config_dir(), home.data_dir(), home.data_local_dir()] {
            if let Ok(protected) = resolved(protected) {
                ensure!(
                    !overlaps(&p, &protected),
                    "Application data directories cannot be deployment targets"
                );
            }
        }
    }
    if let Some(user) = directories::UserDirs::new() {
        for folder in [
            user.desktop_dir(),
            user.document_dir(),
            user.download_dir(),
            user.picture_dir(),
            user.audio_dir(),
            user.video_dir(),
            user.public_dir(),
            user.template_dir(),
        ]
        .into_iter()
        .flatten()
        {
            if let Ok(folder) = resolved(folder) {
                ensure!(
                    !folder.starts_with(&p),
                    "A user folder root or its ancestor cannot be a destination"
                );
            }
        }
    }
    #[cfg(windows)]
    for key in [
        "SystemRoot",
        "ProgramFiles",
        "ProgramFiles(x86)",
        "ProgramData",
    ] {
        if let Some(dir) = std::env::var_os(key) {
            ensure!(
                !overlaps(&p, &resolved(Path::new(&dir))?),
                "System directories cannot be destinations"
            );
        }
    }
    #[cfg(unix)]
    for root in [
        "/bin",
        "/sbin",
        "/usr",
        "/etc",
        "/var",
        "/lib",
        "/lib64",
        "/System",
        "/Applications",
        "/Library",
        "/dev",
        "/proc",
        "/sys",
        "/boot",
    ] {
        if let Ok(root) = resolved(Path::new(root)) {
            ensure!(
                !overlaps(&p, &root),
                "System directories cannot be destinations"
            );
        }
    }
    // Never replace a destination which itself is a symlink/junction.
    if let Ok(meta) = fs::symlink_metadata(path) {
        ensure!(!is_link(&meta), "Destination cannot be a link or junction");
        ensure!(meta.is_dir(), "Destination is not a directory");
    }
    Ok(p)
}

pub fn is_link(meta: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        meta.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        meta.file_type().is_symlink()
    }
}

pub fn atomic_json<T: serde::Serialize>(path: &Path, value: &T) -> Result<()> {
    use std::io::Write;
    let parent = path.parent().context("Missing parent directory")?;
    fs::create_dir_all(parent)?;
    let mut tmp = tempfile::NamedTempFile::new_in(parent)?;
    tmp.write_all(&serde_json::to_vec_pretty(value)?)?;
    tmp.as_file().sync_all()?;
    tmp.persist(path).map_err(|e| e.error)?;
    sync_dir(parent)?;
    Ok(())
}

pub fn sync_dir(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        fs::File::open(path)?.sync_all()?;
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
    Ok(())
}

pub fn ensure_not_running(root: &Path) -> Result<()> {
    use sysinfo::{ProcessesToUpdate, System};
    let mut system = System::new();
    system.refresh_processes(ProcessesToUpdate::All, true);
    for process in system.processes().values() {
        if let Some(exe) = process.exe()
            && let Ok(exe) = resolved(exe)
            && exe.starts_with(root)
        {
            bail!(
                "Close the running game/server (PID {}) before deploying",
                process.pid()
            );
        }
        // Wine often exposes wine-preloader as its executable. Check complete
        // path arguments as well, without guessing based on process names.
        for arg in process.cmd() {
            let text = arg.to_string_lossy();
            let candidate = if cfg!(target_os = "linux") && text.starts_with("Z:\\") {
                PathBuf::from(text[2..].replace('\\', "/"))
            } else {
                PathBuf::from(arg)
            };
            if candidate.is_absolute()
                && let Ok(path) = resolved(&candidate)
                && path.starts_with(root)
            {
                bail!(
                    "Close the running game/server (PID {}) before deploying",
                    process.pid()
                );
            }
        }
    }
    Ok(())
}
