use fleab::{
    archives::{self, Build, Target},
    deployment::{self, MARKER},
    profiles::{Config, Profile},
};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

fn temp() -> tempfile::TempDir {
    tempfile::tempdir_in(std::env::current_dir().unwrap()).unwrap()
}
fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../tests/fixtures/Interop%5FGame_1.2%5F%CE%B2_20261108-093000_Windows64.tar.zst")
}
fn profile(root: &Path) -> Profile {
    let source = root.join("source");
    fs::create_dir_all(&source).unwrap();
    let mut p = Profile {
        source: source.to_string_lossy().into(),
        ..Profile::default()
    };
    p.destinations.insert(
        Target::WindowsClient,
        root.join("windows").to_string_lossy().into(),
    );
    p
}
fn config(p: &Profile) -> Config {
    Config {
        profiles: vec![p.clone()],
        ..Config::default()
    }
}
fn copy_fixture(p: &Profile, timestamp: &str) -> Build {
    let name = format!("Game_1_{timestamp}_Windows64.tar.zst");
    let path = Path::new(&p.source).join(name);
    fs::copy(fixture(), &path).unwrap();
    Build::parse(&path).unwrap()
}

#[test]
fn grammar_decodes_names_and_targets() {
    assert!(Build::parse(Path::new("%Fé_1_20261108-093000_Linux64.tar.zst")).is_err());
    let b = Build::parse(&fixture()).unwrap();
    assert_eq!(b.game, "Interop_Game");
    assert_eq!(b.version, "1.2_β");
    assert_eq!(b.target, Target::WindowsClient);
    for (os, target) in [
        ("WindowsARM64", Target::WindowsServer),
        ("LinuxARM64", Target::LinuxServer),
        ("macOS", Target::MacServer),
        ("macOS64", Target::MacServer),
        ("macOSARM64", Target::MacServer),
    ] {
        let name =
            format!("Game_1_20261108-093000_{os}_Server~0123456789abcdef0123456789abcdef.tar.zst");
        assert_eq!(Build::parse(Path::new(&name)).unwrap().target, target);
    }
    for name in [
        "Game_1_20260230-000000_Linux64.tar.zst",
        "Game_1_20261108-093000_Android.tar.zst",
        "Game_bad_version_20261108-093000_Linux64.tar.zst",
        "Game_1_20261108-093000_Linux64.tar.zst.partial",
        "%FF_1_20261108-093000_Linux64.tar.zst",
    ] {
        assert!(Build::parse(Path::new(name)).is_err(), "{name}");
    }
}

#[test]
fn discovery_filters_destinations_and_sorts_utc() {
    let root = temp();
    let mut p = profile(root.path());
    copy_fixture(&p, "20261108-090000");
    copy_fixture(&p, "20261108-100000");
    fs::copy(
        fixture(),
        Path::new(&p.source).join("Other_1_20261108-110000_LinuxARM64.tar.zst"),
    )
    .unwrap();
    fs::copy(
        fixture(),
        Path::new(&p.source).join("Other_1_20261108-120000_Linux64_Server.tar.zst"),
    )
    .unwrap();
    let list = archives::discover(&p).unwrap();
    assert_eq!(list.len(), 2);
    assert!(list[0].filename.contains("100000"));
    p.destinations.insert(
        Target::LinuxClient,
        root.path().join("linux").to_string_lossy().into(),
    );
    let list = archives::discover(&p).unwrap();
    assert_eq!(list.len(), 3);
    assert_eq!(list[0].platform, "LinuxARM64");
    p.destinations.insert(Target::LinuxServer, String::new());
    assert_eq!(archives::discover(&p).unwrap().len(), 3);
}

#[test]
fn profile_crud_storage_import_and_schema() {
    let root = temp();
    let mut p = profile(root.path());
    let path = root.path().join("profiles.json");
    let mut c = config(&p);
    c.selected_profile = Some(p.id);
    c.save(&path).unwrap();
    assert_eq!(Config::load(&path).unwrap().selected_profile, Some(p.id));
    p.name = "Changed".into();
    c.profiles[0] = p.clone();
    c.save(&path).unwrap();
    assert_eq!(Config::load(&path).unwrap().profiles[0].name, "Changed");
    let export = root.path().join("profile.json");
    Config::export_profile(&p, &export).unwrap();
    let imported = Config::import_profile(&export).unwrap();
    assert_ne!(p.id, imported.id);
    assert_eq!(p.source, imported.source);
    assert!(!fs::read_to_string(export).unwrap().contains("installed"));
    c.profiles.clear();
    c.save(&path).unwrap();
    assert!(Config::load(&path).unwrap().profiles.is_empty());
    fs::write(&path, b"invalid").unwrap();
    assert!(Config::load(&path).is_err());
    assert_eq!(fs::read(&path).unwrap(), b"invalid");
    fs::write(
        &path,
        br#"{"schema_version":99,"profiles":[],"selected_profile":null}"#,
    )
    .unwrap();
    assert!(Config::load(&path).is_err());
}

#[test]
fn conflicting_destinations_and_sources_rejected() {
    let root = temp();
    let mut p = profile(root.path());
    let mut c = config(&p);
    c.validate_paths().unwrap();
    p.destinations.insert(
        Target::LinuxClient,
        root.path().join("windows/nested").to_string_lossy().into(),
    );
    c.profiles[0] = p.clone();
    assert!(c.validate_paths().is_err());
    p.destinations.remove(&Target::LinuxClient);
    p.source = root.path().to_string_lossy().into();
    c.profiles[0] = p;
    assert!(c.validate_paths().is_err());
    assert!(
        fleab::platform::safe_destination(Path::new(if cfg!(windows) { "C:\\" } else { "/" }))
            .is_err()
    );
    assert!(
        fleab::platform::safe_destination(directories::BaseDirs::new().unwrap().home_dir())
            .is_err()
    );
}

#[test]
fn publisher_archive_deploys_and_replaces_atomically() {
    let root = temp();
    let p = profile(root.path());
    let c = config(&p);
    let first = copy_fixture(&p, "20261108-090000");
    let cancel = Arc::new(AtomicBool::new(false));
    let installed = deployment::deploy(&c, &p, &first, cancel.clone(), |_| {}).unwrap();
    assert_eq!(installed.entrypoint, Path::new("Game.exe"));
    let dest = p.destination(Target::WindowsClient).unwrap();
    assert!(dest.join("Game.exe").is_file());
    assert!(!dest.join("unrelated.txt").exists());
    assert!(
        dest.join("Game_Data").read_dir().unwrap().any(|e| e
            .unwrap()
            .file_name()
            .to_string_lossy()
            .contains('ü'))
    );
    let second = copy_fixture(&p, "20261108-100000");
    fs::write(dest.join("stale-file"), b"old").unwrap();
    deployment::deploy(&c, &p, &second, cancel, |_| {}).unwrap();
    assert!(!dest.join("stale-file").exists());
    assert_eq!(
        deployment::installed(&p, first.target)
            .unwrap()
            .unwrap()
            .filename,
        second.filename
    );
    assert!(dest.join(MARKER).exists());
    assert!(!fs::read_dir(root.path()).unwrap().any(|e| {
        e.unwrap()
            .file_name()
            .to_string_lossy()
            .ends_with("-backup")
    }));
}

#[test]
fn failed_and_cancelled_deployments_preserve_installation() {
    let root = temp();
    let p = profile(root.path());
    let c = config(&p);
    let first = copy_fixture(&p, "20261108-090000");
    deployment::deploy(&c, &p, &first, Arc::new(AtomicBool::new(false)), |_| {}).unwrap();
    let second = copy_fixture(&p, "20261108-100000");
    let cancel = Arc::new(AtomicBool::new(false));
    let flag = cancel.clone();
    assert!(
        deployment::deploy(&c, &p, &second, cancel, |progress| {
            if progress.phase.contains("extracting") {
                flag.store(true, Ordering::Relaxed);
            }
        })
        .is_err()
    );
    assert_eq!(
        deployment::installed(&p, first.target)
            .unwrap()
            .unwrap()
            .filename,
        first.filename
    );
    fs::write(&second.path, b"corrupt archive").unwrap();
    assert!(deployment::deploy(&c, &p, &second, Arc::new(AtomicBool::new(false)), |_| {}).is_err());
    assert_eq!(
        deployment::installed(&p, first.target)
            .unwrap()
            .unwrap()
            .filename,
        first.filename
    );
    assert!(
        !fs::read_dir(root.path()).unwrap().any(|e| e
            .unwrap()
            .file_name()
            .to_string_lossy()
            .ends_with("-stage"))
    );
}

#[test]
fn unrelated_directory_is_never_replaced() {
    let root = temp();
    let p = profile(root.path());
    let b = copy_fixture(&p, "20261108-090000");
    let dest = p.destination(b.target).unwrap();
    fs::create_dir(dest).unwrap();
    fs::write(dest.join("important"), b"keep").unwrap();
    assert!(
        deployment::deploy(
            &config(&p),
            &p,
            &b,
            Arc::new(AtomicBool::new(false)),
            |_| {}
        )
        .is_err()
    );
    assert_eq!(fs::read(dest.join("important")).unwrap(), b"keep");
}

fn malicious(root: &Path, name: &str, kind: u8, target: &str) -> PathBuf {
    let path = root.join("attack.tar.zst");
    let file = fs::File::create(&path).unwrap();
    let encoder = zstd::Encoder::new(file, 1).unwrap();
    let mut tar = tar::Builder::new(encoder);
    let mut header = tar::Header::new_ustar();
    header.set_mode(0o755);
    header.set_size(if kind == b'0' { 4 } else { 0 });
    header.set_entry_type(tar::EntryType::new(kind));
    // Deliberately bypass the tar crate's safe path setters.
    header.as_mut_bytes()[..name.len()].copy_from_slice(name.as_bytes());
    if !target.is_empty() {
        header.as_mut_bytes()[157..157 + target.len()].copy_from_slice(target.as_bytes());
    }
    header.set_cksum();
    tar.append(&header, if kind == b'0' { &b"data"[..] } else { &b""[..] })
        .unwrap();
    tar.into_inner().unwrap().finish().unwrap();
    path
}
#[test]
fn malicious_archives_are_rejected() {
    let root = temp();
    for (i, (name, kind, target)) in [
        ("../outside", b'0', ""),
        ("/absolute", b'0', ""),
        ("C:/drive", b'0', ""),
        ("foo\\bar", b'0', ""),
        ("CON.txt", b'0', ""),
        ("escape", b'2', "../../outside"),
        ("hard", b'1', "somewhere"),
        ("device", b'3', ""),
        (".fleab-installed.json", b'0', ""),
    ]
    .into_iter()
    .enumerate()
    {
        let archive = malicious(root.path(), name, kind, target);
        let stage = root.path().join(format!("stage{i}"));
        fs::create_dir(&stage).unwrap();
        assert!(
            archives::extract(
                &archive,
                &stage,
                &Arc::new(AtomicBool::new(false)),
                &mut |_| {}
            )
            .is_err(),
            "{name}"
        );
    }
    assert!(!root.path().join("outside").exists());
}

#[test]
fn truncated_frames_are_rejected_even_after_tar_end() {
    let root = temp();
    let path = root.path().join("truncated.tar.zst");
    let mut data = fs::read(fixture()).unwrap();
    data.truncate(data.len() - 3);
    fs::write(&path, data).unwrap();
    let stage = root.path().join("stage");
    fs::create_dir(&stage).unwrap();
    assert!(
        archives::extract(
            &path,
            &stage,
            &Arc::new(AtomicBool::new(false)),
            &mut |_| {}
        )
        .is_err()
    );
}

#[cfg(unix)]
#[test]
fn modes_and_internal_symlinks_survive() {
    use std::os::unix::fs::PermissionsExt;
    let root = temp();
    let path = root.path().join("unix.tar.zst");
    let mut tar =
        tar::Builder::new(zstd::Encoder::new(fs::File::create(&path).unwrap(), 1).unwrap());
    let mut h = tar::Header::new_ustar();
    h.set_path("Game.x86_64").unwrap();
    h.set_size(4);
    h.set_mode(0o755);
    h.set_cksum();
    tar.append(&h, &b"\x7fELF"[..]).unwrap();
    let mut h = tar::Header::new_ustar();
    h.set_path("link").unwrap();
    h.set_link_name("Game.x86_64").unwrap();
    h.set_size(0);
    h.set_mode(0o777);
    h.set_entry_type(tar::EntryType::Symlink);
    h.set_cksum();
    tar.append(&h, &b""[..]).unwrap();
    tar.into_inner().unwrap().finish().unwrap();
    let stage = root.path().join("stage");
    fs::create_dir(&stage).unwrap();
    archives::extract(
        &path,
        &stage,
        &Arc::new(AtomicBool::new(false)),
        &mut |_| {},
    )
    .unwrap();
    assert_eq!(
        fs::metadata(stage.join("Game.x86_64"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o755
    );
    assert_eq!(
        fs::read_link(stage.join("link")).unwrap(),
        Path::new("Game.x86_64")
    );
}

#[test]
fn source_unavailable_preserves_profile_and_installed_state() {
    let root = temp();
    let p = profile(root.path());
    let b = copy_fixture(&p, "20261108-090000");
    deployment::deploy(
        &config(&p),
        &p,
        &b,
        Arc::new(AtomicBool::new(false)),
        |_| {},
    )
    .unwrap();
    fs::rename(&p.source, root.path().join("disconnected")).unwrap();
    assert!(archives::discover(&p).is_err());
    assert!(deployment::installed(&p, b.target).unwrap().is_some());
}
