# Desktop application updates

Fleab checks the stable releases in
[thestonefox/Fleab](https://github.com/thestonefox/Fleab/releases) in the
background at most once a day. Open the settings cog, then **Updates**, to
check manually or turn automatic checks off. A failed check leaves the app
usable; the next daily check can retry. No GitHub account is required.

When a newer release is available, Fleab shows its version and release notes.
Choose **Download and restart** to install it. Updates never install silently.
Save pending profile edits and wait for any deployment to finish first.
Downloading can be cancelled; the final replacement requires a restart.

Existing releases without the updater require one manual download of an
updater-enabled version. The Unity package continues to use Package Manager
and release tags; the desktop updater does not update Unity packages or games.

## Verification and recovery

An update must have an Ed25519 signature from a key embedded in Fleab, match
this machine's operating system and CPU architecture, and pass archive and
executable SHA-256 checks. HTTPS downloads are restricted to GitHub and its
release asset hosts. Missing signatures offer a manual GitHub download link;
invalid signatures or corrupt packages cannot be installed.

A helper waits for Fleab to exit, replaces the executable (the entire `.app`
on macOS), and starts the updated version. The new app must load its profile
configuration and acknowledge startup. An early exit or missing acknowledgement
restores and restarts the previous version. Interrupted transactions are
checked again on the next launch. Recovery refuses to overwrite an unexpected
application file and leaves the backup available for manual recovery.

Profiles, archive sources, game installations and saved games are preserved.
The update preferences and recovery journal are separate from profiles.
Keep Fleab outside game folders; game folders must not be inside `Fleab.app`.

The app needs write access beside its executable or bundle. If it is on a
read-only volume, a mounted disk image, or a protected system location,
download and install the release manually instead. No elevation is requested.
Keep the original executable filename. The update replaces application code;
the README and documentation beside a portable executable are refreshed by
extracting a full portable release.

The most recent backup is retained in `.fleab-update-<id>` beside the app.
The downloaded package, license notices and transaction are retained in
Fleab's native cache directory under `updates/download-<id>`. A subsequent
successful update cleans older completed transactions for this installation,
unless a profile now references their folders. Do not remove these directories
while an update or recovery is pending. Settings and recovery reports live in
Fleab's normal configuration directory (`updates.json`, `update-pending.json`,
and `update-report.json`).

macOS packages currently have an ad-hoc code signature. Update signatures
authenticate Fleab's release payload; they do not replace Developer ID signing,
notarisation, or the operating system's application trust checks.

Windows publisher signing uses SignPath independently of the updater key.
After the one-time onboarding, each merged PR builds automatically and waits
for one SignPath approval before publication. See the
[code signing policy](code-signing.md) for the direct approval link and retry
instructions. Existing unsigned releases do not gain a signature retroactively.

## Maintainer signing setup

`deployer/update-keys.json` contains public verification keys. The matching
Ed25519 private PEM must be stored as the repository Actions secret
**FLEAB_UPDATE_PRIVATE_KEY** before the first updater-enabled release. GitHub's
**Settings > Secrets and variables > Actions > New repository secret** accepts
the complete PEM, including its BEGIN/END lines. Keep an offline backup of the
private key; never commit it or attach it to a release.

For this implementation the generated private PEM is local and ignored at
`.tools/update-signing/private-key.pem`. Only its public counterpart is tracked.
This file is not included in portable packages or Sandbox test inputs.

For a new installation of the signing infrastructure, generate a pair with
Node 24 (paths must not already exist):

```sh
node tools/update-feed.mjs keygen PRIVATE_PEM PUBLIC_KEYS_JSON
```

Install the public key map in `deployer/update-keys.json` before building apps,
and add the private PEM to the repository secret. Do not regenerate the key
for each release: installed apps trust the public keys compiled into them.
For rotation, first release an app trusting both public keys, then change the
signing key after those apps have been distributed. Older clients still using
only the retired key need a manual upgrade. The signer currently uses key ID
`fleab-updates-v1`; change it consistently when rotating keys.

The six native build jobs produce separate updater archives and metadata.
The publishing job verifies every archive against its metadata, matches the
private key to the tracked public key, and signs the exact manifest payload.
It uploads `fleab-update.json` and the six updater archives alongside the full
portable downloads. A missing or mismatched signing secret fails publication.

For a local signing check, set `FLEAB_UPDATE_PRIVATE_KEY` to the PEM in your
process environment, then run:

```sh
node tools/update-feed.mjs sign DIRECTORY vX.Y.Z
node --test tests/updates/feed.test.mjs
```

`DIRECTORY` must contain all six actual packages and their `.tar.gz.json`
metadata. The script never prints private key material. Remove the environment
variable when finished. See [release automation](releases.md) and the
[verification record](verification.md) for publication and platform coverage.
