mod extract;
use crate::profiles::Profile;
use anyhow::{Context, Result, bail, ensure};
use chrono::NaiveDateTime;
pub use extract::{extract, safe_relative};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Target {
    WindowsClient,
    LinuxClient,
    MacClient,
    WindowsServer,
    LinuxServer,
    MacServer,
}
impl Target {
    pub const ALL: [Self; 6] = [
        Self::WindowsClient,
        Self::LinuxClient,
        Self::MacClient,
        Self::WindowsServer,
        Self::LinuxServer,
        Self::MacServer,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::WindowsClient => "Windows client",
            Self::LinuxClient => "Linux client",
            Self::MacClient => "macOS client",
            Self::WindowsServer => "Windows server",
            Self::LinuxServer => "Linux server",
            Self::MacServer => "macOS server",
        }
    }
    pub fn os(self) -> &'static str {
        match self {
            Self::WindowsClient | Self::WindowsServer => "windows",
            Self::LinuxClient | Self::LinuxServer => "linux",
            Self::MacClient | Self::MacServer => "macos",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Build {
    pub path: PathBuf,
    pub filename: String,
    pub game: String,
    pub version: String,
    pub timestamp: NaiveDateTime,
    pub platform: String,
    pub target: Target,
}

fn decode(value: &str) -> Result<String> {
    ensure!(!value.is_empty(), "Empty archive token");
    let b = value.as_bytes();
    let mut bytes = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            ensure!(i + 2 < b.len(), "Invalid percent escape");
            let hex = &b[i + 1..i + 3];
            ensure!(
                hex.iter()
                    .all(|c| c.is_ascii_digit() || (b'A'..=b'F').contains(c)),
                "Noncanonical escape"
            );
            bytes.push(u8::from_str_radix(std::str::from_utf8(hex)?, 16)?);
            i += 3;
        } else {
            ensure!(
                b[i].is_ascii_alphanumeric() || b"-.".contains(&b[i]),
                "Unescaped archive token"
            );
            bytes.push(b[i]);
            i += 1;
        }
    }
    let text = String::from_utf8(bytes)?;
    ensure!(
        !text.chars().any(char::is_control),
        "Control in archive name"
    );
    Ok(text)
}

impl Build {
    pub fn parse(path: &Path) -> Result<Self> {
        let filename = path
            .file_name()
            .and_then(|s| s.to_str())
            .context("Invalid filename")?;
        let stem = filename
            .strip_suffix(".tar.zst")
            .context("Not a Fleab archive")?;
        let stem = if let Some((stem, suffix)) = stem.rsplit_once('~') {
            ensure!(
                suffix.len() == 32
                    && suffix
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
                "Invalid collision suffix"
            );
            stem
        } else {
            stem
        };
        let parts: Vec<_> = stem.split('_').collect();
        ensure!(
            parts.len() == 4 || parts.len() == 5 && parts[4] == "Server",
            "Invalid archive grammar"
        );
        let game = decode(parts[0])?;
        let version = decode(parts[1])?;
        ensure!(
            parts[2].len() == 15
                && parts[2].as_bytes()[8] == b'-'
                && parts[2]
                    .bytes()
                    .enumerate()
                    .all(|(i, b)| i == 8 || b.is_ascii_digit()),
            "Invalid UTC timestamp"
        );
        let timestamp = NaiveDateTime::parse_from_str(parts[2], "%Y%m%d-%H%M%S")?;
        let server = parts.len() == 5;
        let target = match (parts[3], server) {
            ("Windows64" | "WindowsARM64", false) => Target::WindowsClient,
            ("Windows64" | "WindowsARM64", true) => Target::WindowsServer,
            ("Linux64" | "LinuxARM64", false) => Target::LinuxClient,
            ("Linux64" | "LinuxARM64", true) => Target::LinuxServer,
            ("macOS" | "macOS64" | "macOSARM64", false) => Target::MacClient,
            ("macOS" | "macOS64" | "macOSARM64", true) => Target::MacServer,
            _ => bail!("Unsupported platform"),
        };
        Ok(Self {
            path: path.to_owned(),
            filename: filename.into(),
            game,
            version,
            timestamp,
            platform: parts[3].into(),
            target,
        })
    }
}

pub fn discover(profile: &Profile) -> Result<Vec<Build>> {
    let mut builds = Vec::new();
    for entry in fs::read_dir(&profile.source)
        .with_context(|| format!("Cannot read archive source {}", profile.source))?
    {
        let entry = entry?;
        if entry.file_type()?.is_file()
            && let Ok(build) = Build::parse(&entry.path())
            && profile.destination(build.target).is_some()
        {
            builds.push(build);
        }
    }
    builds.sort_by(|a, b| {
        b.timestamp
            .cmp(&a.timestamp)
            .then_with(|| a.filename.cmp(&b.filename))
    });
    Ok(builds)
}
