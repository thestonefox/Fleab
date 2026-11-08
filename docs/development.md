# Development

Fleab is one repository: `unity-package/` is the UPM Editor extension and
`deployer/` is the native Rust application. `docs/archive-format.md` is the
interoperability contract. Both components are versioned together at 0.1.0.

Use Conventional Commits: `type(scope): subject`, a blank line, then a body
explaining the change. Every commit message line is limited to 72 characters.
Implement and validate changes in focused milestones.

Rust: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets
-- -D warnings`, `cargo test --workspace`, `cargo build --release`.

Unity integration requires Unity 6 with desktop platform modules. Target is
Unity 6.6. Standalone C# archive tests do not replace Editor integration tests.

## Toolchains and checks

The workspace's locked dependency graph is committed. Windows uses static CRT
linking in `.cargo/config.toml`; retain it for truly portable releases. On Linux,
install `libxkbcommon-dev libwayland-dev libgl1-mesa-dev libx11-dev libxrandr-dev
libxi-dev libxcursor-dev libxinerama-dev pkg-config` and a C compiler. macOS needs
Xcode command-line tools. .NET 10 is used only for the standalone publisher
tests; the package core is also compiled against .NET Standard 2.1.

`dotnet run --project tests/Publisher.Tests -- tests/fixtures` generates an
archive with the actual bundled managed compressor and TAR writer. Rust tests
consume it, including PAX UTF-8/long paths. Unix runners additionally exercise
modes and framework symlinks. Do not replace that fixture with a Rust-only mock.

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
These workflows have not been executed by this local task.

The Unity workflow is explicitly dispatched on a licensed self-hosted Windows
runner labelled `unity`, with `UNITY_EDITOR_PATH` set to its installed Editor.
`tools/test-unity.ps1` creates an isolated project and runs Editor tests. Enable
actual player builds with `-BuildPlayers`; the required modules must be present.
Dedicated Server smoke builds require separately installed Server modules.

Set the workspace and UPM versions together, then tag `vX.Y.Z`. `release.yml`
checks version consistency, tests/builds all six targets, packages native
artifacts and publishes only after every build succeeds. The owner name is
intentionally unspecified. GitHub releases/tags are not created by local builds.

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

