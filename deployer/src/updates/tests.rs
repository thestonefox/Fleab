use super::*;
use ed25519_dalek::{Signer, SigningKey};
use std::{io::Write, sync::atomic::AtomicBool};

fn asset(bytes: &[u8], unpacked: u64, executable: &[u8]) -> Asset {
    Asset {
        filename: "Fleab-1.2.3-update-windows-x64.tar.gz".into(),
        size: bytes.len() as u64,
        sha256: hex(&Sha256::digest(bytes)),
        executable_sha256: hex(&Sha256::digest(executable)),
        unpacked_size: unpacked,
    }
}
fn signed(manifest: &Manifest) -> (Envelope, BTreeMap<String, String>) {
    // Deliberately public test seed, unrelated to the release signing key.
    let key = SigningKey::from_bytes(&[7; 32]);
    let payload = serde_json::to_vec(manifest).unwrap();
    let envelope = Envelope {
        schema_version: 1,
        key_id: "test".into(),
        signature: STANDARD.encode(key.sign(&payload).to_bytes()),
        payload: STANDARD.encode(payload),
    };
    (
        envelope,
        [(
            "test".into(),
            STANDARD.encode(key.verifying_key().to_bytes()),
        )]
        .into(),
    )
}
fn manifest() -> Manifest {
    Manifest {
        schema_version: 1,
        minimum_updater_version: 1,
        version: "1.2.3".into(),
        assets: [("windows-x64".into(), asset(b"archive", 100, b"program"))].into(),
    }
}

#[test]
fn authenticated_feed_rejects_tampering_wrong_keys_and_unsafe_metadata() {
    let (envelope, keys) = signed(&manifest());
    assert_eq!(verify(&envelope, &keys).unwrap().version, "1.2.3");
    let mut tampered = envelope.clone();
    let mut payload = STANDARD.decode(&tampered.payload).unwrap();
    payload[2] ^= 1;
    tampered.payload = STANDARD.encode(payload);
    assert!(verify(&tampered, &keys).is_err());
    let mut wrong = keys.clone();
    wrong.insert("test".into(), STANDARD.encode([0; 32]));
    assert!(verify(&envelope, &wrong).is_err());
    let mut unknown = envelope.clone();
    unknown.key_id = "other".into();
    assert!(verify(&unknown, &keys).is_err());
    for mutation in 0..6 {
        let mut value = manifest();
        match mutation {
            0 => value.version = "1.2.3-beta.1".into(),
            1 => value.minimum_updater_version = 2,
            2 => value.assets.get_mut("windows-x64").unwrap().filename = "../../other.exe".into(),
            3 => value.assets.get_mut("windows-x64").unwrap().size = MAX_DOWNLOAD + 1,
            4 => value.assets.get_mut("windows-x64").unwrap().unpacked_size = MAX_UNPACKED + 1,
            _ => value.assets.get_mut("windows-x64").unwrap().sha256 = "not-a-digest".into(),
        }
        let (envelope, keys) = signed(&value);
        assert!(
            verify(&envelope, &keys).is_err(),
            "accepted unsafe metadata {mutation}"
        );
    }
}

#[test]
fn semver_ordering_and_daily_check_preferences_are_persistent() {
    assert!(stable_version("1.10.0").unwrap() > stable_version("1.9.9").unwrap());
    for invalid in ["1.0.0-beta.1", "1.0.0+build", "01.2.3", "v1.0.0"] {
        assert!(stable_version(invalid).is_err());
    }
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("updates.json");
    let mut preferences = Preferences::load(&path).unwrap();
    assert!(preferences.due(100));
    preferences.last_attempt = 100;
    assert!(!preferences.due(86_499));
    assert!(preferences.due(86_500));
    assert!(preferences.due(50));
    preferences.automatic = false;
    preferences.save(&path).unwrap();
    assert!(!Preferences::load(&path).unwrap().due(1_000_000));
    fs::write(&path, b"{broken}").unwrap();
    assert!(Preferences::load(&path).is_err());
    assert_eq!(fs::read(&path).unwrap(), b"{broken}");
}

#[test]
fn downloads_are_limited_to_https_github_hosts() {
    for valid in [
        "https://api.github.com/repos/thestonefox/Fleab/releases/latest",
        "https://github.com/thestonefox/Fleab/releases/download/v1.2.3/fleab-update.json",
        "https://release-assets.githubusercontent.com/asset?signature=test",
    ] {
        client::allowed_url(valid).unwrap();
    }
    for invalid in [
        "http://github.com/file",
        "https://github.com.evil.test/file",
        "https://github.com@evil.test/file",
        "https://github.com:444/file",
        "file:///etc/passwd",
        "https://example.org/file",
    ] {
        assert!(client::allowed_url(invalid).is_err(), "accepted {invalid}");
    }
}

#[test]
#[ignore = "Requires public GitHub network access"]
fn live_github_release_metadata_is_readable_without_credentials() {
    let candidate = Client::new().unwrap().check("0.0.0").unwrap().unwrap();
    assert!(stable_version(&candidate.version).unwrap() > stable_version("0.0.0").unwrap());
    assert_eq!(candidate.page(), release_url(&candidate.version));
}

fn tar(entries: &[(&str, u8, &[u8])]) -> Vec<u8> {
    let encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    let mut tar = tar::Builder::new(encoder);
    for (name, kind, contents) in entries {
        let mut header = tar::Header::new_ustar();
        header.set_entry_type(tar::EntryType::new(*kind));
        header.set_size(contents.len() as u64);
        header.set_mode(0o644);
        header.set_mtime(0);
        header.set_uid(0);
        header.set_gid(0);
        // Raw headers let the fixture carry paths a safe archive writer rejects.
        header.as_mut_bytes()[..name.len()].copy_from_slice(name.as_bytes());
        header.set_cksum();
        tar.append(&header, *contents).unwrap();
    }
    tar.into_inner().unwrap().finish().unwrap()
}
fn unpack_case(bytes: &[u8], metadata: &Asset, cancel: bool) -> Result<()> {
    let root = tempfile::tempdir()?;
    let archive = root.path().join("update.tar.gz");
    fs::write(&archive, bytes)?;
    fs::write(root.path().join("unrelated-game"), b"preserve")?;
    let result = client::unpack(
        &archive,
        &root.path().join("payload"),
        metadata,
        "windows-x64",
        &AtomicBool::new(cancel),
    );
    assert_eq!(fs::read(root.path().join("unrelated-game"))?, b"preserve");
    result
}
#[test]
fn update_extraction_rejects_links_traversal_duplicates_and_size_bombs() {
    let good = tar(&[
        ("fleab.exe", b'0', b"program"),
        ("LICENSE", b'0', b"license"),
    ]);
    let metadata = asset(&good, 14, b"program");
    unpack_case(&good, &metadata, false).unwrap();
    assert!(unpack_case(&good, &metadata, true).is_err());
    for name in [
        "../outside",
        "/absolute",
        "C:/outside",
        "fleab.exe/../other",
        "unrelated/file",
        "licenses/CON",
    ] {
        let bytes = tar(&[(name, b'0', b"program")]);
        assert!(
            unpack_case(&bytes, &asset(&bytes, 7, b"program"), false).is_err(),
            "accepted {name}"
        );
    }
    for kind in *b"1236xgL" {
        let bytes = tar(&[("fleab.exe", kind, b"")]);
        assert!(unpack_case(&bytes, &asset(&bytes, 1, b""), false).is_err());
    }
    let bytes = tar(&[
        ("fleab.exe", b'0', b"program"),
        ("FLEAB.EXE", b'0', b"program"),
    ]);
    assert!(unpack_case(&bytes, &asset(&bytes, 14, b"program"), false).is_err());
    let bytes = tar(&[("fleab.exe", b'0', &[0; 4096])]);
    assert!(unpack_case(&bytes, &asset(&bytes, 10, &[0; 4096]), false).is_err());
}
#[test]
fn corrupt_compression_after_tar_end_and_wrong_executable_are_rejected() {
    let mut bytes = tar(&[("fleab.exe", b'0', b"program")]);
    let index = bytes.len() - 8;
    bytes[index] ^= 1;
    assert!(unpack_case(&bytes, &asset(&bytes, 7, b"program"), false).is_err());
    let bytes = tar(&[("fleab.exe", b'0', b"program")]);
    assert!(unpack_case(&bytes, &asset(&bytes, 7, b"wrong"), false).is_err());
    let mut corrupted = bytes.clone();
    corrupted[10] ^= 1;
    assert!(unpack_case(&corrupted, &asset(&bytes, 7, b"program"), false).is_err());
    // A second compressed frame after the tar terminator must not hide content.
    let mut extra = bytes.clone();
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(b"hidden content").unwrap();
    extra.extend(encoder.finish().unwrap());
    assert!(unpack_case(&extra, &asset(&extra, 7, b"program"), false).is_err());
}
