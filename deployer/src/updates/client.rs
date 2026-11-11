use super::*;
use anyhow::bail;
use sha2::Digest;
use std::{
    collections::HashSet,
    io::{Read, Write},
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

#[derive(Clone, Debug)]
pub struct Candidate {
    pub version: String,
    pub notes: String,
    pub envelope: Option<Envelope>,
}
impl Candidate {
    pub fn page(&self) -> String {
        release_url(&self.version)
    }
}
pub struct Downloaded {
    pub directory: tempfile::TempDir,
    pub envelope: Envelope,
    pub manifest: Manifest,
    pub asset: Asset,
    pub target: String,
}
impl Downloaded {
    /// Stages an authenticated local package using the same verifier as HTTPS
    /// downloads. This also supports offline recovery and acceptance tests.
    pub fn from_file(envelope: Envelope, archive: &Path, paths: &Paths) -> Result<Self> {
        let manifest = verify(&envelope, &production_keys()?)?;
        ensure!(
            stable_version(&manifest.version)? > stable_version(VERSION)?,
            "Not a newer update"
        );
        let target = target()?;
        let asset = manifest
            .assets
            .get(&target)
            .context("No update for this device")?
            .clone();
        ensure!(
            fs::metadata(archive)?.len() == asset.size && hash_file(archive)? == asset.sha256,
            "Local update package is corrupt"
        );
        paths.create()?;
        let directory = tempfile::Builder::new()
            .prefix("download-")
            .tempdir_in(&paths.root)?;
        let copy = directory.path().join("update.tar.gz");
        fs::copy(archive, &copy)?;
        unpack(
            &copy,
            &directory.path().join("payload"),
            &asset,
            &target,
            &AtomicBool::new(false),
        )?;
        Ok(Self {
            directory,
            envelope,
            manifest,
            asset,
            target,
        })
    }
}
#[derive(Clone)]
pub struct Client {
    agent: ureq::Agent,
    keys: BTreeMap<String, String>,
}
impl Client {
    pub fn new() -> Result<Self> {
        let agent = ureq::Agent::config_builder()
            .https_only(true)
            .max_redirects(0)
            .max_redirects_will_error(false)
            .http_status_as_error(false)
            .timeout_connect(Some(Duration::from_secs(8)))
            .timeout_per_read(Some(Duration::from_secs(20)))
            .timeout_global(Some(Duration::from_secs(600)))
            .user_agent(format!("Fleab/{VERSION}"))
            .build()
            .into();
        Ok(Self {
            agent,
            keys: production_keys()?,
        })
    }
    fn request(&self, url: &str) -> Result<ureq::http::Response<ureq::Body>> {
        let mut url = url.to_owned();
        for _ in 0..=5 {
            allowed_url(&url)?;
            let response = self
                .agent
                .get(&url)
                .header("Accept", "application/vnd.github+json")
                .call()?;
            if response.status().is_redirection() {
                let location = response
                    .headers()
                    .get("location")
                    .context("Missing redirect location")?
                    .to_str()?;
                url = if location.starts_with('/') && !location.starts_with("//") {
                    let parsed: ureq::http::Uri = url.parse()?;
                    format!(
                        "https://{}{location}",
                        parsed.authority().context("Missing redirect host")?
                    )
                } else {
                    location.to_owned()
                };
            } else {
                ensure!(
                    response.status().as_u16() == 200,
                    "GitHub returned HTTP {}",
                    response.status()
                );
                return Ok(response);
            }
        }
        bail!("Too many update redirects")
    }
    fn metadata(&self, url: &str) -> Result<Vec<u8>> {
        let mut response = self.request(url)?;
        let mut bytes = Vec::new();
        response
            .body_mut()
            .as_reader()
            .take(MAX_MANIFEST + 1)
            .read_to_end(&mut bytes)?;
        ensure!(
            bytes.len() as u64 <= MAX_MANIFEST,
            "Update metadata is too large"
        );
        Ok(bytes)
    }
    pub fn check(&self, current: &str) -> Result<Option<Candidate>> {
        #[derive(Deserialize)]
        struct Release {
            tag_name: String,
            draft: bool,
            prerelease: bool,
            body: Option<String>,
            assets: Vec<ReleaseAsset>,
        }
        #[derive(Deserialize)]
        struct ReleaseAsset {
            name: String,
        }
        let bytes =
            self.metadata("https://api.github.com/repos/thestonefox/Fleab/releases/latest")?;
        let release: Release = serde_json::from_slice(&bytes)?;
        ensure!(
            !release.draft && !release.prerelease,
            "GitHub did not return a stable release"
        );
        let version = release
            .tag_name
            .strip_prefix('v')
            .context("Unexpected release tag")?;
        if stable_version(version)? <= stable_version(current)? {
            return Ok(None);
        }
        let envelope = if release.assets.iter().any(|a| a.name == MANIFEST_NAME) {
            let envelope: Envelope =
                serde_json::from_slice(&self.metadata(&asset_url(version, MANIFEST_NAME))?)?;
            let manifest = verify(&envelope, &self.keys)?;
            ensure!(
                manifest.version == version,
                "Signed update version does not match the release"
            );
            ensure!(
                manifest.assets.contains_key(&target()?),
                "No update package for this device"
            );
            Some(envelope)
        } else {
            None
        };
        Ok(Some(Candidate {
            version: version.into(),
            notes: release.body.unwrap_or_default(),
            envelope,
        }))
    }
    pub fn download(
        &self,
        candidate: &Candidate,
        paths: &Paths,
        cancel: &AtomicBool,
        mut progress: impl FnMut(f32),
    ) -> Result<Downloaded> {
        let envelope = candidate
            .envelope
            .clone()
            .context("This release has no signed update; download it manually")?;
        let manifest = verify(&envelope, &self.keys)?;
        ensure!(
            manifest.version == candidate.version
                && stable_version(&manifest.version)? > stable_version(VERSION)?,
            "Not a newer release"
        );
        let target = target()?;
        let asset = manifest
            .assets
            .get(&target)
            .context("No update for this device")?
            .clone();
        paths.create()?;
        let directory = tempfile::Builder::new()
            .prefix("download-")
            .tempdir_in(&paths.root)?;
        let archive = directory.path().join("update.tar.gz");
        let mut response = self.request(&asset_url(&manifest.version, &asset.filename))?;
        if let Some(size) = response.headers().get("content-length") {
            ensure!(
                size.to_str()?.parse::<u64>()? == asset.size,
                "Unexpected update download size"
            );
        }
        let mut reader = response
            .body_mut()
            .with_config()
            .limit(asset.size + 1)
            .reader();
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&archive)?;
        let mut total = 0_u64;
        let mut hash = Sha256::new();
        let mut buffer = [0; 65_536];
        loop {
            ensure!(!cancel.load(Ordering::Relaxed), "Update cancelled");
            let size = reader.read(&mut buffer)?;
            if size == 0 {
                break;
            }
            total += size as u64;
            ensure!(
                total <= asset.size,
                "Update download exceeds its signed size"
            );
            file.write_all(&buffer[..size])?;
            hash.update(&buffer[..size]);
            progress(total as f32 / asset.size as f32);
        }
        file.sync_all()?;
        ensure!(
            total == asset.size && hex(&hash.finalize()) == asset.sha256,
            "Update download is incomplete or corrupt"
        );
        let payload = directory.path().join("payload");
        unpack(&archive, &payload, &asset, &target, cancel)?;
        Ok(Downloaded {
            directory,
            envelope,
            manifest,
            asset,
            target,
        })
    }
}

pub(super) fn allowed_url(url: &str) -> Result<()> {
    let parsed: ureq::http::Uri = url.parse()?;
    ensure!(
        parsed.scheme_str() == Some("https") && parsed.port_u16().is_none_or(|p| p == 443),
        "Update URL must use HTTPS"
    );
    ensure!(
        !parsed
            .authority()
            .context("Missing host")?
            .as_str()
            .contains('@'),
        "Invalid update URL"
    );
    ensure!(
        [
            "api.github.com",
            "github.com",
            "release-assets.githubusercontent.com",
            "objects.githubusercontent.com",
            "github-releases.githubusercontent.com"
        ]
        .contains(&parsed.host().unwrap_or("")),
        "Unexpected update host"
    );
    Ok(())
}

pub(super) fn executable_relative(target: &str) -> &'static str {
    if target.starts_with("windows-") {
        "fleab.exe"
    } else if target.starts_with("macos-") {
        "Fleab.app/Contents/MacOS/fleab"
    } else {
        "fleab"
    }
}
pub(super) fn unpack(
    archive: &Path,
    destination: &Path,
    asset: &Asset,
    target: &str,
    cancel: &AtomicBool,
) -> Result<()> {
    ensure!(
        hash_file(archive)? == asset.sha256 && fs::metadata(archive)?.len() == asset.size,
        "Update archive is corrupt"
    );
    fs::create_dir(destination)?;
    let mut decoder = flate2::read::MultiGzDecoder::new(fs::File::open(archive)?);
    let mut total = 0_u64;
    let mut count = 0;
    let mut names = HashSet::new();
    {
        let mut tar = tar::Archive::new(&mut decoder);
        for entry in tar.entries()?.raw(true) {
            ensure!(!cancel.load(Ordering::Relaxed), "Update cancelled");
            count += 1;
            ensure!(count <= 50_000, "Too many update files");
            let mut entry = entry?;
            let kind = entry.header().entry_type();
            ensure!(
                kind.is_file() || kind.is_dir(),
                "Update archives cannot contain links or special entries"
            );
            let name = entry
                .path()?
                .to_str()
                .context("Non-UTF-8 update path")?
                .trim_end_matches('/')
                .to_owned();
            let relative = crate::archives::safe_relative(&name)?;
            ensure!(names.insert(name.to_lowercase()), "Duplicate update path");
            let first = relative
                .components()
                .next()
                .context("Empty update path")?
                .as_os_str();
            let app = if target.starts_with("macos-") {
                "Fleab.app"
            } else {
                executable_relative(target)
            };
            ensure!(
                [app, "LICENSE", "THIRD_PARTY_NOTICES.md", "licenses"]
                    .iter()
                    .any(|n| first == std::ffi::OsStr::new(n)),
                "Unexpected file in update archive"
            );
            let path = destination.join(relative);
            if kind.is_dir() {
                ensure!(entry.size() == 0, "Directory entry has unexpected data");
                fs::create_dir_all(&path)?;
                continue;
            }
            let size = entry.size();
            total = total.checked_add(size).context("Update size overflow")?;
            ensure!(
                total <= asset.unpacked_size && total <= MAX_UNPACKED,
                "Update extraction exceeds its signed size"
            );
            fs::create_dir_all(path.parent().context("Invalid update path")?)?;
            let mut file = fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&path)?;
            let mut copied = 0;
            let mut buffer = [0; 65_536];
            loop {
                ensure!(!cancel.load(Ordering::Relaxed), "Update cancelled");
                let read = entry.read(&mut buffer)?;
                if read == 0 {
                    break;
                }
                file.write_all(&buffer[..read])?;
                copied += read as u64;
            }
            ensure!(copied == size, "Truncated update file");
            file.sync_all()?;
        }
    }
    let mut tail = [0; 4096];
    let mut padding = 0;
    loop {
        let size = decoder.read(&mut tail)?;
        if size == 0 {
            break;
        }
        padding += size;
        ensure!(
            padding <= 20_480 && tail[..size].iter().all(|b| *b == 0),
            "Unexpected trailing update data"
        );
    }
    ensure!(
        total == asset.unpacked_size,
        "Update extracted size does not match its signed metadata"
    );
    let executable = destination.join(executable_relative(target));
    ensure!(
        hash_file(&executable)? == asset.executable_sha256,
        "Update executable digest does not match"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o755))?;
    }
    Ok(())
}
