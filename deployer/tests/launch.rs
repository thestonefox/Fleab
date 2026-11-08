use fleab::{
    archives::Target,
    deployment::{Installed, MARKER},
    launching,
    profiles::{Profile, RunnerKind},
};
use std::{fs, path::Path};
use uuid::Uuid;

fn setup(root: &Path) -> Profile {
    let dest = root.join("installed");
    fs::create_dir(&dest).unwrap();
    fs::write(dest.join("Game.exe"), b"MZ\0\0").unwrap();
    fs::create_dir(dest.join("Game_Data")).unwrap();
    let mut p = Profile::default();
    p.destinations
        .insert(Target::WindowsClient, dest.to_string_lossy().into());
    let record = Installed {
        schema_version: 1,
        profile_id: p.id,
        target: Target::WindowsClient,
        filename: format!(
            "Game_OLD_20261108-090000_{}.tar.zst",
            if cfg!(target_arch = "aarch64") {
                "WindowsARM64"
            } else {
                "Windows64"
            }
        ),
        entrypoint: "Game.exe".into(),
        transaction: Uuid::new_v4(),
    };
    fs::write(dest.join(MARKER), serde_json::to_vec(&record).unwrap()).unwrap();
    p
}
#[test]
fn detection_is_unambiguous_and_overrides_stay_inside_installation() {
    let t = tempfile::tempdir_in(std::env::current_dir().unwrap()).unwrap();
    let p = setup(t.path());
    let dest = p.destination(Target::WindowsClient).unwrap();
    assert_eq!(
        launching::detect(dest, Target::WindowsClient, None).unwrap(),
        Path::new("Game.exe")
    );
    fs::write(dest.join("Second.exe"), b"MZ\0\0").unwrap();
    fs::create_dir(dest.join("Second_Data")).unwrap();
    assert!(launching::detect(dest, Target::WindowsClient, None).is_err());
    assert_eq!(
        launching::detect(dest, Target::WindowsClient, Some("Second.exe")).unwrap(),
        Path::new("Second.exe")
    );
    assert!(launching::detect(dest, Target::WindowsClient, Some("../outside")).is_err());
}
#[test]
fn wine_and_proton_plans_keep_arguments_separate() {
    let t = tempfile::tempdir_in(std::env::current_dir().unwrap()).unwrap();
    let mut p = setup(t.path());
    let runner = t.path().join("runner with spaces");
    fs::write(&runner, b"runner").unwrap();
    p.launch.runner = runner.to_string_lossy().into();
    p.launch.runner_kind = RunnerKind::Wine;
    p.launch.game_arguments = vec![
        "one argument with spaces".into(),
        ";touch danger".into(),
        "--server".into(),
    ];
    let plan = launching::plan(&p, Target::WindowsClient, "linux", &[]).unwrap();
    assert_eq!(plan.arguments.len(), 4);
    assert_eq!(plan.arguments[1], "one argument with spaces");
    assert_eq!(plan.arguments[2], ";touch danger");
    assert!(
        plan.environment
            .contains_key(std::ffi::OsStr::new("WINEPREFIX"))
    );
    assert!(
        plan.program
            .to_string_lossy()
            .contains("runner with spaces")
    );
    p.launch.runner_kind = RunnerKind::Proton;
    p.launch.steam_root = t.path().to_string_lossy().into();
    let plan = launching::plan(&p, Target::WindowsClient, "linux", &[]).unwrap();
    assert_eq!(plan.arguments[0], "run");
    assert!(
        plan.environment
            .contains_key(std::ffi::OsStr::new("STEAM_COMPAT_DATA_PATH"))
    );
    assert!(
        plan.environment
            .contains_key(std::ffi::OsStr::new("STEAM_COMPAT_CLIENT_INSTALL_PATH"))
    );
    p.launch.runner_arguments = vec!["waitforexitandrun".into()];
    assert_eq!(
        launching::plan(&p, Target::WindowsClient, "linux", &[])
            .unwrap()
            .arguments[0],
        "waitforexitandrun"
    );
    assert!(launching::plan(&p, Target::WindowsClient, "macos", &[]).is_err());
    p.launch.runner.clear();
    assert!(launching::plan(&p, Target::WindowsClient, "linux", &[]).is_err());
}
#[test]
fn runner_discovery_reads_external_steam_libraries() {
    let t = tempfile::tempdir_in(std::env::current_dir().unwrap()).unwrap();
    let steam = t.path().join("Steam");
    let external = t.path().join("other library");
    let bin = t.path().join("bin");
    fs::create_dir(&bin).unwrap();
    fs::write(bin.join("wine"), b"wine").unwrap();
    let proton = external.join("steamapps/common/Proton Test");
    fs::create_dir_all(&proton).unwrap();
    fs::write(proton.join("proton"), b"proton").unwrap();
    fs::create_dir_all(steam.join("steamapps")).unwrap();
    let path = external.to_string_lossy().replace('\\', "\\\\");
    fs::write(
        steam.join("steamapps/libraryfolders.vdf"),
        format!("\"libraryfolders\" {{ \"1\" {{ \"path\" \"{path}\" }} }}"),
    )
    .unwrap();
    let list = launching::discover_at(&[steam], &[bin]);
    assert_eq!(list.len(), 2);
    assert!(list.iter().any(|r| r.kind == RunnerKind::Proton));
    assert!(list.iter().any(|r| r.kind == RunnerKind::Wine));
}
#[test]
fn launching_uses_installed_state_and_launch_config_survives_json() {
    let t = tempfile::tempdir_in(std::env::current_dir().unwrap()).unwrap();
    let mut p = setup(t.path());
    p.launch.game_arguments = vec!["--port".into(), "7777".into()];
    p.launch.runner_kind = RunnerKind::Proton;
    p.launch.prefix = "/home/deck/prefix with spaces".into();
    let p: Profile = serde_json::from_slice(&serde_json::to_vec(&p).unwrap()).unwrap();
    assert_eq!(p.launch.prefix, "/home/deck/prefix with spaces");
    let plan = launching::plan(&p, Target::WindowsClient, "windows", &[]).unwrap();
    assert!(plan.program.ends_with("installed/Game.exe"));
    assert_eq!(plan.arguments.len(), 2);
    assert!(
        fleab::deployment::installed(&p, Target::WindowsClient)
            .unwrap()
            .unwrap()
            .filename
            .contains("OLD")
    );
}
#[cfg(windows)]
#[test]
fn native_launch_and_running_process_protection() {
    let t = tempfile::tempdir_in(std::env::current_dir().unwrap()).unwrap();
    let mut p = setup(t.path());
    let dest = p.destination(Target::WindowsClient).unwrap().to_owned();
    fs::copy(
        Path::new(&std::env::var("SystemRoot").unwrap()).join("System32/cmd.exe"),
        dest.join("Game.exe"),
    )
    .unwrap();
    p.launch.game_arguments = vec!["/C".into(), "exit".into(), "0".into()];
    let mut child = launching::plan(&p, Target::WindowsClient, "windows", &[])
        .unwrap()
        .spawn()
        .unwrap();
    assert!(child.wait().unwrap().success());
    let mut child = std::process::Command::new(dest.join("Game.exe"))
        .args(["/C", "ping -n 30 127.0.0.1 > NUL"])
        .spawn()
        .unwrap();
    let result = fleab::platform::ensure_not_running(&fleab::platform::resolved(&dest).unwrap());
    child.kill().unwrap();
    child.wait().unwrap();
    assert!(result.is_err());
}
