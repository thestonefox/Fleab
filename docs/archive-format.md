# Fleab archive contract, version 1

All platforms use a single Zstandard frame containing a POSIX TAR stream.
Consumers accept USTAR and PAX long UTF-8 names. No external tools are needed.
Archive roots contain the player files, or the macOS `.app` itself, directly.
There is no wrapper directory and no central manifest.

## Names

`<game>_<version>_<UTC>_<platform>[_Server][~<collision>].tar.zst`

* Game and version are nonempty UTF-8 strings, percent-encoded by byte. Only
  ASCII letters, digits, `-` and `.` remain literal. Underscores, percent signs,
  spaces and all other bytes use uppercase `%HH`. `.` and `..` are encoded.
* UTC is a real Gregorian timestamp in `YYYYMMDD-HHMMSS` form.
* Platforms: `Windows64`, `WindowsARM64`, `Linux64`, `LinuxARM64`, `macOS`,
  `macOS64`, `macOSARM64`. `macOS` means a universal Intel/ARM player.
* Optional `_Server` records the actual Unity standalone server subtarget.
* A collision uses `~` followed by a lowercase UUID without hyphens. It follows
  the platform/server token. It never changes the timestamp or replaces a file.
* Parse the full grammar; do not split unescaped product names heuristically.

Example: `My%5FGame_1.2.16_20261108-093000_Linux64_Server.tar.zst`.
The filename is the build identity. Architectures share OS/client-server targets.

## Entries and safety

Names use `/`, are relative, and contain no empty, `.` or `..` components,
backslashes, drive letters, NTFS streams, reserved device names or controls.
Every entry is a directory, regular file, or relative symbolic link. Hardlinks,
devices, FIFOs, sparse files and duplicate/case-colliding names are rejected.
Symlinks must resolve lexically inside the archive and point to an existing
non-link entry. They are created only after regular files have been extracted.
No entry may have a symlink as a parent. This conservative rule rejects link
chains while preserving normal bundle links to real framework directories.
On Windows, creating symbolic links may require Developer Mode or the OS
privilege; an unavailable privilege fails staging without changing installation.

Modes retain ordinary Unix rwx bits (no setuid/setgid/sticky). Directories need
owner rwx for staging. When the publisher runs on Windows, known entry points,
`.so` libraries and `Contents/MacOS` files receive 0755, other files 0644.
On Unix, file modes are preserved. Symlink payloads are never dereferenced.

Publisher compression level is 3. The Deployer accepts standard Zstandard
compression levels. Compression settings are not encoded in filenames.
Publishers validate compressed output and copy to a unique `.partial` file,
flush it, and rename without overwrite. Discovery accepts only complete names.

The Deployer limits expanded contents to 1 TiB, 250,000 entries and 4096 bytes
per path. It rejects empty archives and requires a unique Unity player entry
point (or a validated profile override) before replacing an installation.

