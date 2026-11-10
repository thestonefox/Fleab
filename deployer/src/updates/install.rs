use super::*;
use anyhow::bail;
use fs2::FileExt;
use std::{
    ffi::OsString,
    process::{Child, Command},
    sync::{
        OnceLock,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

const HELPER_FLAG: &str = "--fleab-update-helper";
const HEALTH_FLAG: &str = "--fleab-update-health";
static HEALTH: OnceLock<PathBuf> = OnceLock::new();
static HEALTH_ACKNOWLEDGED: AtomicBool = AtomicBool::new(false);
static STARTUP_ERROR: OnceLock<String> = OnceLock::new();

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ProcessIdentity {
    pub pid: u32,
    pub start_time: u64,
}
impl ProcessIdentity {
    pub fn current() -> Result<Self> {
        Self::from_pid(std::process::id())
    }
    pub fn from_pid(pid: u32) -> Result<Self> {
        let mut system = sysinfo::System::new();
        let id = sysinfo::Pid::from_u32(pid);
        system.refresh_processes(sysinfo::ProcessesToUpdate::Some(&[id]), true);
        let process = system
            .process(id)
            .context("Cannot identify the application process")?;
        Ok(Self {
            pid,
            start_time: process.start_time(),
        })
    }
    fn running(&self) -> bool {
        Self::from_pid(self.pid).is_ok_and(|p| p == *self)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
enum Phase {
    Prepared,
    Applying,
    Applied,
    Committed,
    RolledBack,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Plan {
    schema_version: u32,
    id: uuid::Uuid,
    cache: PathBuf,
    work: PathBuf,
    destination: PathBuf,
    executable: PathBuf,
    original_version: String,
    original_hash: String,
    target: String,
    envelope: Envelope,
    parent: ProcessIdentity,
    phase: Phase,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Pending {
    transaction: PathBuf,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Healthy {
    id: uuid::Uuid,
    version: String,
    pid: u32,
    executable_hash: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Report {
    pub message: String,
    pub version: String,
    pub backup: Option<PathBuf>,
    #[serde(default)]
    transaction: Option<PathBuf>,
}
pub struct Prepared {
    pub transaction: PathBuf,
    pub helper: PathBuf,
}
impl Plan {
    fn file(&self) -> PathBuf {
        self.cache.join("transaction.json")
    }
    fn replacement(&self) -> PathBuf {
        self.work.join(if self.target.starts_with("macos-") {
            "replacement.app"
        } else {
            "replacement"
        })
    }
    fn backup(&self) -> PathBuf {
        if self.target.starts_with("macos-") {
            self.replacement()
        } else {
            self.work.join("previous")
        }
    }
    fn helper(&self) -> PathBuf {
        self.cache.join(if cfg!(windows) {
            "updater.exe"
        } else {
            "updater"
        })
    }
    fn installed_binary(&self) -> PathBuf {
        if self.target.starts_with("macos-") {
            self.destination.join("Contents/MacOS/fleab")
        } else {
            self.destination.clone()
        }
    }
    fn replacement_binary(&self) -> PathBuf {
        if self.target.starts_with("macos-") {
            self.replacement().join("Contents/MacOS/fleab")
        } else {
            self.replacement()
        }
    }
    fn backup_binary(&self) -> PathBuf {
        if self.target.starts_with("macos-") {
            self.backup().join("Contents/MacOS/fleab")
        } else {
            self.backup()
        }
    }
    fn save(&self) -> Result<()> {
        crate::platform::atomic_json(&self.file(), self)
    }
}

fn regular(path: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        metadata.is_file() && !crate::platform::is_link(&metadata),
        "Update path is not a regular file: {}",
        path.display()
    );
    Ok(())
}
fn destination(executable: &Path, target: &str) -> Result<PathBuf> {
    regular(executable)?;
    let executable = fs::canonicalize(executable)?;
    if target.starts_with("macos-") {
        ensure!(
            executable.ends_with("Contents/MacOS/fleab"),
            "Run Fleab from its .app bundle to update"
        );
        let bundle = executable
            .parent()
            .and_then(Path::parent)
            .and_then(Path::parent)
            .context("Invalid application bundle")?;
        ensure!(
            bundle.extension().is_some_and(|s| s == "app"),
            "Invalid application bundle"
        );
        let info: plist::Value = plist::from_file(bundle.join("Contents/Info.plist"))?;
        ensure!(
            info.as_dictionary()
                .and_then(|d| d.get("CFBundleIdentifier"))
                .and_then(plist::Value::as_string)
                == Some("org.fleab.deployer"),
            "Not a Fleab application bundle"
        );
        Ok(bundle.to_owned())
    } else {
        ensure!(
            executable.file_name().is_some_and(|s| s
                == if target.starts_with("windows-") {
                    "fleab.exe"
                } else {
                    "fleab"
                }),
            "Run the app with its original filename to update"
        );
        Ok(executable)
    }
}

pub fn prepare(
    download: Downloaded,
    executable: &Path,
    parent: ProcessIdentity,
    config: &crate::profiles::Config,
    paths: &Paths,
) -> Result<Prepared> {
    let manifest = verify(&download.envelope, &production_keys()?)?;
    ensure!(
        manifest.version == download.manifest.version
            && stable_version(&manifest.version)? > stable_version(VERSION)?,
        "Not a newer signed release"
    );
    ensure!(
        download.target == target()?,
        "Update target does not match this device"
    );
    let executable = fs::canonicalize(executable)?;
    let destination = destination(&executable, &download.target)?;
    if download.target.starts_with("macos-") {
        for profile in &config.profiles {
            for path in profile
                .destinations
                .values()
                .chain(std::iter::once(&profile.source))
                .filter(|p| !p.is_empty())
            {
                ensure!(
                    !crate::platform::overlaps(
                        &destination,
                        &crate::platform::resolved(Path::new(path))?
                    ),
                    "Move game folders outside Fleab.app before updating"
                );
            }
        }
    }
    ensure!(
        !paths.pending.exists(),
        "A previous update needs recovery before installing another"
    );
    let id = uuid::Uuid::new_v4();
    let work = destination
        .parent()
        .context("Missing app directory")?
        .join(format!(".fleab-update-{id}"));
    fs::create_dir(&work).context(
        "Cannot write beside Fleab; move it to a writable location or download the update manually",
    )?;
    let cache = fs::canonicalize(download.directory.path())?;
    let mut plan = Plan {
        schema_version: 1,
        id,
        cache,
        work,
        destination,
        executable,
        original_version: VERSION.into(),
        original_hash: String::new(),
        target: download.target.clone(),
        envelope: download.envelope.clone(),
        parent,
        phase: Phase::Prepared,
    };
    let result =
        (|| -> Result<()> {
            crate::platform::atomic_json(&plan.work.join("owner.json"), &id)?;
            crate::platform::atomic_json(&plan.cache.join("owner.json"), &id)?;
            let source = download.directory.path().join("payload").join(
                if plan.target.starts_with("macos-") {
                    "Fleab.app"
                } else {
                    super::client::executable_relative(&plan.target)
                },
            );
            copy_application(&source, &plan.replacement())?;
            let asset = manifest
                .assets
                .get(&plan.target)
                .context("Missing target")?;
            ensure!(
                hash_file(&plan.replacement_binary())? == asset.executable_sha256,
                "Staged app is corrupt"
            );
            #[cfg(target_os = "macos")]
            ensure!(
                Command::new("/usr/bin/codesign")
                    .args(["--verify", "--deep", "--strict"])
                    .arg(plan.replacement())
                    .status()?
                    .success(),
                "Updated app bundle signature is invalid"
            );
            plan.original_hash = hash_file(&plan.executable)?;
            fs::copy(&plan.executable, plan.helper())?;
            sync_copy(&plan.helper())?;
            ensure!(
                hash_file(&plan.helper())? == plan.original_hash,
                "Cannot prepare updater helper"
            );
            if !plan.target.starts_with("macos-") {
                fs::copy(&plan.destination, plan.backup())?;
                sync_copy(&plan.backup())?;
                ensure!(
                    hash_file(&plan.backup())? == plan.original_hash,
                    "Cannot create application backup"
                );
            }
            plan.save()?;
            crate::platform::sync_dir(&plan.work)?;
            crate::platform::atomic_json(
                &paths.pending,
                &Pending {
                    transaction: plan.file(),
                },
            )?;
            Ok(())
        })();
    if let Err(error) = result {
        let _ = clear_pending(paths, &plan);
        if owned(&plan.work, id) {
            let _ = fs::remove_dir_all(&plan.work);
        }
        return Err(error);
    }
    let prepared = Prepared {
        transaction: plan.file(),
        helper: plan.helper(),
    };
    let _ = download.directory.keep();
    Ok(prepared)
}

fn owned(path: &Path, id: uuid::Uuid) -> bool {
    fs::symlink_metadata(path).is_ok_and(|m| m.is_dir() && !crate::platform::is_link(&m))
        && read_json::<uuid::Uuid>(&path.join("owner.json"), 256).is_ok_and(|owner| owner == id)
}
fn read_json<T: serde::de::DeserializeOwned>(path: &Path, limit: u64) -> Result<T> {
    regular(path)?;
    ensure!(fs::metadata(path)?.len() <= limit, "Oversized update state");
    Ok(serde_json::from_reader(
        fs::File::open(path)?.take(limit + 1),
    )?)
}
fn load(path: &Path, paths: &Paths) -> Result<(Plan, Manifest)> {
    let plan: Plan = read_json(path, MAX_MANIFEST * 2)?;
    ensure!(
        plan.schema_version == 1 && plan.target == target()?,
        "Unsupported update transaction"
    );
    ensure!(
        path == plan.file()
            && fs::canonicalize(plan.cache.parent().context("Invalid cache")?)?
                == fs::canonicalize(&paths.root)?,
        "Update transaction is outside the application cache"
    );
    ensure!(
        plan.cache
            .file_name()
            .is_some_and(|s| s.to_string_lossy().starts_with("download-"))
            && owned(&plan.cache, plan.id),
        "Invalid update cache ownership"
    );
    let expected = plan
        .destination
        .parent()
        .context("Missing app parent")?
        .join(format!(".fleab-update-{}", plan.id));
    ensure!(
        plan.work == expected && owned(&plan.work, plan.id),
        "Invalid update workspace ownership"
    );
    ensure!(
        plan.executable == plan.installed_binary(),
        "Invalid application path"
    );
    ensure!(
        valid_hash(&plan.original_hash),
        "Invalid original application digest"
    );
    let manifest = verify(&plan.envelope, &production_keys()?)?;
    ensure!(
        stable_version(&manifest.version)? > stable_version(&plan.original_version)?,
        "Update is not newer than the original app"
    );
    ensure!(
        manifest.assets.contains_key(&plan.target),
        "Missing update target"
    );
    Ok((plan, manifest))
}
fn copy_application(source: &Path, destination: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(source)?;
    ensure!(
        !crate::platform::is_link(&metadata),
        "Application update contains a link"
    );
    if metadata.is_file() {
        ensure!(!destination.exists(), "Update staging path already exists");
        fs::copy(source, destination)?;
        sync_copy(destination)?;
    } else {
        ensure!(metadata.is_dir(), "Invalid application entry");
        fs::create_dir(destination)?;
        for entry in fs::read_dir(source)? {
            let entry = entry?;
            copy_application(&entry.path(), &destination.join(entry.file_name()))?;
        }
        crate::platform::sync_dir(destination)?;
    }
    Ok(())
}

fn sync_copy(path: &Path) -> Result<()> {
    // FlushFileBuffers needs a writable handle on Windows. Temporarily grant
    // write access only to our private copy, preserving its original mode.
    let original = fs::metadata(path)?.permissions();
    let mut writable = original.clone();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        writable.set_mode(original.mode() | 0o200);
    }
    #[cfg(windows)]
    #[allow(
        clippy::permissions_set_readonly_false,
        reason = "Windows readonly attribute only; Unix uses owner-write PermissionsExt above"
    )]
    writable.set_readonly(false);
    fs::set_permissions(path, writable)?;
    let result = fs::OpenOptions::new()
        .write(true)
        .open(path)
        .and_then(|file| file.sync_all());
    fs::set_permissions(path, original)?;
    result?;
    Ok(())
}

fn spawn_helper(prepared: &Prepared, recover: bool) -> Result<Child> {
    let mut command = Command::new(&prepared.helper);
    command
        .arg(HELPER_FLAG)
        .arg(&prepared.transaction)
        .arg(if recover { "recover" } else { "apply" });
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    command.spawn().context("Could not start the update helper")
}
pub fn start(prepared: &Prepared) -> Result<()> {
    let (plan, _) = load(&prepared.transaction, &Paths::current()?)?;
    ensure!(
        plan.phase == Phase::Prepared && hash_file(&prepared.helper)? == plan.original_hash,
        "Invalid update helper"
    );
    let mut child = spawn_helper(prepared, false)?;
    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline {
        if plan.cache.join("ready").exists() {
            return Ok(());
        }
        if child.try_wait()?.is_some() {
            bail!("Update helper could not start; app files were preserved");
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    // Do not leave a helper waiting to install after the UI reports failure.
    let _ = child.kill();
    let _ = child.wait();
    bail!("Update helper did not become ready; app files were preserved")
}

fn swap(source: &Path, destination: &Path) -> Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        let source: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
        let destination: Vec<u16> = destination
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect();
        let result = unsafe {
            windows_sys::Win32::Storage::FileSystem::ReplaceFileW(
                destination.as_ptr(),
                source.as_ptr(),
                std::ptr::null(),
                0,
                std::ptr::null(),
                std::ptr::null(),
            )
        };
        ensure!(
            result != 0,
            "Cannot replace app: {}",
            std::io::Error::last_os_error()
        );
    }
    #[cfg(target_os = "macos")]
    {
        use std::os::unix::ffi::OsStrExt;
        let source = std::ffi::CString::new(source.as_os_str().as_bytes())?;
        let destination = std::ffi::CString::new(destination.as_os_str().as_bytes())?;
        let result =
            unsafe { libc::renamex_np(source.as_ptr(), destination.as_ptr(), libc::RENAME_SWAP) };
        ensure!(
            result == 0,
            "Cannot exchange app bundles: {}",
            std::io::Error::last_os_error()
        );
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    fs::rename(source, destination)?;
    Ok(())
}
fn apply(plan: &mut Plan, manifest: &Manifest) -> Result<()> {
    regular(&plan.installed_binary())?;
    regular(&plan.replacement_binary())?;
    ensure!(
        hash_file(&plan.installed_binary())? == plan.original_hash,
        "Installed application changed; refusing to overwrite it"
    );
    ensure!(
        hash_file(&plan.replacement_binary())? == manifest.assets[&plan.target].executable_sha256,
        "Staged application changed"
    );
    if !plan.target.starts_with("macos-") {
        ensure!(
            hash_file(&plan.backup())? == plan.original_hash,
            "Application backup is corrupt"
        );
    }
    plan.phase = Phase::Applying;
    plan.save()?;
    swap(&plan.replacement(), &plan.destination)?;
    crate::platform::sync_dir(plan.destination.parent().context("Missing app parent")?)?;
    plan.phase = Phase::Applied;
    plan.save()?;
    Ok(())
}
fn restore(plan: &mut Plan, manifest: &Manifest) -> Result<()> {
    let current = hash_file(&plan.installed_binary()).ok();
    if current.as_deref() != Some(plan.original_hash.as_str()) {
        ensure!(
            current.is_none()
                || current.as_deref()
                    == Some(manifest.assets[&plan.target].executable_sha256.as_str()),
            "Application changed after update; backup preserved for manual recovery"
        );
        ensure!(
            hash_file(&plan.backup_binary())? == plan.original_hash,
            "Original application backup is missing or corrupt"
        );
        if plan.destination.exists() {
            swap(&plan.backup(), &plan.destination)?;
        } else {
            fs::rename(plan.backup(), &plan.destination)?;
        }
    }
    plan.phase = Phase::RolledBack;
    plan.save()?;
    Ok(())
}
fn clear_pending(paths: &Paths, plan: &Plan) -> Result<()> {
    if paths.pending.exists() {
        ensure!(
            read_json::<Pending>(&paths.pending, MAX_MANIFEST)?.transaction == plan.file(),
            "Another update owns the pending record"
        );
        fs::remove_file(&paths.pending)?;
    }
    Ok(())
}
fn report(paths: &Paths, plan: &Plan, message: String, backup: bool) -> Result<()> {
    crate::platform::atomic_json(
        &paths.report,
        &Report {
            message,
            version: if plan.phase == Phase::Committed {
                verify(&plan.envelope, &production_keys()?)?.version
            } else {
                plan.original_version.clone()
            },
            backup: backup.then(|| plan.backup()),
            transaction: Some(plan.file()),
        },
    )
}
fn launch(plan: &Plan, health: bool) -> Result<Child> {
    let mut command = Command::new(plan.installed_binary());
    command.current_dir(plan.destination.parent().context("Missing app directory")?);
    if health {
        command.arg(HEALTH_FLAG).arg(plan.file());
    }
    Ok(command.spawn()?)
}
fn helper(path: &Path, recover: bool, paths: &Paths) -> Result<()> {
    let (mut plan, manifest) = load(path, paths)?;
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(plan.cache.join("install.lock"))?;
    lock.try_lock_exclusive()
        .context("Another update helper is active")?;
    ensure!(
        hash_file(&std::env::current_exe()?)? == plan.original_hash,
        "Not the original application helper"
    );
    ensure!(
        VERSION == plan.original_version,
        "Updater helper version mismatch"
    );
    ensure!(
        plan.phase == Phase::Prepared
            || (recover && (plan.phase == Phase::Applied || plan.phase == Phase::Applying)),
        "Update was already attempted"
    );
    ensure!(
        read_json::<Pending>(&paths.pending, MAX_MANIFEST)?.transaction == plan.file(),
        "Update was cancelled before the helper became ready"
    );
    fs::write(plan.cache.join("ready"), plan.id.to_string())?;
    let deadline = Instant::now() + Duration::from_secs(90);
    while plan.parent.running() {
        ensure!(
            Instant::now() < deadline,
            "Application did not exit; update cancelled"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
    let result = (|| -> Result<()> {
        if recover {
            restore(&mut plan, &manifest)?;
            bail!("An interrupted update was rolled back");
        }
        apply(&mut plan, &manifest)?;
        let mut child = launch(&plan, true).context("Updated application could not start")?;
        let deadline = Instant::now() + Duration::from_secs(45);
        loop {
            if let Ok(healthy) = read_json::<Healthy>(&plan.cache.join("healthy.json"), 4096)
                && healthy.id == plan.id
                && healthy.version == manifest.version
                && healthy.pid == child.id()
                && healthy.executable_hash == manifest.assets[&plan.target].executable_sha256
                && child.try_wait()?.is_none()
            {
                plan.phase = Phase::Committed;
                plan.save()?;
                let previous = last_report(paths);
                report(
                    paths,
                    &plan,
                    format!("Updated to Fleab {}", manifest.version),
                    true,
                )?;
                clear_pending(paths, &plan)?;
                cleanup_previous(previous, &plan, paths);
                return Ok(());
            }
            if child.try_wait()?.is_some() {
                bail!("Updated application exited before becoming ready");
            }
            if Instant::now() >= deadline {
                // This is only the new process started by this helper, never a game.
                let _ = child.kill();
                let _ = child.wait();
                bail!("Updated application did not become ready");
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    })();
    if let Err(error) = result {
        restore(&mut plan, &manifest)?;
        report(
            paths,
            &plan,
            format!("Update rolled back: {error:#}"),
            false,
        )?;
        clear_pending(paths, &plan)?;
        launch(&plan, false).context("Previous version was restored but could not be restarted")?;
    }
    Ok(())
}

/// Runs before native UI creation. Helpers never create a window of their own.
/// Returns true if this process completed an internal helper/recovery action.
pub fn bootstrap() -> Result<bool> {
    match bootstrap_inner() {
        Ok(handled) => Ok(handled),
        Err(error)
            if !std::env::args_os()
                .skip(1)
                .any(|s| s == HELPER_FLAG || s == HEALTH_FLAG) =>
        {
            let _ = STARTUP_ERROR.set(format!(
                "Update recovery needs attention; app files were preserved: {error:#}"
            ));
            Ok(false)
        }
        Err(error) => Err(error),
    }
}
fn bootstrap_inner() -> Result<bool> {
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    let paths = Paths::current()?;
    if args.first().is_some_and(|s| s == HELPER_FLAG) {
        ensure!(
            args.len() == 3 && (args[2] == "apply" || args[2] == "recover"),
            "Invalid updater arguments"
        );
        helper(Path::new(&args[1]), args[2] == "recover", &paths)?;
        return Ok(true);
    }
    if args.first().is_some_and(|s| s == HEALTH_FLAG) {
        ensure!(args.len() == 2, "Invalid update health arguments");
        let path = PathBuf::from(&args[1]);
        let (plan, manifest) = load(&path, &paths)?;
        ensure!(
            plan.phase == Phase::Applied
                && manifest.version == VERSION
                && fs::canonicalize(std::env::current_exe()?)? == plan.executable
                && hash_file(&plan.executable)? == manifest.assets[&plan.target].executable_sha256,
            "Updated application failed startup verification"
        );
        HEALTH
            .set(path)
            .map_err(|_| anyhow::anyhow!("Startup ticket already registered"))?;
        return Ok(false);
    }
    if paths.pending.exists() {
        let pending: Pending = read_json(&paths.pending, MAX_MANIFEST)?;
        let (mut plan, manifest) = load(&pending.transaction, &paths)?;
        if fs::canonicalize(std::env::current_exe()?)? != plan.executable {
            return Ok(false);
        }
        let lock = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(plan.cache.join("install.lock"))?;
        if lock.try_lock_exclusive().is_err() {
            return Ok(true);
        }
        let current = hash_file(&plan.executable)?;
        if plan.phase == Phase::Committed
            && current == manifest.assets[&plan.target].executable_sha256
        {
            report(
                &paths,
                &plan,
                format!("Updated to Fleab {}", manifest.version),
                true,
            )?;
            clear_pending(&paths, &plan)?;
            return Ok(false);
        }
        if current == manifest.assets[&plan.target].executable_sha256
            && current != plan.original_hash
        {
            plan.parent = ProcessIdentity::current()?;
            plan.save()?;
            ensure!(
                hash_file(&plan.helper())? == plan.original_hash,
                "Recovery helper is missing; original backup retained"
            );
            drop(lock);
            spawn_helper(
                &Prepared {
                    transaction: plan.file(),
                    helper: plan.helper(),
                },
                true,
            )?;
            return Ok(true);
        }
        ensure!(
            current == plan.original_hash,
            "Pending update does not match this installation"
        );
        plan.phase = Phase::RolledBack;
        plan.save()?;
        report(
            &paths,
            &plan,
            "An interrupted update was cancelled; the original application is intact.".into(),
            false,
        )?;
        clear_pending(&paths, &plan)?;
    }
    Ok(false)
}
pub fn awaiting_health() -> bool {
    HEALTH.get().is_some() && !HEALTH_ACKNOWLEDGED.load(Ordering::Relaxed)
}
pub fn acknowledge_startup() -> Result<()> {
    if let Some(path) = HEALTH.get() {
        let (plan, manifest) = load(path, &Paths::current()?)?;
        crate::platform::atomic_json(
            &plan.cache.join("healthy.json"),
            &Healthy {
                id: plan.id,
                version: manifest.version,
                pid: std::process::id(),
                executable_hash: hash_file(&plan.executable)?,
            },
        )?;
        HEALTH_ACKNOWLEDGED.store(true, Ordering::Relaxed);
    }
    Ok(())
}
pub fn last_report(paths: &Paths) -> Option<Report> {
    read_json(&paths.report, 65_536).ok()
}
fn cleanup_previous(previous: Option<Report>, current: &Plan, paths: &Paths) {
    let Some(path) = previous.and_then(|r| r.transaction) else {
        return;
    };
    let Ok((old, _)) = load(&path, paths) else {
        return;
    };
    if old.id == current.id
        || old.destination != current.destination
        || !matches!(old.phase, Phase::Committed | Phase::RolledBack)
    {
        return;
    }
    let Ok(config_path) = crate::profiles::config_path() else {
        return;
    };
    let Ok(config) = crate::profiles::Config::load(&config_path) else {
        return;
    };
    // Never clean a workspace which the user has subsequently configured as a
    // game/archive folder, even though its name and ownership marker are ours.
    for profile in &config.profiles {
        for value in profile
            .destinations
            .values()
            .chain(std::iter::once(&profile.source))
            .filter(|p| !p.is_empty())
        {
            let Ok(path) = crate::platform::resolved(Path::new(value)) else {
                return;
            };
            if crate::platform::overlaps(&path, &old.work)
                || crate::platform::overlaps(&path, &old.cache)
            {
                return;
            }
        }
    }
    let Ok(lock) = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(old.cache.join("install.lock"))
    else {
        return;
    };
    if lock.try_lock_exclusive().is_err() {
        return;
    }
    if owned(&old.work, old.id) {
        let _ = fs::remove_dir_all(&old.work);
    }
    drop(lock);
    if owned(&old.cache, old.id) {
        let _ = fs::remove_dir_all(&old.cache);
    }
}
pub fn startup_error() -> Option<&'static str> {
    STARTUP_ERROR.get().map(String::as_str)
}
pub fn discard(prepared: &Prepared) -> Result<()> {
    let paths = Paths::current()?;
    let (mut plan, _) = load(&prepared.transaction, &paths)?;
    ensure!(
        plan.phase == Phase::Prepared && hash_file(&plan.executable)? == plan.original_hash,
        "Update has already started"
    );
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(plan.cache.join("install.lock"))?;
    lock.try_lock_exclusive()
        .context("Update helper is still active")?;
    plan.phase = Phase::RolledBack;
    plan.save()?;
    clear_pending(&paths, &plan)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (tempfile::TempDir, Plan, Manifest) {
        let root = tempfile::tempdir().unwrap();
        let cache = root.path().join("cache/download-test");
        fs::create_dir_all(&cache).unwrap();
        let target = target().unwrap();
        let destination = root.path().join(if cfg!(target_os = "macos") {
            "Fleab.app"
        } else if cfg!(windows) {
            "fleab.exe"
        } else {
            "fleab"
        });
        let executable = if cfg!(target_os = "macos") {
            destination.join("Contents/MacOS/fleab")
        } else {
            destination.clone()
        };
        fs::create_dir_all(executable.parent().unwrap()).unwrap();
        fs::write(&executable, b"original app").unwrap();
        let id = uuid::Uuid::new_v4();
        let work = root.path().join(format!(".fleab-update-{id}"));
        fs::create_dir(&work).unwrap();
        let plan = Plan {
            schema_version: 1,
            id,
            cache,
            work,
            destination,
            executable,
            original_version: "1.0.0".into(),
            original_hash: hex(&Sha256::digest(b"original app")),
            target: target.clone(),
            envelope: Envelope {
                schema_version: 1,
                key_id: "unused-test-key".into(),
                payload: String::new(),
                signature: String::new(),
            },
            parent: ProcessIdentity {
                pid: 0,
                start_time: 0,
            },
            phase: Phase::Prepared,
        };
        fs::write(
            plan.cache.join("owner.json"),
            serde_json::to_vec(&id).unwrap(),
        )
        .unwrap();
        fs::write(
            plan.work.join("owner.json"),
            serde_json::to_vec(&id).unwrap(),
        )
        .unwrap();
        fs::create_dir_all(plan.replacement_binary().parent().unwrap()).unwrap();
        fs::write(plan.replacement_binary(), b"updated app").unwrap();
        if !cfg!(target_os = "macos") {
            fs::write(plan.backup(), b"original app").unwrap();
        }
        let manifest = Manifest {
            schema_version: 1,
            minimum_updater_version: 1,
            version: "1.1.0".into(),
            assets: [(
                target,
                Asset {
                    filename: "unused-test-archive".into(),
                    size: 1,
                    sha256: "0".repeat(64),
                    unpacked_size: 1,
                    executable_sha256: hex(&Sha256::digest(b"updated app")),
                },
            )]
            .into(),
        };
        fs::create_dir(root.path().join("games")).unwrap();
        fs::write(root.path().join("games/save.dat"), b"game save").unwrap();
        fs::write(root.path().join("profiles.json"), b"profile configuration").unwrap();
        (root, plan, manifest)
    }
    #[test]
    fn application_replacement_and_rollback_preserve_profiles_and_games() {
        let (root, mut plan, manifest) = fixture();
        apply(&mut plan, &manifest).unwrap();
        assert_eq!(fs::read(plan.installed_binary()).unwrap(), b"updated app");
        assert_eq!(fs::read(plan.backup_binary()).unwrap(), b"original app");
        assert_eq!(plan.phase, Phase::Applied);
        restore(&mut plan, &manifest).unwrap();
        assert_eq!(fs::read(plan.installed_binary()).unwrap(), b"original app");
        restore(&mut plan, &manifest).unwrap();
        assert_eq!(
            fs::read(root.path().join("games/save.dat")).unwrap(),
            b"game save"
        );
        assert_eq!(
            fs::read(root.path().join("profiles.json")).unwrap(),
            b"profile configuration"
        );
    }
    #[test]
    fn interrupted_exchange_is_recovered_from_file_identity() {
        let (_root, mut plan, manifest) = fixture();
        plan.phase = Phase::Applying;
        plan.save().unwrap();
        swap(&plan.replacement(), &plan.destination).unwrap();
        // The swap happened, but the Applied journal write did not.
        restore(&mut plan, &manifest).unwrap();
        assert_eq!(fs::read(plan.installed_binary()).unwrap(), b"original app");
    }
    #[test]
    fn changed_application_and_corrupt_backup_are_not_overwritten() {
        let (_root, mut plan, manifest) = fixture();
        fs::write(&plan.executable, b"unrelated replacement").unwrap();
        assert!(apply(&mut plan, &manifest).is_err());
        assert!(restore(&mut plan, &manifest).is_err());
        assert_eq!(
            fs::read(&plan.executable).unwrap(),
            b"unrelated replacement"
        );
        fs::write(&plan.executable, b"original app").unwrap();
        if !cfg!(target_os = "macos") {
            fs::write(plan.backup(), b"corrupt backup").unwrap();
            assert!(apply(&mut plan, &manifest).is_err());
            assert_eq!(fs::read(&plan.executable).unwrap(), b"original app");
        }
    }
    #[test]
    fn transaction_paths_cannot_escape_owned_cache_or_workspace() {
        let (root, mut plan, _manifest) = fixture();
        let paths = Paths {
            root: root.path().join("cache"),
            preferences: root.path().join("updates.json"),
            pending: root.path().join("pending.json"),
            report: root.path().join("report.json"),
        };
        plan.work = root.path().join("games");
        plan.save().unwrap();
        assert!(load(&plan.file(), &paths).is_err());
        assert_eq!(
            fs::read(root.path().join("games/save.dat")).unwrap(),
            b"game save"
        );
    }
}
