use fleab::{
    archives::Build,
    deployment, launching,
    profiles::{Config, Profile},
};
use std::{
    path::Path,
    sync::{Arc, atomic::AtomicBool},
};

#[test]
#[ignore = "Set FLEAB_ACCEPTANCE_SOURCE to a directory of real Publisher archives"]
fn real_cross_platform_archives_deploy_independently() {
    let source = std::env::var("FLEAB_ACCEPTANCE_SOURCE").expect("Set FLEAB_ACCEPTANCE_SOURCE");
    let root = tempfile::tempdir_in(std::env::current_dir().unwrap()).unwrap();
    let mut profile = Profile {
        source,
        ..Profile::default()
    };
    for target in fleab::archives::Target::ALL {
        profile.destinations.insert(
            target,
            root.path()
                .join(format!("{target:?}"))
                .to_string_lossy()
                .into(),
        );
    }
    let config = Config {
        profiles: vec![profile.clone()],
        ..Config::default()
    };
    let mut seen = std::collections::BTreeSet::new();
    for build in fleab::archives::discover(&profile).unwrap() {
        if !seen.insert(build.target) {
            continue;
        }
        deployment::deploy(
            &config,
            &profile,
            &build,
            Arc::new(AtomicBool::new(false)),
            |_| {},
        )
        .unwrap();
        assert_eq!(
            deployment::installed(&profile, build.target)
                .unwrap()
                .unwrap()
                .filename,
            build.filename
        );
    }
    assert!(
        seen.len() >= 3,
        "Provide real Windows, Linux and macOS archives"
    );
    for target in seen {
        assert!(deployment::installed(&profile, target).unwrap().is_some());
    }
}

/// Run with a real Publisher archive. CI's small fixture is independently
/// exercised by core.rs; this test intentionally requires an actual player.
#[test]
#[ignore = "Set FLEAB_ACCEPTANCE_ARCHIVE to a real native Unity player archive"]
fn real_unity_player_deploys_launches_and_blocks_replacement() {
    let archive = std::env::var("FLEAB_ACCEPTANCE_ARCHIVE").expect("Set FLEAB_ACCEPTANCE_ARCHIVE");
    let build = Build::parse(Path::new(&archive)).unwrap();
    assert_eq!(
        build.target.os(),
        std::env::consts::OS,
        "Use a native archive for this host"
    );
    let root = tempfile::tempdir_in(std::env::current_dir().unwrap()).unwrap();
    let mut profile = Profile {
        source: build.path.parent().unwrap().to_string_lossy().into(),
        ..Profile::default()
    };
    profile.destinations.insert(
        build.target,
        root.path().join("installed").to_string_lossy().into(),
    );
    let config = Config {
        profiles: vec![profile.clone()],
        ..Config::default()
    };
    deployment::deploy(
        &config,
        &profile,
        &build,
        Arc::new(AtomicBool::new(false)),
        |_| {},
    )
    .unwrap();
    assert_eq!(
        deployment::installed(&profile, build.target)
            .unwrap()
            .unwrap()
            .filename,
        build.filename
    );
    profile.launch.game_arguments = vec![
        "-batchmode".into(),
        "-nographics".into(),
        "-logFile".into(),
        root.path().join("player.log").to_string_lossy().into(),
    ];
    let mut child = launching::launch(&profile, build.target, &[]).unwrap();
    let root_path = fleab::platform::resolved(profile.destination(build.target).unwrap()).unwrap();
    let blocked = fleab::platform::ensure_not_running(&root_path);
    let _ = child.kill();
    child.wait().unwrap();
    assert!(blocked.is_err(), "Running player must block replacement");
}
