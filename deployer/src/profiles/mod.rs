use crate::{archives::Target, platform};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};
use uuid::Uuid;

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct LaunchConfig {
    pub runner_kind: RunnerKind,
    pub runner: String,
    pub runner_arguments: Vec<String>,
    pub game_arguments: Vec<String>,
    pub prefix: String,
    pub steam_root: String,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum RunnerKind {
    #[default]
    Auto,
    Wine,
    Proton,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    pub id: Uuid,
    pub name: String,
    pub source: String,
    #[serde(default)]
    pub destinations: BTreeMap<Target, String>,
    #[serde(default)]
    pub executable_overrides: BTreeMap<Target, String>,
    #[serde(default)]
    pub launch: LaunchConfig,
}

impl Default for Profile {
    fn default() -> Self {
        Self {
            id: Uuid::new_v4(),
            name: "New game".into(),
            source: String::new(),
            destinations: BTreeMap::new(),
            executable_overrides: BTreeMap::new(),
            launch: LaunchConfig::default(),
        }
    }
}
impl Profile {
    pub fn destination(&self, target: Target) -> Option<&Path> {
        self.destinations
            .get(&target)
            .filter(|v| !v.trim().is_empty())
            .map(Path::new)
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(!self.name.trim().is_empty(), "Profile name is required");
        ensure!(
            !self.source.trim().is_empty(),
            "Archive source directory is required"
        );
        ensure!(
            Path::new(&self.source).is_absolute(),
            "Source must be an absolute path"
        );
        for target in Target::ALL {
            if let Some(dest) = self.destination(target) {
                platform::safe_destination(dest)?;
            }
            if let Some(p) = self
                .executable_overrides
                .get(&target)
                .filter(|s| !s.is_empty())
            {
                crate::archives::safe_relative(p)?;
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub schema_version: u32,
    pub profiles: Vec<Profile>,
    pub selected_profile: Option<Uuid>,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            schema_version: 1,
            profiles: Vec::new(),
            selected_profile: None,
        }
    }
}
impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let value: Self = serde_json::from_slice(&fs::read(path)?)
            .context("Configuration is malformed; the file has been preserved")?;
        ensure!(
            value.schema_version == 1,
            "Unsupported configuration version {}; file preserved",
            value.schema_version
        );
        let mut ids = std::collections::HashSet::new();
        ensure!(
            value.profiles.iter().all(|p| ids.insert(p.id)),
            "Duplicate profile IDs in configuration"
        );
        Ok(value)
    }
    pub fn save(&self, path: &Path) -> Result<()> {
        platform::atomic_json(path, self)
    }
    pub fn validate_paths(&self) -> Result<()> {
        let mut destinations: Vec<(String, PathBuf)> = Vec::new();
        let mut sources = Vec::new();
        for profile in &self.profiles {
            profile.validate()?;
            sources.push(platform::resolved(Path::new(&profile.source))?);
            for target in Target::ALL {
                if let Some(p) = profile.destination(target) {
                    let p = platform::safe_destination(p)?;
                    for (name, other) in &destinations {
                        ensure!(
                            !platform::overlaps(&p, other),
                            "Destination for {} overlaps {}",
                            profile.name,
                            name
                        );
                    }
                    destinations.push((profile.name.clone(), p));
                }
            }
        }
        for (_, dest) in &destinations {
            for source in &sources {
                ensure!(
                    !platform::overlaps(dest, source),
                    "Archive sources and destinations must not overlap"
                );
            }
        }
        Ok(())
    }
    pub fn export_profile(profile: &Profile, path: &Path) -> Result<()> {
        #[derive(Serialize)]
        struct Export<'a> {
            schema_version: u32,
            profile: &'a Profile,
        }
        platform::atomic_json(
            path,
            &Export {
                schema_version: 1,
                profile,
            },
        )
    }
    pub fn import_profile(path: &Path) -> Result<Profile> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Import {
            schema_version: u32,
            profile: Profile,
        }
        let mut value: Import = serde_json::from_slice(&fs::read(path)?)?;
        ensure!(value.schema_version == 1, "Unsupported profile schema");
        value.profile.id = Uuid::new_v4();
        ensure!(
            !value.profile.name.trim().is_empty(),
            "Imported profile has no name"
        );
        Ok(value.profile)
    }
}

pub fn config_path() -> Result<PathBuf> {
    let dirs = directories::ProjectDirs::from("", "", "Fleab")
        .context("Cannot locate user configuration directory")?;
    Ok(dirs.config_local_dir().join("profiles.json"))
}
