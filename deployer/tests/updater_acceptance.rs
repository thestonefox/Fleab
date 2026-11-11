//! Exercises real GUI restart/health acknowledgement only in disposable Sandbox.
#![cfg(windows)]
use fleab::{
    profiles::{Config, Profile},
    updates::{self, Downloaded, Envelope, Preferences, install},
};
use std::{
    fs,
    path::Path,
    process::{Child, Command},
    time::{Duration, Instant},
};

fn running(executable: &Path) -> Vec<sysinfo::Pid> {
    let expected = fs::canonicalize(executable).unwrap();
    let system = sysinfo::System::new_all();
    system
        .processes()
        .iter()
        .filter_map(|(pid, p)| {
            p.exe()
                .and_then(|p| fs::canonicalize(p).ok())
                .filter(|p| p == &expected)
                .map(|_| *pid)
        })
        .collect()
}
fn stop(executable: &Path) {
    let system = sysinfo::System::new_all();
    for pid in running(executable) {
        if let Some(process) = system.process(pid) {
            process.kill();
        }
    }
}
fn wait(mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(80);
    while !condition() {
        assert!(Instant::now() < deadline, "Updater acceptance timed out");
        std::thread::sleep(Duration::from_millis(200));
    }
}
fn case(root: &Path, name: &str, success: bool) {
    let progress = |phase: &str| {
        fs::write(
            Path::new("C:/FleabResults").join(format!("{name}-progress.txt")),
            phase,
        )
        .unwrap();
    };
    progress("Preparing original installation");
    let paths = updates::Paths::current().unwrap();
    assert!(!paths.pending.exists());
    Preferences {
        automatic: false,
        ..Default::default()
    }
    .save(&paths.preferences)
    .unwrap();
    let app = root.join(name);
    fs::create_dir(&app).unwrap();
    let executable = app.join("fleab.exe");
    fs::copy("C:/FleabInput/fleab.exe", &executable).unwrap();
    let game = root.join(format!("{name}-game"));
    fs::create_dir(&game).unwrap();
    fs::write(game.join("save.dat"), b"keep this saved game").unwrap();
    let source = root.join(format!("{name}-source"));
    fs::create_dir(&source).unwrap();
    let mut profile = Profile {
        name: "Updater acceptance".into(),
        source: source.to_string_lossy().into(),
        ..Default::default()
    };
    profile.destinations.insert(
        fleab::archives::Target::WindowsClient,
        game.to_string_lossy().into(),
    );
    let config = Config {
        schema_version: 1,
        selected_profile: Some(profile.id),
        profiles: vec![profile],
    };
    let config_path = fleab::profiles::config_path().unwrap();
    config.save(&config_path).unwrap();
    let original_profiles = fs::read(&config_path).unwrap();
    let original_hash = updates::hash_file(&executable).unwrap();
    progress("Starting original app");
    let mut parent: Child = Command::new(&executable).spawn().unwrap();
    std::thread::sleep(Duration::from_secs(2));
    assert!(parent.try_wait().unwrap().is_none());
    let fixture = Path::new("C:/FleabInput/fixtures").join(name);
    let envelope: Envelope =
        serde_json::from_slice(&fs::read(fixture.join("fleab-update.json")).unwrap()).unwrap();
    let manifest = updates::verify(&envelope, &updates::production_keys().unwrap()).unwrap();
    let expected_hash = manifest.assets[&updates::target().unwrap()]
        .executable_sha256
        .clone();
    progress("Verifying signed package");
    let downloaded =
        Downloaded::from_file(envelope, &fixture.join("update.tar.gz"), &paths).unwrap();
    progress("Preparing replacement and backup");
    let prepared = install::prepare(
        downloaded,
        &executable,
        install::ProcessIdentity::from_pid(parent.id()).unwrap(),
        &config,
        &paths,
    )
    .unwrap();
    progress("Starting update helper");
    install::start(&prepared).unwrap();
    progress("Closing original app");
    parent.kill().unwrap();
    parent.wait().unwrap();
    progress("Waiting for update or rollback and restarted app");
    wait(|| {
        let report = install::last_report(&paths);
        !paths.pending.exists()
            && !running(&executable).is_empty()
            && report.is_some_and(|r| {
                if success {
                    r.message.starts_with("Updated to")
                } else {
                    r.message.starts_with("Update rolled back")
                }
            })
    });
    assert_eq!(fs::read(&config_path).unwrap(), original_profiles);
    assert_eq!(
        fs::read(game.join("save.dat")).unwrap(),
        b"keep this saved game"
    );
    assert_eq!(
        updates::hash_file(&executable).unwrap(),
        if success {
            expected_hash
        } else {
            original_hash
        }
    );
    if success {
        assert!(
            install::last_report(&paths)
                .unwrap()
                .backup
                .unwrap()
                .exists()
        );
    }
    progress("Stopping acceptance app");
    stop(&executable);
    wait(|| running(&executable).is_empty());
    fs::write(
        Path::new("C:/FleabResults").join(format!("{name}-acceptance.json")),
        serde_json::to_vec_pretty(&serde_json::json!({
            "passed": true,
            "updated": success,
            "profiles_preserved": true,
            "game_save_preserved": true,
            "report": install::last_report(&paths),
        }))
        .unwrap(),
    )
    .unwrap();
    progress("Passed");
}

#[test]
#[ignore = "Runs actual desktop app processes; Windows Sandbox only"]
fn signed_update_restarts_and_failed_startup_rolls_back() {
    assert_eq!(
        std::env::var("USERNAME").unwrap(),
        "WDAGUtilityAccount",
        "Never run this GUI test on the host desktop"
    );
    fs::create_dir_all("C:/FleabWork").unwrap();
    let root = tempfile::Builder::new()
        .prefix("updater-acceptance-")
        .tempdir_in("C:/FleabWork")
        .unwrap();
    case(root.path(), "good", true);
    case(root.path(), "broken", false);
}
