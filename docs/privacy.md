# Privacy

Fleab has no accounts, advertising, usage analytics or crash-report upload.
Game profiles, paths, launch options and deployment records are stored locally.
The Unity publisher writes builds to the folder or share you choose. The
desktop app reads that source and writes to your configured game destinations;
those files are not uploaded to Fleab's maintainer or a signing provider.

## GitHub update checks

The desktop app checks stable releases in `thestonefox/Fleab` on GitHub.
Daily background checks are enabled by default. You can disable them in
**Settings > Updates**, and check manually instead. Failed checks leave the
app usable. Downloading and installing an update requires your action.

Update requests use HTTPS and send the usual connection information to GitHub,
including your IP address and a `Fleab/<installed version>` user agent. Fleab
does not send profile names, local paths, game data or personal identifiers.
Downloads can use GitHub's release asset hosts. GitHub handles these requests
under its [privacy statement](https://docs.github.com/en/site-policy/privacy-policies/github-general-privacy-statement).
There is no separate Fleab telemetry endpoint. Opening external documentation
or release links uses your browser and the destination site's privacy policy.

## SignPath

SignPath is used by maintainers during release production, not by the running
desktop app. Build artifacts and source/build provenance are submitted for
signing. Maintainer account details and approval notifications are handled
under [SignPath's privacy policy](https://signpath.io/privacy-policy).
End users do not need a SignPath account. Windows may independently perform
certificate, reputation or malware checks according to its security settings.

## Local files

Profiles can be exported to JSON at your request. Those exports can include
paths and launch arguments; choose who receives them. Configured network
shares and game launchers have their own access controls and privacy behavior.
Deleting Fleab's application folder leaves local profiles and game data in
place. See [storage](deployer.md#storage) and [update recovery](updates.md) for
the configuration and cache locations.
