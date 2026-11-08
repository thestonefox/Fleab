# Verification record

Local verification on 8 November 2026. These results distinguish executed
checks from automation and hardware checks that remain outstanding.

| Check | Result |
|---|---|
| Rust formatting | Passed |
| Clippy, all targets in this workspace, warnings denied | Passed on Windows x64 |
| Rust unit/integration suite | 23 passed; 2 real-build tests opt-in |
| Standalone managed publisher | 24 assertions passed on Windows |
| Unity Editor | Unity 6000.6.8f1; 6 package tests passed |
| Unity Windows publishing acceptance | 7 tests passed, including two real builds |
| Unity Linux cross-build publishing acceptance | 7 tests passed, including two real builds |
| Unity macOS ARM64 cross-build publishing acceptance | 7 tests passed, including two real builds |
| Unity BuildHistory recovery | Passed in Windows real-build acceptance |
| Rust deployment of real Windows/Linux/macOS archives | Passed; independent destinations and state |
| Clean Windows Sandbox | Core tests, real native Unity deploy/launch, process protection and GUI startup passed |
| Native release | Windows x64 release built with static CRT |
| Small-window settings layout | Offscreen egui tests passed at 700×490 and 480×390 |
| Workflow files | YAML parsed locally; hosted workflows not yet run |

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
provided for native CI, but were not executed on this Windows host.

## Still requires an appropriate environment

* Native Linux x64/ARM64 and macOS Intel/Apple Silicon compilation/runtime.
* Windows ARM64 compilation/runtime. Local Visual Studio lacks its ARM64 C++
  compiler; native ARM64 GitHub Actions jobs are configured.
* Steam Deck X11/Wayland, XDG portal pickers, actual Wine and Proton launches,
  Steam Linux Runtime requirements, prefixes and game-specific compatibility.
* Native Unix publisher modes and framework symlinks on real filesystems.
* Dedicated Server player builds: this Unity installation has client desktop
  modules, but no Dedicated Server playback variations. Configuration-based
  server identification and independent server targets are implemented.
* Network-share interruption/permissions and real SMB atomic rename semantics.
* Physical disk-full conditions, power-loss durability and concurrent external
  processes starting during the final replacement window.
* macOS Developer ID signing, notarisation, Gatekeeper and quarantined game
  bundles. Packaging applies only an ad-hoc signature.
* Full manual interaction on each desktop platform and accessibility tooling.

The six-platform CI and portable-release workflows are supplied, not reported
as passed. No remote tags, releases or uploads were created. The Unity package
is ready at `unity-package/`; Windows build/package artifacts are generated in
`target/release/` and `dist/`. Local toolchains, isolated Unity projects, raw
test logs and disposable Sandbox output remain ignored under `.tools/`.

## Known operational limits

Process visibility is OS-dependent, especially under Wine/Proton wrappers;
close game and server processes before replacing builds. Unsafe/ambiguous
filesystem states fail rather than guessing. Windows symlink creation requires
an existing OS privilege/Developer Mode. Publishing Windows reparse points is
rejected. TAR does not preserve macOS extended attributes/resource forks.
BuildHistory recovery with no original Fleab record uses current Player
Settings for the name/version, as described in `publisher.md`.
