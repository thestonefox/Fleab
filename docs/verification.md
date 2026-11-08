# Verification record

Local verification on 8 November 2026. These results distinguish executed
checks from automation and hardware checks that remain outstanding.

| Check | Result |
|---|---|
| Rust formatting | Passed |
| Clippy, all targets in this workspace, warnings denied | Passed on Windows x64 and Linux x64 (WSL Ubuntu) |
| Rust unit/integration suite | 25 passed on Windows; 27 on native Linux x64; 2 real-build tests opt-in |
| Standalone managed publisher | 27 assertions passed on Windows; 30 on Linux x64 |
| Unity Editor | Unity 6000.6.8f1; 6 package tests passed |
| Unity Windows publishing acceptance | 7 tests passed, including two real builds |
| Unity Linux cross-build publishing acceptance | 7 tests passed, including two real builds |
| Unity macOS ARM64 cross-build publishing acceptance | 7 tests passed, including two real builds |
| Unity BuildHistory recovery | Passed in Windows real-build acceptance |
| Rust deployment of real Windows/Linux/macOS archives | Passed; independent destinations and state |
| Clean Windows Sandbox | Core tests, real native Unity deploy/launch, process protection and GUI startup passed |
| Native release | Windows x64 release built with static CRT |
| Small-window layouts | Build controls fit at 780×660 and 480×390; settings fields/actions fit at 700×490 and 480×390 in both themes |
| System appearance | Light/dark/light transitions passed; Linux portal input updates and no-preference fallback passed |
| Redesign screenshots | App and settings captured and inspected in light/dark Windows Sandbox sessions |
| Workflow files | actionlint passed previously; user reported the corrected six-platform validation passed; redesign awaits fresh CI |

The initial clean-Sandbox test exposed a missing Visual C++ runtime dependency.
Static CRT linking fixed it. The successful guest had networking and clipboard
disabled, no installed Rust/.NET toolchains, and access only to test input and
disposable output. The real player ran in batch/no-graphics mode. GUI startup
smoke does not establish full interaction or rendering correctness on all GPUs.

Core tests cover naming/UTC ordering, target filtering, CRUD/import/export,
atomic config writes, invalid schemas, path protection, source/destination
overlap, C# archive interoperability, UTF-8/PAX paths, malicious paths and entry
types, oversized metadata, truncated frames, cancellation, stale-file removal,
unowned directories, durable interruption recovery, installed-state semantics,
entry-point ambiguity, executable overrides, binary macOS plists, Wine/Proton
discovery and argument/environment construction. Native Windows process launch
and running-process detection are exercised. Unix mode/symlink tests are
provided for native CI and passed in native Linux filesystems under WSL.

## Desktop appearance redesign

The desktop now follows Fluent layout, typography and color guidance with
installed system fonts, vector icons, decoded build labels and separate settings
tabs. App branding appears in the native title bar as `Fleab <version>`; the
slogan is removed from the desktop and publisher interfaces. The publisher uses
Unity's own themed controls and a scrolling layout, and its title includes the
installed package version.

Screenshot previews under `docs/screenshots/` capture the actual app views using
a disposable GPU capture harness in Windows Sandbox. They contain sample game
data and exclude native OS window decorations. No host desktop interaction or
host appearance change was required. The harness uses the production view code
and forces each palette only for capture. Production follows system appearance.
Theme-change tests exercise the system-preference path rather than this capture
override. The Linux portal listener compiles and its input mapping is tested;
live portal signals on GNOME/KDE/Steam Deck still need a desktop session.

The updated Unity window compiles and the six package editor tests pass in
hidden batch mode. A visual Unity capture remains unavailable in Sandbox:
Unity exits before producing its startup log with `0x8007007E` (missing module).

## Still requires an appropriate environment

* Fresh six-platform CI for the desktop redesign. The user reported all prior
  validation jobs passed after the metadata/Clippy fixes below.
* Native Linux ARM64, Linux x64 desktop runtime beyond WSL, macOS Intel/Apple
  Silicon desktop interaction and Windows ARM64 runtime. Local Visual Studio
  lacks the ARM64 C++ compiler; native ARM64 CI jobs are configured.
* Steam Deck X11/Wayland, XDG portal pickers, actual Wine and Proton launches,
  Steam Linux Runtime requirements, prefixes and game-specific compatibility.
* Native macOS publisher modes and framework symlinks on hardware. The Linux
  publisher's modes and framework links have passed native filesystem tests.
* Dedicated Server player builds: this Unity installation has client desktop
  modules, but no Dedicated Server playback variations. Configuration-based
  server identification and independent server targets are implemented.
* Network-share interruption/permissions and real SMB atomic rename semantics.
* Physical disk-full conditions, power-loss durability and concurrent external
  processes starting during the final replacement window.
* macOS Developer ID signing, notarisation, Gatekeeper and quarantined game
  bundles. Packaging applies only an ad-hoc signature.
* Full manual interaction on each desktop platform and accessibility tooling.

The first hosted CI run passed macOS ARM64. Its Linux jobs failed Clippy and
macOS Intel failed publisher-archive deployment tests, as described below.
The user subsequently reported all validation jobs passed. No remote tags,
releases or uploads were created by these local verification tasks. The Unity package
is ready at `unity-package/`; Windows build/package artifacts are generated in
`target/release/` and `dist/`. Local toolchains, isolated Unity projects, raw
test logs and disposable Sandbox output remain ignored under `.tools/`.

The subsequent merged-PR SemVer automation is covered separately by
19 passing tests in `tests/release/`: bump precedence, breaking syntax, grouped/escaped notes,
pagination beyond 250 PR commits, exact Git snapshots, unchanged main/index,
version/lock synchronisation, atomic tag pushes, queued-merge catch-up and
idempotent retry. These local tests use no live GitHub writes. The first hosted
release must verify repository token/tag permissions and native runners. The
workflow definitions also pass actionlint. A disposable clone of the actual
project was stamped as 0.2.0; `cargo metadata --locked` confirmed that the Rust
manifest, lockfile and Unity package agree without modifying main. Release
rules and notes use locked official semantic-release plugins.

## First hosted CI corrections

The Linux failures were redundant borrows in `platform::overlaps`, exposed
because Linux retains `&Path` values where Windows/macOS use case-folded
`PathBuf` buffers. Both branches now compare `&Path` references. Native Linux
Clippy with warnings denied passes after the correction.
All 24 Linux Rust tests then passed using the publisher's Linux-generated
fixture. Test executables ran from a native Linux temporary filesystem so
permission/symlink assertions did not depend on DrvFS mount emulation.

The macOS Intel failure was consistent with incorrect permissions in the
C#-produced archive: the raw `lstat` symbol uses Darwin's legacy layout on x64,
while the publisher read the modern layout, as defined in
[Apple's ABI headers](https://github.com/apple-oss-distributions/xnu/blob/main/bsd/sys/cdefs.h).
Intel Darwin now calls
`lstat$INODE64`; Apple Silicon retains the modern unsuffixed function. An
explicit archive regression check verifies mode 0644 and the player's original
bytes before Rust consumes the fixture. Older Linux x64 glibc also receives a
versioned `__lxstat` fallback, verified against Ubuntu 20.04 headers/runtime.

Executable-read errors now retain their file/permission context instead of
being reported only as zero detected candidates. Artifact upload/download
actions have been updated to their maintained Node runtime versions. The
macOS Intel correction still requires the next hosted run for confirmation.

## Known operational limits

Process visibility is OS-dependent, especially under Wine/Proton wrappers;
close game and server processes before replacing builds. Unsafe/ambiguous
filesystem states fail rather than guessing. Windows symlink creation requires
an existing OS privilege/Developer Mode. Publishing Windows reparse points is
rejected. TAR does not preserve macOS extended attributes/resource forks.
BuildHistory recovery with no original Fleab record uses current Player
Settings for the name/version, as described in `publisher.md`.
