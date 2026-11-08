use fleab::archives;
use std::{
    fs,
    path::Path,
    sync::{Arc, atomic::AtomicBool},
};

fn make(root: &Path, cycle: bool) -> std::path::PathBuf {
    let path = root.join("links.tar.zst");
    let mut tar =
        tar::Builder::new(zstd::Encoder::new(fs::File::create(&path).unwrap(), 1).unwrap());
    for name in ["Framework", "Framework/Versions", "Framework/Versions/A"] {
        let mut h = tar::Header::new_ustar();
        h.set_path(name).unwrap();
        h.set_entry_type(tar::EntryType::Directory);
        h.set_size(0);
        h.set_mode(0o755);
        h.set_cksum();
        tar.append(&h, &b""[..]).unwrap();
    }
    let mut h = tar::Header::new_ustar();
    h.set_path("Framework/Versions/A/Binary").unwrap();
    h.set_size(4);
    h.set_mode(0o755);
    h.set_cksum();
    tar.append(&h, &b"data"[..]).unwrap();
    for (name, target) in [
        (
            "Framework/Versions/Current",
            if cycle { "../Binary" } else { "A" },
        ),
        ("Framework/Binary", "Versions/Current/Binary"),
    ] {
        let mut h = tar::Header::new_ustar();
        h.set_path(name).unwrap();
        h.set_link_name(target).unwrap();
        h.set_entry_type(tar::EntryType::Symlink);
        h.set_size(0);
        h.set_mode(0o777);
        h.set_cksum();
        tar.append(&h, &b""[..]).unwrap();
    }
    tar.into_inner().unwrap().finish().unwrap();
    path
}
#[test]
fn symlink_cycles_are_rejected_before_creating_links() {
    let t = tempfile::tempdir_in(std::env::current_dir().unwrap()).unwrap();
    let path = make(t.path(), true);
    let stage = t.path().join("stage");
    fs::create_dir(&stage).unwrap();
    let err = archives::extract(
        &path,
        &stage,
        &Arc::new(AtomicBool::new(false)),
        &mut |_| {},
    )
    .unwrap_err();
    assert!(format!("{err:#}").contains("cycle"));
    assert!(!stage.join("Framework/Binary").exists());
}

#[test]
fn parent_components_are_checked_after_resolving_directory_aliases() {
    let t = tempfile::tempdir_in(std::env::current_dir().unwrap()).unwrap();
    let path = t.path().join("alias-escape.tar.zst");
    let mut tar =
        tar::Builder::new(zstd::Encoder::new(fs::File::create(&path).unwrap(), 1).unwrap());
    for name in ["dir", "top"] {
        let mut h = tar::Header::new_ustar();
        h.set_path(name).unwrap();
        h.set_entry_type(tar::EntryType::Directory);
        h.set_size(0);
        h.set_mode(0o755);
        h.set_cksum();
        tar.append(&h, &b""[..]).unwrap();
    }
    let mut h = tar::Header::new_ustar();
    h.set_path("outside").unwrap();
    h.set_size(4);
    h.set_mode(0o644);
    h.set_cksum();
    tar.append(&h, &b"safe"[..]).unwrap();
    for (name, target) in [("dir/alias", "../top"), ("out", "dir/alias/../../outside")] {
        let mut h = tar::Header::new_ustar();
        h.set_path(name).unwrap();
        h.set_link_name(target).unwrap();
        h.set_entry_type(tar::EntryType::Symlink);
        h.set_size(0);
        h.set_mode(0o777);
        h.set_cksum();
        tar.append(&h, &b""[..]).unwrap();
    }
    tar.into_inner().unwrap().finish().unwrap();
    let stage = t.path().join("stage");
    fs::create_dir(&stage).unwrap();
    let err = archives::extract(
        &path,
        &stage,
        &Arc::new(AtomicBool::new(false)),
        &mut |_| {},
    )
    .unwrap_err();
    assert!(format!("{err:#}").contains("escapes"));
}
#[cfg(unix)]
#[test]
fn macos_framework_aliases_stay_inside_staging() {
    let t = tempfile::tempdir_in(std::env::current_dir().unwrap()).unwrap();
    let path = make(t.path(), false);
    let stage = t.path().join("stage");
    fs::create_dir(&stage).unwrap();
    archives::extract(
        &path,
        &stage,
        &Arc::new(AtomicBool::new(false)),
        &mut |_| {},
    )
    .unwrap();
    assert_eq!(fs::read(stage.join("Framework/Binary")).unwrap(), b"data");
}
