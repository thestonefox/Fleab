use anyhow::{Context, Result, ensure};
use std::{
    collections::{HashMap, HashSet},
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

pub fn safe_relative(name: &str) -> Result<PathBuf> {
    ensure!(
        !name.is_empty() && name.len() <= 4096 && !name.starts_with('/'),
        "Unsafe archive path: {name}"
    );
    for part in name.split('/') {
        ensure!(
            !part.is_empty() && part != "." && part != "..",
            "Unsafe path component in {name}"
        );
        ensure!(
            !part.ends_with(['.', ' '])
                && !part
                    .chars()
                    .any(|c| c.is_control() || "\\:<>\"|?*".contains(c)),
            "Nonportable archive path: {name}"
        );
        let device = part.split('.').next().unwrap_or("").to_ascii_uppercase();
        ensure!(
            !matches!(device.as_str(), "CON" | "PRN" | "AUX" | "NUL")
                && !(device.len() == 4
                    && (device.starts_with("COM") || device.starts_with("LPT"))
                    && matches!(device.as_bytes()[3], b'1'..=b'9')),
            "Reserved device path: {name}"
        );
    }
    Ok(PathBuf::from(name))
}

fn link_target(name: &Path, target: &str) -> Result<PathBuf> {
    ensure!(
        !target.is_empty() && !target.starts_with('/') && !target.contains(['\\', ':']),
        "Unsafe symbolic link"
    );
    for part in target.split('/') {
        match part {
            "" | "." | ".." => (),
            _ => {
                safe_relative(part)?;
            }
        }
    }
    // Preserve .. until alias resolution. Collapsing it first would disagree
    // with the OS when an earlier path component is itself a symbolic link.
    Ok(name.parent().unwrap_or(Path::new("")).join(target))
}

pub fn extract(
    archive: &Path,
    staging: &Path,
    cancel: &Arc<AtomicBool>,
    progress: &mut dyn FnMut(u64),
) -> Result<()> {
    preflight(archive, cancel)?;
    let input = fs::File::open(archive)?;
    let mut decoder = zstd::stream::read::Decoder::new(input)?;
    decoder.window_log_max(27)?;
    let mut tar = tar::Archive::new(decoder);
    let mut seen = HashSet::new();
    let mut spelling = HashMap::new();
    let mut kinds = HashMap::new();
    let mut links = Vec::new();
    let mut modes = Vec::new();
    let mut total = 0u64;
    let mut count = 0;
    for item in tar.entries()? {
        ensure!(!cancel.load(Ordering::Relaxed), "Deployment cancelled");
        let mut entry = item.context("Corrupt TAR entry")?;
        count += 1;
        ensure!(count <= 250_000, "Archive entry limit exceeded");
        let bytes = entry.path_bytes();
        let name = std::str::from_utf8(&bytes)?.trim_end_matches('/');
        let relative = safe_relative(name)?;
        for ancestor in relative.ancestors().filter(|p| !p.as_os_str().is_empty()) {
            let key = ancestor.to_string_lossy().to_lowercase();
            if let Some(previous) = spelling.insert(key, ancestor.to_owned()) {
                ensure!(previous == ancestor, "Case-colliding directory spelling");
            }
        }
        ensure!(
            !name.split('/').any(|p| p.starts_with(".fleab-")),
            "Archive uses reserved Fleab metadata name"
        );
        ensure!(
            seen.insert(name.to_lowercase()),
            "Duplicate or case-colliding archive entry: {name}"
        );
        let kind = entry.header().entry_type();
        ensure!(
            kind.is_file() || kind.is_dir() || kind.is_symlink(),
            "Unsupported TAR entry type"
        );
        kinds.insert(relative.clone(), kind);
        let out = staging.join(&relative);
        if let Some(parent) = out.parent() {
            fs::create_dir_all(parent)?;
        }
        let mode = entry.header().mode()? & 0o777;
        if kind.is_dir() {
            fs::create_dir_all(&out)?;
            modes.push((out, mode | 0o700));
        } else if kind.is_symlink() {
            let target = entry
                .link_name()?
                .context("Missing symbolic link target")?
                .to_str()
                .context("Invalid link target")?
                .to_owned();
            let resolved = link_target(&relative, &target)?;
            links.push((relative, target, resolved));
        } else {
            let size = entry.size();
            total = total.checked_add(size).context("Archive size overflow")?;
            ensure!(
                total <= 1_099_511_627_776,
                "Archive expanded size exceeds 1 TiB"
            );
            let mut file = OpenOptions::new().write(true).create_new(true).open(&out)?;
            let mut copied = 0u64;
            let mut buf = [0u8; 128 * 1024];
            loop {
                ensure!(!cancel.load(Ordering::Relaxed), "Deployment cancelled");
                let n = entry.read(&mut buf)?;
                if n == 0 {
                    break;
                }
                file.write_all(&buf[..n])?;
                copied += n as u64;
                progress(n as u64);
            }
            ensure!(copied == size, "Truncated TAR member");
            file.sync_all()?;
            set_mode(&out, mode)?;
        }
    }
    ensure!(count > 0, "Archive is empty");
    // Consume the full compressed frame, including checksum/trailer. TAR stops
    // at its end markers; zstd must still be driven to EOF to detect truncation.
    let mut decoder = tar.into_inner();
    let mut tail = [0u8; 8192];
    let mut tail_len = 0usize;
    loop {
        ensure!(!cancel.load(Ordering::Relaxed), "Deployment cancelled");
        let n = decoder.read(&mut tail)?;
        if n == 0 {
            break;
        }
        tail_len += n;
        ensure!(
            tail_len <= 1_048_576 && tail[..n].iter().all(|b| *b == 0),
            "Unexpected data after TAR end"
        );
    }
    let link_map: HashMap<_, _> = links
        .iter()
        .map(|(name, _, target)| (name.clone(), target.clone()))
        .collect();
    let mut resolved_links = Vec::new();
    for (name, target, resolved) in &links {
        ensure!(!cancel.load(Ordering::Relaxed), "Deployment cancelled");
        for ancestor in name.ancestors().skip(1) {
            ensure!(
                !kinds.get(ancestor).is_some_and(|k| k.is_symlink()),
                "Symbolic link parent is another link"
            );
        }
        ensure!(
            !staging.join(name).exists(),
            "Symbolic link collides with a directory"
        );
        let endpoint = resolve_links(resolved, &link_map, staging, 0)?;
        ensure!(
            staging.join(&endpoint).exists(),
            "Symbolic link points to missing entry"
        );
        resolved_links.push((name, target, endpoint));
    }
    for (name, target, resolved) in resolved_links {
        ensure!(!cancel.load(Ordering::Relaxed), "Deployment cancelled");
        make_link(
            Path::new(&target),
            &staging.join(name),
            staging.join(resolved).is_dir(),
        )?;
    }
    for (path, mode) in modes.into_iter().rev() {
        set_mode(&path, mode)?;
    }
    Ok(())
}

// tar's high-level iterator buffers PAX/GNU metadata before yielding entries.
// Bound that metadata in a streaming validation pass before handing it to tar.
fn preflight(path: &Path, cancel: &Arc<AtomicBool>) -> Result<()> {
    let mut input = zstd::stream::read::Decoder::new(fs::File::open(path)?)?;
    input.window_log_max(27)?;
    let mut block = [0u8; 512];
    let mut buffer = [0u8; 128 * 1024];
    let mut pax_size = None;
    let mut total = 0u64;
    let mut count = 0;
    loop {
        ensure!(!cancel.load(Ordering::Relaxed), "Deployment cancelled");
        input
            .read_exact(&mut block)
            .context("Truncated TAR header or missing end markers")?;
        if block.iter().all(|b| *b == 0) {
            input
                .read_exact(&mut block)
                .context("Missing second TAR end marker")?;
            ensure!(block.iter().all(|b| *b == 0), "Invalid TAR end marker");
            break;
        }
        count += 1;
        ensure!(count <= 500_000, "Archive header limit exceeded");
        let h = tar::Header::from_byte_slice(&block);
        let sum: u32 = block
            .iter()
            .enumerate()
            .map(|(i, b)| {
                if (148..156).contains(&i) {
                    32
                } else {
                    u32::from(*b)
                }
            })
            .sum();
        ensure!(h.cksum()? == sum, "Invalid TAR checksum");
        let kind = h.entry_type();
        let raw_size = h.entry_size()?;
        if kind.is_pax_local_extensions() {
            ensure!(raw_size <= 65536, "PAX metadata exceeds 64 KiB");
            let mut data = vec![0u8; raw_size as usize];
            input.read_exact(&mut data)?;
            let mut offset = 0;
            while offset < data.len() {
                let space = data[offset..]
                    .iter()
                    .position(|b| *b == b' ')
                    .context("Malformed PAX record")?
                    + offset;
                let length = std::str::from_utf8(&data[offset..space])?.parse::<usize>()?;
                ensure!(
                    length > space - offset + 1
                        && length <= data.len() - offset
                        && data[offset + length - 1] == b'\n',
                    "Malformed PAX length"
                );
                let field = &data[space + 1..offset + length - 1];
                if let Some(value) = field.strip_prefix(b"size=") {
                    pax_size = Some(std::str::from_utf8(value)?.parse::<u64>()?);
                }
                offset += length;
            }
            input.read_exact(&mut buffer[..((512 - raw_size % 512) % 512) as usize])?;
            continue;
        }
        ensure!(
            kind.is_file() || kind.is_dir() || kind.is_symlink(),
            "Unsupported TAR header type"
        );
        let size = pax_size.take().unwrap_or(raw_size);
        if !kind.is_file() {
            ensure!(size == 0, "Non-file TAR entry has a payload");
        }
        total = total.checked_add(size).context("Archive size overflow")?;
        ensure!(
            total <= 1_099_511_627_776,
            "Archive expanded size exceeds 1 TiB"
        );
        let mut remaining = size
            .checked_add((512 - size % 512) % 512)
            .context("Archive size overflow")?;
        while remaining > 0 {
            ensure!(!cancel.load(Ordering::Relaxed), "Deployment cancelled");
            let length = remaining.min(buffer.len() as u64) as usize;
            input
                .read_exact(&mut buffer[..length])
                .context("Truncated TAR payload")?;
            remaining -= length as u64;
        }
    }
    ensure!(
        count > 0 && pax_size.is_none(),
        "Empty or incomplete TAR archive"
    );
    let mut trailing = 0usize;
    loop {
        ensure!(!cancel.load(Ordering::Relaxed), "Deployment cancelled");
        let n = input.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        trailing += n;
        ensure!(
            trailing <= 1_048_576 && buffer[..n].iter().all(|b| *b == 0),
            "Unexpected archive trailer"
        );
    }
    Ok(())
}

fn resolve_links(
    path: &Path,
    links: &HashMap<PathBuf, PathBuf>,
    staging: &Path,
    depth: usize,
) -> Result<PathBuf> {
    ensure!(depth < 40, "Symbolic link cycle or excessive chain");
    let components: Vec<_> = path.iter().collect();
    let mut prefix = PathBuf::new();
    for (index, part) in components.iter().enumerate() {
        if *part == std::ffi::OsStr::new("..") {
            ensure!(prefix.pop(), "Symbolic link escapes archive");
            continue;
        }
        if *part == std::ffi::OsStr::new(".") {
            continue;
        }
        prefix.push(part);
        if let Some(target) = links.get(&prefix) {
            let mut substituted = target.clone();
            for tail in &components[index + 1..] {
                substituted.push(tail);
            }
            return resolve_links(&substituted, links, staging, depth + 1);
        }
        if index + 1 < components.len() {
            ensure!(
                staging.join(&prefix).is_dir(),
                "Symbolic link traverses a missing or non-directory entry"
            );
        }
    }
    ensure!(
        !prefix.as_os_str().is_empty(),
        "Symbolic link targets archive root"
    );
    Ok(prefix)
}

fn set_mode(path: &Path, mode: u32) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(mode))?;
    }
    #[cfg(not(unix))]
    {
        let _ = (path, mode);
    }
    Ok(())
}
fn make_link(target: &Path, link: &Path, is_dir: bool) -> Result<()> {
    #[cfg(unix)]
    {
        let _ = is_dir;
        std::os::unix::fs::symlink(target, link)?;
    }
    #[cfg(windows)]
    {
        if is_dir {
            std::os::windows::fs::symlink_dir(target, link)?;
        } else {
            std::os::windows::fs::symlink_file(target, link)?;
        }
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (target, link, is_dir);
        anyhow::bail!("Symlinks unsupported on this OS");
    }
    Ok(())
}
