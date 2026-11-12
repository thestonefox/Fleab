//! One profile writer, with a bounded, user-local activation inbox.
use anyhow::{Context, Result, ensure};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

pub struct Primary {
    pub path: PathBuf,
    pub lock: File,
}
pub enum Claim {
    Primary(Primary),
    Activated,
}

#[derive(Serialize, Deserialize)]
struct Owner {
    pid: u32,
}
#[derive(Serialize, Deserialize)]
struct Request {
    nonce: uuid::Uuid,
}

pub fn claim() -> Result<Claim> {
    claim_at(&crate::profiles::config_path()?)
}

fn claim_at(path: &Path) -> Result<Claim> {
    let directory = path.parent().context("Missing configuration directory")?;
    fs::create_dir_all(directory)?;
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(directory.join("fleab.lock"))?;
    match lock.try_lock_exclusive() {
        Ok(()) => {
            let inbox = directory.join("activate.json");
            if inbox.exists() {
                fs::remove_file(inbox)?;
            }
            crate::platform::atomic_json(
                &directory.join("instance.json"),
                &Owner {
                    pid: std::process::id(),
                },
            )?;
            Ok(Claim::Primary(Primary {
                path: path.to_owned(),
                lock,
            }))
        }
        Err(error)
            if error.kind() == std::io::ErrorKind::WouldBlock
                || error.raw_os_error() == fs2::lock_contended_error().raw_os_error() =>
        {
            // The first process may still be writing its startup information.
            for _ in 0..20 {
                if let Ok(owner) = read::<Owner>(&directory.join("instance.json")) {
                    allow_foreground(owner.pid);
                    crate::platform::atomic_json(
                        &directory.join("activate.json"),
                        &Request {
                            nonce: uuid::Uuid::new_v4(),
                        },
                    )?;
                    return Ok(Claim::Activated);
                }
                thread::sleep(Duration::from_millis(50));
            }
            anyhow::bail!(
                "Fleab is running, but cannot be activated. Open the existing window and update/restart that older version"
            )
        }
        Err(error) => Err(error).context("Cannot acquire Fleab configuration lock"),
    }
}

fn read<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        metadata.is_file() && !crate::platform::is_link(&metadata) && metadata.len() <= 1024,
        "Invalid activation message"
    );
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}

fn allow_foreground(pid: u32) {
    #[cfg(windows)]
    unsafe {
        windows_sys::Win32::UI::WindowsAndMessaging::AllowSetForegroundWindow(pid);
    }
    #[cfg(not(windows))]
    let _ = pid;
}

pub struct Activation {
    alive: Arc<AtomicBool>,
    pending: Arc<AtomicBool>,
}
impl Activation {
    pub fn start(path: &Path, wake: impl Fn() + Send + 'static) -> Self {
        let inbox = path.with_file_name("activate.json");
        let alive = Arc::new(AtomicBool::new(true));
        let pending = Arc::new(AtomicBool::new(false));
        let (running, signal) = (alive.clone(), pending.clone());
        thread::spawn(move || {
            let mut previous = uuid::Uuid::nil();
            while running.load(Ordering::Relaxed) {
                if let Ok(request) = read::<Request>(&inbox)
                    && request.nonce != previous
                {
                    previous = request.nonce;
                    signal.store(true, Ordering::Release);
                    wake();
                }
                thread::sleep(Duration::from_millis(100));
            }
        });
        Self { alive, pending }
    }
    pub fn take(&self) -> bool {
        self.pending.swap(false, Ordering::AcqRel)
    }
}
impl Drop for Activation {
    fn drop(&mut self) {
        self.alive.store(false, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn second_launch_signals_primary_without_replacing_configuration() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("profiles.json");
        fs::write(&path, b"preserve profiles").unwrap();
        let Claim::Primary(primary) = claim_at(&path).unwrap() else {
            panic!("First launch")
        };
        let receiver = Activation::start(&path, || {});
        assert!(matches!(claim_at(&path).unwrap(), Claim::Activated));
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        while !receiver.take() {
            assert!(std::time::Instant::now() < deadline);
            thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(fs::read(path).unwrap(), b"preserve profiles");
        drop(primary);
    }
}
