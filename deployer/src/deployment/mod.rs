use crate::{
    archives::{self, Build, Target},
    launching, platform,
    profiles::{Config, Profile},
};
use anyhow::{Context, Result, bail, ensure};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use uuid::Uuid;

pub const MARKER: &str = ".fleab-installed.json";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Installed {
    pub schema_version: u32,
    pub profile_id: Uuid,
    pub target: Target,
    pub filename: String,
    pub entrypoint: PathBuf,
    pub transaction: Uuid,
}

pub fn installed(profile: &Profile, target: Target) -> Result<Option<Installed>> {
    let Some(dest) = profile.destination(target) else {
        return Ok(None);
    };
    let marker = dest.join(MARKER);
    if !marker.exists() {
        return Ok(None);
    }
    let meta = fs::symlink_metadata(&marker)?;
    ensure!(
        meta.is_file() && !platform::is_link(&meta) && meta.len() < 65536,
        "Invalid installation record"
    );
    let value: Installed = serde_json::from_slice(&fs::read(marker)?)?;
    ensure!(
        value.schema_version == 1 && value.profile_id == profile.id && value.target == target,
        "Destination belongs to another profile or schema"
    );
    archives::safe_relative(
        value
            .entrypoint
            .to_str()
            .context("Invalid recorded executable")?,
    )?;
    let build = Build::parse(Path::new(&value.filename))?;
    ensure!(build.target == target, "Installed target mismatch");
    Ok(Some(value))
}

#[derive(Clone, Debug)]
pub struct Progress {
    pub phase: &'static str,
    pub fraction: Option<f32>,
    pub cancellable: bool,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Journal {
    schema_version: u32,
    profile_id: Uuid,
    target: Target,
    destination: PathBuf,
    transaction: Uuid,
}

struct Paths {
    stage: PathBuf,
    backup: PathBuf,
    journal: PathBuf,
    lock: PathBuf,
}
impl Paths {
    fn new(profile: &Profile, target: Target, destination: &Path) -> Result<Self> {
        let parent = destination.parent().context("Invalid destination parent")?;
        let stem = format!(".fleab-{}-{target:?}", profile.id);
        // Lock by destination name, independent of profile identity.
        let name = destination
            .file_name()
            .context("Invalid destination")?
            .to_string_lossy();
        Ok(Self {
            stage: parent.join(format!("{stem}-stage")),
            backup: parent.join(format!("{stem}-backup")),
            journal: parent.join(format!("{stem}-transaction.json")),
            lock: parent.join(format!(".{name}.fleab-lock")),
        })
    }
}

fn remove_owned(path: &Path) -> Result<()> {
    if let Ok(meta) = fs::symlink_metadata(path) {
        ensure!(
            meta.is_dir() && !platform::is_link(&meta),
            "Refusing cleanup of a link or non-directory: {}",
            path.display()
        );
        // remove_dir_all does not follow symlinks. Reject junctions at the root.
        fs::remove_dir_all(path)
            .with_context(|| format!("Cannot clean temporary directory {}", path.display()))?;
    }
    Ok(())
}

fn recover(profile: &Profile, target: Target, destination: &Path, paths: &Paths) -> Result<()> {
    if !paths.journal.exists() {
        ensure!(
            !paths.backup.exists() && !paths.stage.exists(),
            "Orphaned Fleab transaction directory; inspect it before retrying"
        );
        return Ok(());
    }
    let meta = fs::symlink_metadata(&paths.journal)?;
    ensure!(
        meta.is_file() && !platform::is_link(&meta) && meta.len() < 65536,
        "Unsafe transaction record"
    );
    let journal: Journal = serde_json::from_slice(&fs::read(&paths.journal)?)?;
    ensure!(
        journal.schema_version == 1
            && journal.profile_id == profile.id
            && journal.target == target
            && journal.destination == destination,
        "Transaction does not match this destination"
    );
    let committed =
        installed(profile, target)?.is_some_and(|i| i.transaction == journal.transaction);
    if committed {
        remove_owned(&paths.backup)?;
    } else if paths.backup.exists() {
        ensure!(
            !destination.exists(),
            "Recovery needs attention: both old and final directories exist"
        );
        ensure!(
            !platform::is_link(&fs::symlink_metadata(&paths.backup)?),
            "Unsafe backup directory"
        );
        fs::rename(&paths.backup, destination).context("Cannot restore previous installation")?;
    }
    remove_owned(&paths.stage)?;
    fs::remove_file(&paths.journal)?;
    platform::sync_dir(destination.parent().unwrap())?;
    Ok(())
}

pub fn deploy(
    config: &Config,
    profile: &Profile,
    build: &Build,
    cancel: Arc<AtomicBool>,
    mut progress: impl FnMut(Progress),
) -> Result<Installed> {
    config.validate_paths()?;
    let configured = profile
        .destination(build.target)
        .context("No destination configured for this target")?;
    let dest = platform::safe_destination(configured)?;
    let source = platform::resolved(Path::new(&profile.source))?;
    let archive = platform::resolved(&build.path)?;
    ensure!(
        archive.parent() == Some(source.as_path()),
        "Archive is outside profile source"
    );
    ensure!(
        Build::parse(&archive)?.filename == build.filename,
        "Archive identity changed"
    );
    ensure!(
        !platform::overlaps(&dest, &source),
        "Archive source overlaps deployment destination"
    );
    let parent = dest.parent().unwrap();
    fs::create_dir_all(parent)?;
    let paths = Paths::new(profile, build.target, &dest)?;
    if let Ok(meta) = fs::symlink_metadata(&paths.lock) {
        ensure!(!platform::is_link(&meta), "Unsafe deployment lock");
    }
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&paths.lock)?;
    lock.try_lock_exclusive()
        .context("Another deployment is using this destination")?;
    platform::ensure_not_running(&dest)?;
    recover(profile, build.target, &dest, &paths)?;
    if dest.exists() && fs::read_dir(&dest)?.next().is_some() {
        ensure!(
            installed(profile, build.target)?.is_some(),
            "Destination contains unrelated files. Choose an empty directory or a Fleab installation owned by this profile"
        );
    }
    let txn = Uuid::new_v4();
    platform::atomic_json(
        &paths.journal,
        &Journal {
            schema_version: 1,
            profile_id: profile.id,
            target: build.target,
            destination: dest.clone(),
            transaction: txn,
        },
    )?;
    let result = (|| -> Result<Installed> {
        fs::create_dir(&paths.stage)?;
        progress(Progress {
            phase: "Copying archive locally",
            fraction: Some(0.0),
            cancellable: true,
        });
        let mut local = tempfile::NamedTempFile::new()?;
        let mut input = fs::File::open(&archive)?;
        let size = input.metadata()?.len();
        let mut done = 0u64;
        let mut buf = [0u8; 128 * 1024];
        loop {
            ensure!(!cancel.load(Ordering::Relaxed), "Deployment cancelled");
            let n = input.read(&mut buf)?;
            if n == 0 {
                break;
            }
            local.write_all(&buf[..n])?;
            done += n as u64;
            progress(Progress {
                phase: "Copying archive locally",
                fraction: Some(done as f32 / size.max(1) as f32),
                cancellable: true,
            });
        }
        ensure!(
            done == size && input.metadata()?.len() == size,
            "Archive changed or copy was incomplete"
        );
        local.as_file().sync_all()?;
        progress(Progress {
            phase: "Validating and extracting",
            fraction: None,
            cancellable: true,
        });
        archives::extract(local.path(), &paths.stage, &cancel, &mut |_| {
            progress(Progress {
                phase: "Validating and extracting",
                fraction: None,
                cancellable: true,
            });
        })?;
        let entrypoint = launching::detect(
            &paths.stage,
            build.target,
            profile
                .executable_overrides
                .get(&build.target)
                .map(String::as_str),
        )?;
        let record = Installed {
            schema_version: 1,
            profile_id: profile.id,
            target: build.target,
            filename: build.filename.clone(),
            entrypoint,
            transaction: txn,
        };
        platform::atomic_json(&paths.stage.join(MARKER), &record)?;
        ensure!(!cancel.load(Ordering::Relaxed), "Deployment cancelled");
        platform::ensure_not_running(&dest)?;
        ensure!(
            platform::safe_destination(configured)? == dest,
            "Destination changed during staging"
        );
        progress(Progress {
            phase: "Replacing installation",
            fraction: None,
            cancellable: false,
        });
        if dest.exists() {
            fs::rename(&dest, &paths.backup)
                .context("Cannot move previous installation; close files or programs using it")?;
        }
        platform::sync_dir(parent)?;
        if let Err(e) = fs::rename(&paths.stage, &dest) {
            if paths.backup.exists() {
                fs::rename(&paths.backup, &dest).context("Replacement failed and backup restoration failed; preserve the transaction files")?;
            }
            bail!("Replacement failed; previous installation restored: {e}");
        }
        platform::sync_dir(parent)?;
        Ok(record)
    })();
    match result {
        Ok(record) => {
            // A cleanup error reports the actual committed state. Recovery on
            // retry completes cleanup instead of rolling a committed build back.
            recover(profile, build.target, &dest, &paths)
                .context("Build installed, but temporary cleanup failed. Retry to recover")?;
            Ok(record)
        }
        Err(error) => {
            recover(profile, build.target, &dest, &paths)
                .with_context(|| format!("{error:#}; transaction recovery also failed"))?;
            Err(error)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn interrupted_replacement_restores_backup() {
        let root = tempfile::tempdir_in(std::env::current_dir().unwrap()).unwrap();
        let mut p = Profile::default();
        let dest = root.path().join("game");
        p.destinations
            .insert(Target::WindowsClient, dest.to_string_lossy().into());
        let paths = Paths::new(&p, Target::WindowsClient, &dest).unwrap();
        fs::create_dir(&paths.backup).unwrap();
        fs::write(paths.backup.join("old"), b"old installation").unwrap();
        fs::create_dir(&paths.stage).unwrap();
        fs::write(paths.stage.join("partial"), b"new").unwrap();
        platform::atomic_json(
            &paths.journal,
            &Journal {
                schema_version: 1,
                profile_id: p.id,
                target: Target::WindowsClient,
                destination: dest.clone(),
                transaction: Uuid::new_v4(),
            },
        )
        .unwrap();
        recover(&p, Target::WindowsClient, &dest, &paths).unwrap();
        assert_eq!(fs::read(dest.join("old")).unwrap(), b"old installation");
        assert!(!paths.stage.exists() && !paths.backup.exists() && !paths.journal.exists());
    }
    #[test]
    fn interrupted_cleanup_keeps_committed_installation() {
        let root = tempfile::tempdir_in(std::env::current_dir().unwrap()).unwrap();
        let mut p = Profile::default();
        let dest = root.path().join("game");
        p.destinations
            .insert(Target::WindowsClient, dest.to_string_lossy().into());
        let paths = Paths::new(&p, Target::WindowsClient, &dest).unwrap();
        fs::create_dir(&dest).unwrap();
        fs::create_dir(&paths.backup).unwrap();
        fs::write(paths.backup.join("old"), b"old").unwrap();
        let transaction = Uuid::new_v4();
        let record = Installed {
            schema_version: 1,
            profile_id: p.id,
            target: Target::WindowsClient,
            filename: "Game_1_20261108-093000_Windows64.tar.zst".into(),
            entrypoint: "Game.exe".into(),
            transaction,
        };
        platform::atomic_json(&dest.join(MARKER), &record).unwrap();
        platform::atomic_json(
            &paths.journal,
            &Journal {
                schema_version: 1,
                profile_id: p.id,
                target: Target::WindowsClient,
                destination: dest.clone(),
                transaction,
            },
        )
        .unwrap();
        recover(&p, Target::WindowsClient, &dest, &paths).unwrap();
        assert!(dest.join(MARKER).exists());
        assert!(!paths.backup.exists());
    }
}
