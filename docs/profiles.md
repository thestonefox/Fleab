# Folders and profiles

The Unity publisher writes build archives to a folder. Each test computer reads
those archives and deploys selected builds to its own installation folders.
The publishing machine and test computer do not need to be online together.

## Connect a publishing folder

Choose a local folder, network share or mounted drive in **Tools > Fleab >
Settings** in Unity. For example:

| Location | Example |
|---|---|
| Local folder | `D:\Published\MyGame` |
| Windows network share | `\\buildbox\builds\MyGame` |
| Mapped drive | `Z:\MyGame` |
| Linux mount | `/mnt/builds/MyGame` |
| macOS mount | `/Volumes/Builds/MyGame` |

Mount and authenticate shares through the operating system. Fleab uses your
existing filesystem access and does not ask for network-share credentials.
The same share can have different paths on different computers. Set each
computer's archive source to its own path to that share.

For a disconnected test machine, copy the archives to a local folder and use
that as the source. Archives are ordinary files; no Fleab service is required.

## Create a profile

Open the desktop app's settings cog, choose **New profile**, and set:

1. **Name**: the name you want to see in the game selector.
2. **Published archives**: the directory containing your `.tar.zst` files.
3. **Client destinations**: a dedicated installation folder for each enabled OS.
4. **Server destinations**, if needed: separate folders for dedicated servers.

Blank destinations disable their targets in the build list. Save the profile,
select a published build, and click **Deploy**. **Launch** starts the installed
build. Selecting an archive does not install it or start the game.

### Example Linux or Steam Deck profile

| Setting | Value |
|---|---|
| Name | My multiplayer game |
| Published archives | `/mnt/builds/MyGame` |
| Windows client | `/home/deck/Games/MyGame/Windows` |
| Linux client | `/home/deck/Games/MyGame/Linux` |
| macOS client | blank |
| Windows server | blank |
| Linux server | `/home/deck/Games/MyGame/Server` |
| macOS server | blank |

Windows builds on Linux need a compatible Wine or Proton runner to launch.
See the [compatibility guide](proton.md) for runner setup and architecture limits.

All enabled targets share one newest-first build selector. Architectures share
a destination: deploying a different architecture replaces that target's
installation. Discovery does not filter builds by the host architecture.
Client and server installations are independent.

## Choose installation folders

Use empty, dedicated folders for game installations. Fleab replaces its own
installations and refuses to replace unrelated files. Archive sources and
destinations must not overlap; destinations must not share or nest inside one
another. Roots, home directories and protected system locations are rejected.

Published archives stay in their source folder. Fleab stages and validates each
deployment before replacing the previous installation. Cancel is available
during copying and extraction. Running games block replacement; Fleab never
terminates them. See [replacement and recovery](deployer.md#replacement-and-recovery).

## Share and maintain profiles

Profiles are stored in the normal user configuration directory. They survive
replacing the Fleab executable. Settings supports editing, deleting and JSON
import/export. Imports receive a new identity and carry no local installation
state; edit their paths before saving. Deleting a profile leaves installed files
in place. See [configuration storage](deployer.md#storage) for exact paths.

To upgrade Fleab, extract the new portable release and replace the executable or
`.app`. Keep your configuration and deployment folders. The desktop app does not
need Rust, .NET, TAR or Zstandard installed. Versions with the updater also
offer [signed application updates](updates.md) through **Settings > Updates**.

For executable overrides and game arguments, open **Settings > Launch options**.
Native Windows, Linux and macOS players launch on their matching operating
systems. Dedicated servers use their matching server destinations.
