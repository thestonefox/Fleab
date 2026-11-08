<p align="center"><img src="assets/fleab.svg" width="160" alt="Fleab builder flea wearing a hard hat" /></p>

# Fleab — Flea Builder

**Build it. Hop it. Play it.**

Fleab moves Unity desktop builds between computers using ordinary folders.
Build normally in Unity; **Fleab Publisher** packages the successful output as
`.tar.zst`. On another computer, the portable **Fleab** desktop app discovers,
deploys and launches those builds. A target computer can be off while you build.

There is no deployment server, account, database or target-machine agent. A
local directory, Windows UNC share, mapped drive or mounted SMB share is enough.

## Quick start

1. In Unity Package Manager, choose **Install package from Git URL**:

   ```text
   https://github.com/OWNER/fleab.git?path=/unity-package#v0.1.0
   ```

   Replace `OWNER` with the repository owner. For development, use **Install
   package from disk** and select `unity-package/package.json`.
2. Open **Tools > Fleab > Settings**. Choose the publishing directory and enable
   automatic publishing. Optionally override the build name; otherwise Unity's
   Product Name is used. The application version comes from Player Settings.
3. Use Unity's normal **Build Profiles > Build** action. Fleab discovers the
   completed output; there is no Fleab build-output setting.
4. On the other computer, extract the appropriate portable release and run
   `fleab.exe`, `fleab` or `Fleab.app`.
5. In **Settings**, create a profile with an archive source and at least one
   deployment destination. Save it, select a build, and click **Deploy**.
6. **Launch** starts the installed build for that target. Selecting a newer
   archive does not deploy it, and deployment does not launch automatically.

Use **Tools > Fleab > Publish Last Build** to publish without rebuilding, even
when automatic publishing is disabled. **Retry Publish** retries using the
recorded output and current publishing destination. Missing output produces an
error; Unity's original build files are never deleted by Fleab.

## Folders and profiles

Example publishing destination on the Unity machine:

```text
Local:          D:\Published\MyGame
Windows SMB:    \\buildbox\builds\MyGame
Mapped drive:   Z:\MyGame
Linux mount:    /mnt/builds/MyGame
macOS mount:    /Volumes/Builds/MyGame
```

Mount or authenticate shares using the operating system. Fleab uses your existing
filesystem access and never asks for SMB credentials.

Example Steam Deck profile:

| Setting | Value |
|---|---|
| Name | My multiplayer game |
| Archive source | `/mnt/builds/MyGame` |
| Windows client | `/home/deck/Games/MyGame/Windows` |
| Linux client | `/home/deck/Games/MyGame/Linux` |
| macOS client | blank |
| Windows server | blank |
| Linux server | `/home/deck/Games/MyGame/Server` |
| macOS server | blank |

A destination enables its OS/client-server target. All enabled builds appear in
one dropdown, newest first. Architectures share the same destination; installing
another architecture replaces that target's installation. Host architecture
does not filter discovery. Client and server installations are independent.

Fleab only replaces its own installations or empty directories. Choose a
dedicated folder for each target. Sources and destinations must not overlap;
destinations cannot share or nest inside one another. Roots, home directories,
user folder roots and protected system directories are rejected.

Profiles live in the normal user configuration directory and survive executable
replacement. Settings supports create/edit/delete and JSON import/export.
Imports receive a new identity and no installation state. Paths can be edited
before saving; installation records stay on the local computer. Deleting a
profile leaves its installed files intact.

## Safe deployment

Deployment copies an archive to local temporary storage, validates and extracts
it into staging beside the destination, and checks for a unique player entry
point. The previous installation moves to a temporary backup only after staging
succeeds. Final replacement uses same-filesystem renames. A durable transaction
record permits recovery after interruption. Successful deployments remove the
temporary backup; published archives are retained until you remove them.

Cancel is available during copying/extraction. The final rename transaction
cannot be cancelled. Running games block replacement; Fleab does not terminate
them. Progress and errors appear in the native window. Refresh scans on demand;
there is no filesystem watcher or background synchronisation.

## Native games, servers and Steam Deck

Windows players launch on Windows, Linux players on Linux, and macOS bundles on
macOS. Automatic detection uses Unity's output structure. An ambiguous entry
point requires an optional relative executable override in profile settings.
Arguments use one argument per line; spaces remain part of an argument, without
shell quoting. Dedicated Servers use the matching server destination.

On Linux, Windows players can launch with Wine or Proton. Fleab discovers Wine
on PATH and Proton in common Steam libraries, including external libraries and
`compatibilitytools.d`. Select a runner in profile settings or specify it
manually. A manual Proton runner needs the Steam root directory. Compatibility
prefixes are created per profile in user data by default. Runner arguments,
game arguments and prefix paths are persistent. See [Proton and Steam Deck](docs/proton.md).

Deployment works without a compatible launch runner. Unsupported launch
combinations explain why Launch is disabled. No other emulation is provided.

## Portable releases

After the repository is configured, obtain archives from its **GitHub Releases**
page. Release automation produces Windows x64/ARM64 ZIPs, Linux x64/ARM64
tarballs and macOS Intel/Apple Silicon `.app` tarballs, with SHA-256 files.
No releases have been uploaded by this local implementation task.

Windows links the C/C++ runtime statically. Linux needs a desktop session,
OpenGL/EGL, X11 or Wayland, and the usual system libraries described in
[Deployer](docs/deployer.md). macOS artifacts are ad-hoc signed for integrity;
Developer ID signing/notarisation requires a maintainer's signing identity.
Follow macOS's normal Gatekeeper prompts and organisational security policy.
Fleab does not bypass OS security controls or update itself.

Replace the executable or `.app` to upgrade. Keep the user configuration and
deployment folders. Do not install Rust, .NET, TAR or Zstandard for normal use.

## Build and test

Install stable Rust and native build tools (Visual Studio C++ on Windows,
Xcode command-line tools on macOS, or a C compiler and desktop development
libraries on Linux), then run:

```sh
cargo build --locked --release
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace
```

The native binary is `target/release/fleab[.exe]`. To package the current host:

```sh
python tools/package-release.py --platform windows --arch x64
# Or --platform linux|macos and --arch x64|arm64, matching the built binary.
```

Package outputs are in `dist/`. The Unity package is the ready-to-install
`unity-package/` directory. Managed dependencies and licenses are bundled.

With .NET 10, test the same archive code outside Unity:

```sh
dotnet run --project tests/Publisher.Tests -- tests/fixtures
```

On Windows, run Editor integration and actual build tests without opening UI:

```powershell
./tools/test-unity.ps1 -UnityEditor 'F:\Unity\6000.6.8f1\Editor\Unity.exe' -BuildPlayers
```

Use an isolated test project. The test creates scenes and builds Windows
players; `-BuildTarget Linux64` or `macOS` checks cross-build publishing where
those modules are installed. Ordinary Editor tests omit the expensive builds.

## Repository

```text
unity-package/   UPM settings, callback, TAR/Zstd publisher and Editor tests
deployer/        Native egui app, profiles, archive/deploy/launch logic and tests
docs/            Format, usage, safety, development and verification records
tools/           Standalone C# harness, Unity tests, packaging and Sandbox tools
tests/           Managed tests and a real C#-produced interop fixture
assets/          Original builder-flea SVG, PNG and platform icons
.github/         Six-target validation, portable releases and Unity workflow
```

See [Publisher](docs/publisher.md), [Deployer](docs/deployer.md),
[archive contract](docs/archive-format.md), [development](docs/development.md)
and [verification status](docs/verification.md) for details and limitations.

Licensed under MIT; bundled third-party libraries retain their own licenses.
