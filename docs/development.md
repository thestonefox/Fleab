# Development

Fleab is one repository: `unity-package/` is the UPM Editor extension and
`deployer/` is the native Rust application. [The archive contract](archive-format.md)
defines interoperability. Both components share a release version.

Use Conventional Commits: `type(scope): subject`, a blank line, then a body
explaining the change. Every commit message line is limited to 72 characters.
Implement and validate changes in focused milestones.

## Build from source

Install stable Rust and the platform's native build tools: Visual Studio C++ on
Windows, Xcode command-line tools on macOS, or a C compiler and desktop development
libraries on Linux. From the repository root:

```sh
cargo build --locked --release
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace
```

The executable is `target/release/fleab.exe` on Windows and
`target/release/fleab` on Linux/macOS. The `unity-package/` directory is the
ready-to-install UPM package; its managed dependencies and licenses are bundled.

Unity integration requires Unity 6 with desktop platform modules. Target is
Unity 6.6. Standalone C# archive tests do not replace Editor integration tests.

## Toolchains and checks

The workspace's locked dependency graph is committed. Windows uses static CRT
linking in `.cargo/config.toml`; retain it for truly portable releases. On Linux,
install `libxkbcommon-dev libwayland-dev libgl1-mesa-dev libx11-dev libxrandr-dev
libxi-dev libxcursor-dev libxinerama-dev pkg-config` and a C compiler. macOS needs
Xcode command-line tools. .NET 10 is used only for the standalone publisher
tests; the package core is also compiled against .NET Standard 2.1.

With .NET 10, run the publisher harness independently of Unity:

```sh
dotnet run --project tests/Publisher.Tests -- tests/fixtures
```

This generates an archive with the bundled managed compressor and TAR writer.
Rust tests consume it, including PAX UTF-8/long paths. Unix runners exercise
modes and framework symlinks. Do not replace that fixture with a Rust-only mock.

For Unity editor tests, set `UNITY_EDITOR_PATH` to the installed `Unity.exe` and
run the hidden batch-mode harness from PowerShell:

```powershell
./tools/test-unity.ps1 -UnityEditor $env:UNITY_EDITOR_PATH
./tools/test-unity.ps1 -UnityEditor $env:UNITY_EDITOR_PATH -BuildPlayers
```

The script uses an isolated project. `-BuildPlayers` creates scenes and actual
Windows builds; `-BuildTarget Linux64` or `macOS` checks cross-build publishing
when those modules are installed. The ordinary editor run skips expensive builds.

For a real native Unity archive, set `FLEAB_ACCEPTANCE_ARCHIVE` and run
`cargo test --test acceptance -- --ignored`. It deploys to a disposable directory,
starts the native player in batch mode, checks running-process protection and
terminates only its own test process. Use a test game, not an interactive build
you are actively playing. The normal test suite leaves this test ignored.

Windows Sandbox checks avoid desktop interaction:

```powershell
./tools/start-sandbox-tests.ps1 -UnityArchive 'D:\Published\Game_1_20261108-090000_Windows64.tar.zst'
```

Build release/tests first and have `cargo` on PATH. The Sandbox feature must
already be enabled. The guest runs unit/integration tests, optional real-player
acceptance, and a GUI startup smoke check. Networking and clipboard are off.
Only the read-only test input and a disposable result directory are mapped.
The guest shuts itself down; the script has an explicit guest-user guard.
Results are written under `.tools/sandbox-results-*/`. GUI smoke establishes
startup, not interaction correctness or visual layout.

## Automation and releases

`ci.yml` validates both architectures on all three operating systems using
native hosted runners, including C# interop and portable packaging. Availability
of ARM runners can depend on repository visibility/account entitlement; use an
equivalent native self-hosted runner if needed. Runner definitions are based on
[GitHub's supported labels](https://docs.github.com/en/actions/reference/runners/github-hosted-runners).
See the [verification record](verification.md) for executed checks and remaining
hardware validation.

The Unity workflow is explicitly dispatched on a licensed self-hosted Windows
runner labelled `unity`, with `UNITY_EDITOR_PATH` set to its installed Editor.
`tools/test-unity.ps1` creates an isolated project and runs Editor tests. Enable
actual player builds with `-BuildPlayers`; the required modules must be present.
Dedicated Server smoke builds require separately installed Server modules.

Set the initial workspace/UPM versions together and push the initial `vX.Y.Z`
tag. Thereafter `semver.yml` allocates a tag for each merged PR from its original
Conventional Commits. Breaking changes bump major, `feat` bumps minor, and other
changes bump patch. Versioned release snapshots leave protected main untouched.
`release.yml` checks consistency, tests/builds all six targets and publishes
after every build and asset upload succeeds. See `releases.md` for details.
The repository is [thestonefox/Fleab](https://github.com/thestonefox/Fleab).
Local builds do not push tags or releases.

## Package a portable release

After building the native release executable, package the matching host target:

```sh
python tools/package-release.py --platform windows --arch x64
```

Use `--platform linux` or `--platform macos` and `--arch x64` or `--arch arm64`
to match the compiled binary. For a cross-compiled binary, also pass its Rust
`--target` triple. This command packages existing output; it does not cross-compile.

`tools/package-release.py` copies the executable, documentation and dependency
license texts to `dist/`, then writes a ZIP or tarball plus SHA-256. macOS
packaging creates and ad-hoc signs a `.app`. Maintainers must separately arrange
Developer ID signing/notarisation when distributing a trusted macOS release.
An existing package staging directory is never silently overwritten.

## Engineering boundaries

Archive names, paths and links are untrusted. Keep extraction staged and links
deferred until all regular contents have been validated. Test new entry types
before allowing them. Preserve the installed marker inside the renamed tree;
separately saving a config flag is not an atomic installation commit. Never
remove an unowned destination or infer success from a selected archive.

Do not add web repositories, accounts, services, auto-deploy/watchers, retention,
self-updates or queues. Additional features require explicit agreement.

