# Fleab desktop application

## Appearance

The native title bar shows Fleab and the release version. The interface follows
the system light/dark preference at startup and when the desktop reports a theme
change. Linux reads and watches the standard XDG desktop portal appearance
setting, including on X11. If no desktop preference is available, it uses light
mode. Both palettes use the same spacing, controls and readable contrast.
Windows uses an installed Segoe UI font; macOS and Linux use an available system
font, with bundled egui fonts as a fallback. No platform font is redistributed.

The layout follows Microsoft's [Windows typography](https://learn.microsoft.com/en-us/windows/apps/design/signature-experiences/typography),
[color](https://learn.microsoft.com/en-us/windows/apps/design/signature-experiences/color)
and [geometry](https://learn.microsoft.com/en-us/windows/apps/design/signature-experiences/geometry)
guidance. Controls are rendered by egui; they are not WinUI or AppKit widgets.
The settings cog, refresh, folder and back buttons have descriptive tooltips and
accessibility labels. Settings has separate folder and launch tabs, a scrolling
form and a fixed save/cancel footer. Published build labels show decoded version,
target and UTC time; the original archive name is available on hover.

Windows Sandbox captures with a sample game profile:
[light app](screenshots/fleab-light.png), [dark app](screenshots/fleab-dark.png),
[light settings](screenshots/settings-light.png),
[dark settings](screenshots/settings-dark.png). These show the rendered window
content; the OS draws the native title bar separately.

## Deployment

For shared-folder examples and profile setup, see [folders and profiles](profiles.md).

The portable native egui/eframe application uses filesystem paths only.
Create profiles in Settings. There are six optional destinations, one for
each OS/client-server target. A blank destination disables that target's builds
in discovery. Architectures share destinations; no host-platform filter exists.

Sources are scanned at startup, profile selection and Refresh. There is no
watcher. All matching archives share one newest-first dropdown. Discovery
ignores temporary filenames, malformed names, directories and symlink files.
A disconnected source reports an error without erasing profile settings or
local installation records. Reconnect the share and Refresh.
If a share disconnects after discovery, the last scanned selection remains
available for launching the installed build; deployment still validates the
source on use. With no scanned selection, local installation records remain
visible as status information.

The selected archive determines the destination and installed-build display.
Deploy installs the selected archive immediately. Launch uses the installed
record for that target. Configured executable overrides are relative to the
installation and must resolve inside it. Automatic detection requires a unique
Unity player executable with its `_Data` directory or a valid `.app` bundle.
macOS bundle validation uses `CFBundleExecutable`, including binary plists.

## Storage

Configuration schema 1 is JSON, saved atomically to:

* Windows: `%LOCALAPPDATA%\Fleab\config\profiles.json`.
* Linux: `$XDG_CONFIG_HOME/fleab/profiles.json`, usually `~/.config/fleab/profiles.json`.
* macOS: `~/Library/Application Support/Fleab/profiles.json`.

`directories` determines the exact OS-native path; a load error shows it in the
app. A configuration lock prevents simultaneous writers. Malformed JSON or an
unknown schema opens the app with an error and disables writes; correct the
file and restart. No automatic migration drops unknown data. Version 1 has no
older schema to migrate. Future migrations must be explicit.

Each owned installation contains `.fleab-installed.json` with profile identity,
target, archive name, executable and transaction identity. The marker moves
atomically with the installation and is independent of exported profile JSON.
Import assigns a new profile ID. It cannot claim an existing installation
owned by the exported ID; choose an empty destination. Replacing the Fleab
executable preserves both profiles and installed state.

## Replacement and recovery

Fleab refuses nonempty unowned destinations, destination symlinks/junctions,
unsafe roots, and all source/destination overlaps. It checks every profile for
shared or nested destination conflicts, resolves existing ancestors, and locks
the target while deploying. The selected archive must be in the configured
source directory. Never configure a general-purpose folder as a game target.

The archive is copied locally, fully decompressed and safely extracted to a
same-filesystem staging directory. Paths, entry types, checksums, duplicate
names, symbolic link graphs and entry points are validated before replacement.
Modes retain rwx bits without special privilege bits. Extraction has size,
entry-count, path-length and decompression-window limits; see the archive
contract. Disk-full and permission errors leave the prior installation intact.

A flushed journal and temporary backup protect the final renames. Retry Deploy
after an interruption: Fleab restores an uncommitted backup or completes
cleanup for a committed installation before proceeding. Never manually delete
transaction directories while an operation is running. If both the final and
backup directories conflict or the journal is malformed, Fleab stops for
inspection, preserving the evidence. Lock files may remain; locks are OS-held,
so a stale filename does not mean a deployment is running.

Cancel stops copying/extraction and cleans staging. Closing the window during
deployment requests cancellation and waits until the transaction is safe.
Published archives are untouched. There is no permanent rollback history.

Process detection checks executable paths and absolute command arguments,
including common Wine `Z:` paths. It never kills a game. OS permissions,
short-lived launcher wrappers, Wine path aliases and processes that hide their
command lines can limit detection. Close all game/server instances before
deploying. OS file locks remain an additional check on Windows. Fleab cannot
eliminate races with external programs starting a game during replacement or
another privileged program changing filesystem links concurrently.

## System requirements

Windows x64 and ARM64 builds statically link the C/C++ runtime. Standard Windows
system DLLs and graphics drivers remain required. No .NET runtime is used by
the Deployer. Safe symlink extraction on Windows can require Developer Mode or
an existing symlink privilege; Fleab does not change these settings.

Linux releases built on Ubuntu 24.04 require a compatible glibc (2.39 or newer),
libgcc, OpenGL/EGL drivers, libxkbcommon, and X11 or Wayland client libraries.
Install the distribution's desktop packages rather than copying random shared
libraries. Folder pickers use the XDG desktop portal and require a functioning
portal service/backend. Steam Deck desktop mode supplies the relevant desktop
environment; actual Steam Deck runtime verification is still required.

macOS releases are portable `.app` bundles for Intel and Apple Silicon. Build
automation applies an ad-hoc signature; it is not Developer ID notarisation.
Unsigned game bundles may also be subject to quarantine and Gatekeeper. Use
the normal macOS approval process or a maintainer-signed release; Fleab does not
remove quarantine attributes or bypass platform security.
