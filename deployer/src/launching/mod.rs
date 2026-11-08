use crate::{
    archives::{Target, safe_relative},
    platform,
};
use anyhow::{Context, Result, ensure};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
};

use crate::{
    deployment,
    profiles::{Profile, RunnerKind},
};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    process::{Child, Command},
};

#[derive(Clone, Debug)]
pub struct Runner {
    pub name: String,
    pub path: PathBuf,
    pub kind: RunnerKind,
    pub steam_root: PathBuf,
}

fn vdf_paths(text: &str) -> Vec<PathBuf> {
    // Valve's libraryfolders.vdf contains quoted key/value pairs. Parse quoted
    // tokens, including escaped backslashes, rather than splitting on spaces.
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut escaped = false;
    for c in text.chars() {
        if quoted {
            if escaped {
                current.push(c);
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                tokens.push(std::mem::take(&mut current));
                quoted = false;
            } else {
                current.push(c);
            }
        } else if c == '"' {
            quoted = true;
        }
    }
    tokens
        .windows(2)
        .filter(|pair| pair[0] == "path")
        .map(|pair| PathBuf::from(&pair[1]))
        .collect()
}

pub fn discover_runners() -> Vec<Runner> {
    let Some(base) = directories::BaseDirs::new() else {
        return Vec::new();
    };
    let home = base.home_dir();
    let roots = [
        home.join(".steam/steam"),
        home.join(".steam/root"),
        home.join(".local/share/Steam"),
        home.join(".var/app/com.valvesoftware.Steam/data/Steam"),
    ];
    let path_dirs: Vec<_> = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default();
    discover_at(&roots, &path_dirs)
}

pub fn discover_at(roots: &[PathBuf], path_dirs: &[PathBuf]) -> Vec<Runner> {
    let mut found = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for dir in path_dirs {
        for name in ["wine", "wine64"] {
            let path = dir.join(name);
            if path.is_file()
                && let Ok(path) = platform::resolved(&path)
                && seen.insert(path.clone())
            {
                found.push(Runner {
                    name: format!("Wine — {}", path.display()),
                    path,
                    kind: RunnerKind::Wine,
                    steam_root: PathBuf::new(),
                });
            }
        }
    }
    for root in roots.iter().filter(|r| r.is_dir()) {
        let mut libraries = vec![root.clone()];
        if let Ok(vdf) = fs::read_to_string(root.join("steamapps/libraryfolders.vdf")) {
            libraries.extend(vdf_paths(&vdf));
        }
        let mut dirs = vec![root.join("compatibilitytools.d")];
        dirs.extend(libraries.into_iter().map(|p| p.join("steamapps/common")));
        for dir in dirs {
            if let Ok(entries) = fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let path = entry.path().join("proton");
                    if path.is_file()
                        && let Ok(path) = platform::resolved(&path)
                        && seen.insert(path.clone())
                    {
                        found.push(Runner {
                            name: format!("Proton — {}", entry.file_name().to_string_lossy()),
                            path,
                            kind: RunnerKind::Proton,
                            steam_root: root.clone(),
                        });
                    }
                }
            }
        }
    }
    found
}

#[derive(Clone, Debug)]
pub struct LaunchPlan {
    pub program: PathBuf,
    pub arguments: Vec<OsString>,
    pub environment: BTreeMap<OsString, OsString>,
    pub directory: PathBuf,
}
impl LaunchPlan {
    pub fn spawn(&self) -> Result<Child> {
        Command::new(&self.program)
            .args(&self.arguments)
            .envs(&self.environment)
            .current_dir(&self.directory)
            .spawn()
            .with_context(|| format!("Cannot launch {}", self.program.display()))
    }
}

pub fn plan(
    profile: &Profile,
    target: Target,
    host: &str,
    runners: &[Runner],
) -> Result<LaunchPlan> {
    let record =
        deployment::installed(profile, target)?.context("No build is installed for this target")?;
    if host == std::env::consts::OS {
        let build = crate::archives::Build::parse(Path::new(&record.filename))?;
        let arm = std::env::consts::ARCH == "aarch64";
        let architecture_supported = match build.platform.as_str() {
            "WindowsARM64" | "LinuxARM64" | "macOSARM64" => arm,
            "Linux64" => !arm,
            "Windows64" if host == "linux" => !arm,
            _ => true, // Windows/macOS may provide native OS compatibility.
        };
        ensure!(
            architecture_supported,
            "Installed {} build cannot launch on {}. Deployment is still supported",
            build.platform,
            std::env::consts::ARCH
        );
    }
    let root = platform::safe_destination(profile.destination(target).context("No destination")?)?;
    let configured = profile
        .executable_overrides
        .get(&target)
        .filter(|s| !s.is_empty())
        .map(String::as_str);
    let relative = detect(&root, target, configured.or(record.entrypoint.to_str()))?;
    let executable = root.join(&relative);
    let mut result = LaunchPlan {
        program: executable.clone(),
        arguments: Vec::new(),
        environment: BTreeMap::new(),
        directory: root.clone(),
    };
    if target.os() == host {
        if host == "macos" && executable.extension().is_some_and(|s| s == "app") {
            result.program = PathBuf::from("/usr/bin/open");
            result.arguments.extend([
                OsString::from("-n"),
                executable.into_os_string(),
                OsString::from("--args"),
            ]);
        } else {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                ensure!(
                    host == "windows"
                        || fs::metadata(&executable)?.permissions().mode() & 0o111 != 0,
                    "Installed executable has no execute permission"
                );
            }
            result.directory = executable.parent().unwrap_or(&root).to_owned();
        }
    } else {
        ensure!(
            host == "linux" && target.os() == "windows",
            "{} builds cannot launch on {host}. Deployment is still supported",
            target.label()
        );
        let launch = &profile.launch;
        let detected = if launch.runner.is_empty() {
            Some(
                runners
                    .iter()
                    .find(|r| {
                        launch.runner_kind == RunnerKind::Auto || launch.runner_kind == r.kind
                    })
                    .context("No Wine/Proton runner found; configure one in settings")?,
            )
        } else {
            None
        };
        let (runner, kind, steam) = if let Some(r) = detected {
            (r.path.clone(), r.kind, r.steam_root.clone())
        } else {
            ensure!(
                launch.runner_kind != RunnerKind::Auto,
                "Choose Wine or Proton for a manual runner"
            );
            (
                PathBuf::from(&launch.runner),
                launch.runner_kind,
                PathBuf::from(&launch.steam_root),
            )
        };
        ensure!(
            runner.is_absolute() && runner.is_file(),
            "Runner executable does not exist"
        );
        result.program = runner;
        if kind == RunnerKind::Proton {
            let steam = if !launch.steam_root.is_empty() {
                PathBuf::from(&launch.steam_root)
            } else {
                steam
            };
            ensure!(
                steam.is_absolute() && steam.is_dir(),
                "Proton needs a valid Steam root directory"
            );
            if launch.runner_arguments.is_empty() {
                result.arguments.push(OsString::from("run"));
            }
            result.environment.insert(
                "STEAM_COMPAT_CLIENT_INSTALL_PATH".into(),
                steam.into_os_string(),
            );
            result.environment.insert("SteamAppId".into(), "0".into());
            result.environment.insert("SteamGameId".into(), "0".into());
            result.environment.insert(
                "STEAM_COMPAT_INSTALL_PATH".into(),
                root.clone().into_os_string(),
            );
        }
        let prefix = if !launch.prefix.is_empty() {
            PathBuf::from(&launch.prefix)
        } else {
            let dirs = directories::ProjectDirs::from("", "", "Fleab")
                .context("No user data directory")?;
            dirs.data_local_dir()
                .join("prefixes")
                .join(profile.id.to_string())
                .join(match kind {
                    RunnerKind::Proton => "proton",
                    _ => "wine",
                })
        };
        ensure!(
            prefix.is_absolute(),
            "Compatibility prefix must be an absolute path"
        );
        if kind == RunnerKind::Proton {
            result
                .environment
                .insert("STEAM_COMPAT_DATA_PATH".into(), prefix.into_os_string());
        } else {
            result
                .environment
                .insert("WINEPREFIX".into(), prefix.into_os_string());
        }
        result
            .arguments
            .extend(launch.runner_arguments.iter().map(OsString::from));
        result.arguments.push(executable.into_os_string());
    }
    result
        .arguments
        .extend(profile.launch.game_arguments.iter().map(OsString::from));
    Ok(result)
}

pub fn launch(profile: &Profile, target: Target, runners: &[Runner]) -> Result<Child> {
    let plan = plan(profile, target, std::env::consts::OS, runners)?;
    for key in ["WINEPREFIX", "STEAM_COMPAT_DATA_PATH"] {
        if let Some(prefix) = plan.environment.get(std::ffi::OsStr::new(key)) {
            fs::create_dir_all(Path::new(prefix))?;
        }
    }
    plan.spawn()
}

fn magic(path: &Path, expected: &[u8]) -> bool {
    let mut data = [0u8; 4];
    fs::File::open(path)
        .and_then(|mut f| f.read_exact(&mut data))
        .is_ok()
        && data.starts_with(expected)
}

pub fn detect(root: &Path, target: Target, override_path: Option<&str>) -> Result<PathBuf> {
    if let Some(value) = override_path.filter(|s| !s.is_empty()) {
        let relative = safe_relative(value)?;
        let full = platform::resolved(&root.join(&relative))?;
        ensure!(
            full.starts_with(platform::resolved(root)?),
            "Executable override escapes deployment"
        );
        ensure!(
            full.is_file()
                || target.os() == "macos"
                    && full.is_dir()
                    && full.extension().is_some_and(|s| s == "app"),
            "Executable override does not exist"
        );
        if full.is_dir() {
            validate_bundle(&full)?;
        }
        return Ok(relative);
    }
    let mut candidates = Vec::new();
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        if target.os() == "macos"
            && path.extension().is_some_and(|s| s == "app")
            && path.join("Contents/Info.plist").is_file()
        {
            validate_bundle(&path)?;
            candidates.push(PathBuf::from(entry.file_name()));
        } else if entry.file_type()?.is_file() {
            let stem = path
                .file_stem()
                .context("Invalid executable filename")?
                .to_string_lossy();
            if root.join(format!("{stem}_Data")).is_dir()
                && ((target.os() == "windows"
                    && path
                        .extension()
                        .is_some_and(|s| s.eq_ignore_ascii_case("exe"))
                    && magic(&path, b"MZ"))
                    || (target.os() == "linux" && magic(&path, b"\x7fELF")))
            {
                candidates.push(PathBuf::from(entry.file_name()));
            }
        }
    }
    ensure!(
        candidates.len() == 1,
        "Could not identify one Unity entry point (found {}). Set an executable override in profile settings",
        candidates.len()
    );
    Ok(candidates.remove(0))
}

fn validate_bundle(bundle: &Path) -> Result<()> {
    let plist = plist::Value::from_file(bundle.join("Contents/Info.plist"))
        .context("Invalid macOS bundle Info.plist")?;
    let name = plist
        .as_dictionary()
        .and_then(|d| d.get("CFBundleExecutable"))
        .and_then(|v| v.as_string())
        .context("Bundle has no CFBundleExecutable")?;
    let relative = safe_relative(name)?;
    ensure!(
        relative.components().count() == 1,
        "Bundle executable must be a filename"
    );
    let exe = platform::resolved(&bundle.join("Contents/MacOS").join(relative))?;
    ensure!(
        exe.is_file() && exe.starts_with(platform::resolved(bundle)?),
        "Bundle executable is missing or escapes bundle"
    );
    Ok(())
}
