# Wine, Proton and Steam Deck

Configure a Windows client/server destination on Linux, deploy a Windows
archive, then select a runner under **Settings > Launch options > Windows builds
on Linux > Compatibility settings**.
Deployment needs neither Steam nor Wine. Launch needs a working runner and
its system/runtime dependencies.

Automatic discovery searches Wine and wine64 on PATH and these Steam roots:
`~/.steam/steam`, `~/.steam/root`, `~/.local/share/Steam`, and the Flatpak Steam
data location. Steam's `steamapps/libraryfolders.vdf` adds external libraries.
Proton installations are detected in `steamapps/common/*/proton` and
`compatibilitytools.d/*/proton`. Install a runner through your normal Steam or
distribution workflow; Fleab does not download runners.

Automatic mode selects the first detected matching runner. For predictable
behaviour, choose a particular detected runner. Its kind, path and Steam root
are saved. A manual executable requires selecting Wine or Proton rather than
Auto. Paths containing spaces are passed directly to the process API.

Wine receives `WINEPREFIX`. Proton receives `STEAM_COMPAT_DATA_PATH` (the
compatibility data directory, not its `pfx` child),
`STEAM_COMPAT_CLIENT_INSTALL_PATH`, `STEAM_COMPAT_INSTALL_PATH`, `SteamAppId=0`
and `SteamGameId=0`. By default Proton is invoked with `run`, followed by the
Windows executable and game arguments. A nonempty runner argument override
replaces this default verb; for example one line `waitforexitandrun`.

Prefixes are isolated per profile and runner family under Fleab's conventional
user data directory. Provide an explicit prefix directory to use another
location. It is created only when Launch is clicked. Avoid sharing one prefix
between concurrent unrelated games. Game arguments and runner arguments use
one literal argument per line; no shell command is constructed.

Some Proton releases rely on a Steam Linux Runtime or Steam session unavailable
outside Steam. Ensure Steam has completed its first-run/runtime setup and
choose an appropriate installed compatibility tool. Runner discovery and
environment setup are implemented; no hardware-specific Proton compatibility
claim is made. Third-party anti-cheat, DRM, runtime requirements and game
compatibility remain the responsibility of the game and runner.

Troubleshooting:

* **No runner found:** install Wine/Proton, Refresh, or specify its absolute path.
* **Manual Proton needs Steam root:** choose the Steam client installation
  directory, which contains `steamapps`, not the Proton tool's directory.
* **Prefix error:** choose a writable absolute compatibility data directory.
* **Linux ARM64:** normal x86 Windows Wine/Proton compatibility is not provided
  by Fleab. Use a runner that supports the actual executable architecture;
  deployment remains available.
* **Running game not detected through Wine:** close its game and launcher
  processes manually before deploying. Do not rely on process names alone.

Reference: [Valve Proton source](https://github.com/ValveSoftware/Proton) and
[Wine documentation](https://www.winehq.org/documentation).
