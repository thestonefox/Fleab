# Fleab Publisher

Install `com.fleab.publisher` in Unity Package Manager using this Git URL:

```text
https://github.com/thestonefox/Fleab.git?path=/unity-package#v0.6.0
```

For a local checkout, use **Install package from disk** and select
`unity-package/package.json`. Unity 6.6 is the primary
target. Other Unity 6 versions are best effort; only the tested version is
listed in the verification record.

`Tools > Fleab > Settings` has three essential settings: automatic publishing
(off by default), publishing directory, and optional build name override.
They are saved per project in `ProjectSettings/FleabSettings.asset`. Commit
that asset only if its machine-specific destination suits your team.

An `IPostprocessBuildWithReport` callback records successful supported desktop
builds even when automatic publishing is off. It uses the report's target and
`GetSubtarget<StandaloneBuildSubtarget>()`, not the filename, to identify
Dedicated Servers. PE, ELF and Mach-O headers identify the actual output CPU,
including Windows ARM64 and macOS universal players.

The report supplies output paths and files. Known player data/runtime
structures supplement incremental reports. Fleab does not indiscriminately
archive the output's parent folder. macOS archives contain the `.app` at the
archive root. Avoid mixing unrelated content into a player's own `_Data`
directory or runtime directories, which are deliberately packaged in full.

Packaging runs on a worker thread. Unity APIs and progress UI remain on the
Editor thread; assembly reload is locked during publishing. Progress closes on
success or failure. The console reports the resulting archive or a useful
error; the Settings window and Tools menu provide Retry Publish.

`Publish Last Build` uses the persistent record in `Library/Fleab/last-build.json`.
On Unity 6.6 it also queries the public BuildHistory API for the newest
successful supported player build. The record retains build-time naming data.
When recovering an older build with no Fleab record, the current Product Name,
name override and Player Settings version supply naming metadata; the original
build start time supplies the timestamp. Publish before changing project
version metadata when recovering historical builds for the first time.

Missing original files fail usefully. Compression/copy failures preserve the
record and original build for retry. If the publishing share is unavailable,
reconnect it or change the publishing destination and retry without rebuilding.
Archives are never overwritten; a UUID collision suffix is added when needed.
Older archives are never removed automatically. Interrupted `.partial` files
are invisible to discovery and may be removed manually after publishing stops.

The managed ZstdSharp.Port library and its managed Unsafe dependency are
included as Editor-only plugins. No NuGet setup, shell tools or native
compressor installation is needed. Ordinary Unix modes and safe internal
symlinks are retained. Windows reconstruction assigns execution only to known
Linux/macOS entry points and `.so` files. Windows reparse-point publishing is
rejected; use a regular output directory. Native Unix publishing supports
validated relative symlinks, including framework aliases. Resource forks and
extended attributes are not part of the contract; native macOS signing and
notarisation must be verified for your build.

Android, iOS, WebGL, consoles, failed builds and cancelled builds are excluded.
Build Profiles and Unity's normal Build action are never replaced or modified.
