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
watcher. Refresh sits beside the Published build dropdown and selects the
newest available archive after a successful scan. Rescans after deployment
keep the selected archive. All matching archives share one newest-first
dropdown. Discovery ignores temporary filenames, malformed names, directories
and symlink files.
A disconnected source reports an error without erasing profile settings or
local installation records or the last selected archive. Reconnect the share
and Refresh.
If a share disconnects after discovery, the last scanned selection remains
available for launching the installed build; deployment still validates the
source on use. With no scanned selection, local installation records remain
available in the Installation panel.

The selected archive determines the destination and installed-build display.
The **Installation** panel starts collapsed; click its header to see the
destination and installed build. Its open state is remembered for each game
and target during the app session. Installation errors remain visible when
the details are collapsed.

The primary split button defaults to **Deploy and launch**: it installs the
selected archive, then starts that newly installed build with the profile's
launch settings. Open the arrow to choose **Deploy only** or **Launch installed**.
Choosing a menu item changes the button; click the button to run it. The choice
lasts for the current app session.
See the action menu in [light](screenshots/actions-light.png) or
[dark](screenshots/actions-dark.png) mode.

Launch never follows a failed or cancelled deployment. If cancellation arrives
during the final replacement, the completed installation is kept and launch is
skipped. If starting the game fails, the new installation remains available;
correct the launch settings and retry **Launch installed** without redeploying.
Unsupported platforms or missing Wine/Proton runners disable the combined
action with an explanation; **Deploy only** remains available. A first-time
installation can use the combined action without a previously installed build.

**Launch installed** uses the installed record for the selected target,
independently of the selected archive's version. Configured executable overrides
are relative to the installation and must resolve inside it. Automatic detection
requires a unique
Unity player executable with its `_Data` directory or a valid `.app` bundle.
macOS bundle validation uses `CFBundleExecutable`, including binary plists.

## Background mode and game shortcuts

Open **Settings > Application** and enable **Keep Fleab in the system tray**
(**menu bar** on macOS). This is off by default. Choose whether minimising the
window hides it and whether closing the window keeps Fleab running. Changes
are saved immediately, independently of game profiles.
See the Application settings in [light](screenshots/background-light.png) or
[dark](screenshots/background-dark.png) mode.

The icon's menu provides **Open Fleab**, a submenu for each configured game,
**Settings**, and **Exit Fleab**. Games with multiple destinations have a target
submenu so client/server and OS installations are explicit. Game actions are:

* **Deploy latest** rescans the source and installs the newest archive for that
  destination, ordered by the archive's build timestamp.
* **Run installed** launches the installed copy without requiring the archive
  source to be connected.
* **Deploy and run latest** chooses the newest launch-compatible archive for
  that destination, installs it, then launches it only after successful deployment.

Background operations continue with the window hidden. The menu shows the last
operation's status and disables conflicting game actions while busy or editing
settings. New operation errors restore the app window. **Exit Fleab** exits
instead of hiding; it cancels active deployment/download work and waits for safe
completion. Closing to the tray leaves that work running. Updating Fleab still
exits for the authenticated updater's restart.

Windows uses its notification area and macOS uses a native menu bar status
item. Linux uses StatusNotifierItem/D-Bus without requiring GTK/AppIndicator
libraries. A registered tray host is required (KDE Plasma, including Steam Deck
Desktop Mode, or an appropriate GNOME extension). Fleab keeps the window
accessible when no host is available and restores it if that host disappears.

Linux prefers X11/XWayland when `DISPLAY` is available: the current window
framework cannot hide/unhide a native Wayland window. On a Wayland-only system,
the menu can still be available, but hide/close-to-tray is disabled and the
settings page explains the limitation. Steam Deck Gaming Mode does not provide
this desktop tray workflow.

Preferences are stored in `desktop.json` beside `profiles.json`. They survive
app updates. Invalid settings are preserved and reported instead of overwritten.

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

## Application updates

Open the settings cog and select **Updates** to check GitHub Releases or turn
daily background checks off. A newer signed release offers **Download and
restart** when deployment is idle and profile edits are saved. Installation
keeps profiles and game files, retains a backup, and rolls back if the new app
cannot finish starting. See [application updates](updates.md) for recovery,
portable installation requirements and the first manual upgrade.

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
