mod client;
pub mod install;
#[cfg(test)]
mod tests;
use anyhow::{Context, Result, ensure};
use base64::{Engine, engine::general_purpose::STANDARD};
pub use client::{Candidate, Client, Downloaded};
use ed25519_dalek::{Signature, VerifyingKey};
use semver::Version;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::Read,
    path::{Path, PathBuf},
};

pub const REPOSITORY: &str = "https://github.com/thestonefox/Fleab";
pub const MANIFEST_NAME: &str = "fleab-update.json";
pub const MAX_MANIFEST: u64 = 1_048_576;
pub const MAX_DOWNLOAD: u64 = 536_870_912;
pub const MAX_UNPACKED: u64 = 2_147_483_648;
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Clone, Debug)]
pub struct Paths {
    pub root: PathBuf,
    pub preferences: PathBuf,
    pub pending: PathBuf,
    pub report: PathBuf,
}
impl Paths {
    pub fn current() -> Result<Self> {
        let dirs = directories::ProjectDirs::from("", "", "Fleab")
            .context("Cannot locate update storage")?;
        let config = dirs.config_local_dir();
        Ok(Self {
            root: dirs.cache_dir().join("updates"),
            preferences: config.join("updates.json"),
            pending: config.join("update-pending.json"),
            report: config.join("update-report.json"),
        })
    }
    pub fn create(&self) -> Result<()> {
        fs::create_dir_all(&self.root)?;
        let metadata = fs::symlink_metadata(&self.root)?;
        ensure!(
            metadata.is_dir() && !crate::platform::is_link(&metadata),
            "Update cache cannot be a link or junction"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&self.root, fs::Permissions::from_mode(0o700))?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Preferences {
    pub schema_version: u32,
    pub automatic: bool,
    pub last_attempt: u64,
    pub last_checked: u64,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            schema_version: 1,
            automatic: true,
            last_attempt: 0,
            last_checked: 0,
        }
    }
}
impl Preferences {
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let value: Self = serde_json::from_reader(fs::File::open(path)?.take(65_537))
            .context("Update settings are malformed; file preserved")?;
        ensure!(
            value.schema_version == 1,
            "Unsupported update settings version; file preserved"
        );
        Ok(value)
    }
    pub fn save(&self, path: &Path) -> Result<()> {
        crate::platform::atomic_json(path, self)
    }
    pub fn due(&self, now: u64) -> bool {
        self.automatic
            && (self.last_attempt == 0
                || now < self.last_attempt
                || now.saturating_sub(self.last_attempt) >= 86_400)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Envelope {
    pub schema_version: u32,
    pub key_id: String,
    pub payload: String,
    pub signature: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub schema_version: u32,
    pub minimum_updater_version: u32,
    pub version: String,
    pub assets: BTreeMap<String, Asset>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Asset {
    pub filename: String,
    pub size: u64,
    pub sha256: String,
    pub executable_sha256: String,
    pub unpacked_size: u64,
}

pub fn target() -> Result<String> {
    let arch = match std::env::consts::ARCH {
        "x86_64" => "x64",
        "aarch64" => "arm64",
        _ => anyhow::bail!("No updater package for this CPU"),
    };
    ensure!(
        ["windows", "linux", "macos"].contains(&std::env::consts::OS),
        "No updater for this OS"
    );
    Ok(format!("{}-{arch}", std::env::consts::OS))
}
pub fn stable_version(value: &str) -> Result<Version> {
    let version = Version::parse(value)?;
    ensure!(
        version.pre.is_empty() && version.build.is_empty(),
        "Not a stable release version"
    );
    Ok(version)
}
pub fn release_url(version: &str) -> String {
    format!("{REPOSITORY}/releases/tag/v{version}")
}
pub fn asset_url(version: &str, filename: &str) -> String {
    format!("{REPOSITORY}/releases/download/v{version}/{filename}")
}
pub fn production_keys() -> Result<BTreeMap<String, String>> {
    Ok(serde_json::from_str(include_str!(
        "../../update-keys.json"
    ))?)
}

pub fn verify(envelope: &Envelope, keys: &BTreeMap<String, String>) -> Result<Manifest> {
    ensure!(
        envelope.schema_version == 1 && envelope.payload.len() <= MAX_MANIFEST as usize,
        "Unsupported or oversized update envelope"
    );
    let encoded_key = keys
        .get(&envelope.key_id)
        .context("Unknown update signing key")?;
    let bytes: [u8; 32] = STANDARD
        .decode(encoded_key)?
        .try_into()
        .map_err(|_| anyhow::anyhow!("Invalid update public key"))?;
    let key = VerifyingKey::from_bytes(&bytes)?;
    let payload = STANDARD.decode(&envelope.payload)?;
    let signature = Signature::try_from(STANDARD.decode(&envelope.signature)?.as_slice())?;
    key.verify_strict(&payload, &signature)
        .context("Update signature is invalid")?;
    let manifest: Manifest = serde_json::from_slice(&payload)?;
    ensure!(
        manifest.schema_version == 1 && manifest.minimum_updater_version <= 1,
        "This release needs a newer updater; download it manually"
    );
    stable_version(&manifest.version)?;
    ensure!(
        !manifest.assets.is_empty() && manifest.assets.len() <= 6,
        "Invalid update target list"
    );
    for (target, asset) in &manifest.assets {
        ensure!(
            [
                "windows-x64",
                "windows-arm64",
                "linux-x64",
                "linux-arm64",
                "macos-x64",
                "macos-arm64"
            ]
            .contains(&target.as_str()),
            "Unknown update target"
        );
        ensure!(
            asset.filename == format!("Fleab-{}-update-{target}.tar.gz", manifest.version),
            "Unexpected update filename"
        );
        ensure!(
            asset.size > 0
                && asset.size <= MAX_DOWNLOAD
                && asset.unpacked_size > 0
                && asset.unpacked_size <= MAX_UNPACKED,
            "Update exceeds size limits"
        );
        ensure!(
            valid_hash(&asset.sha256) && valid_hash(&asset.executable_sha256),
            "Invalid update digest"
        );
    }
    Ok(manifest)
}
pub fn valid_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
pub fn hash_file(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path)?;
    let mut hash = Sha256::new();
    let mut buffer = [0; 65_536];
    loop {
        let size = file.read(&mut buffer)?;
        if size == 0 {
            break;
        }
        hash.update(&buffer[..size]);
    }
    Ok(hex(&hash.finalize()))
}
pub(super) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
