# Code signing policy

Fleab is maintained by [Harvey Ball (@thestonefox)](https://github.com/thestonefox).
Its source and build configuration are public in
[thestonefox/Fleab](https://github.com/thestonefox/Fleab).

## Current status

Windows publisher signing is disabled. The Foundation application was not
approved and Fleab is not enrolled. The optional SignPath integration is kept
for possible future use; no SignPath request or approval is part of normal
releases. Merge a PR and GitHub builds and publishes the release automatically.

The release workflow explicitly sets `WINDOWS_SIGNING_ENABLED: 'false'`.
Repository variables cannot enable it. Windows binaries have no publisher
signature, so Windows may still block or warn about them. The updater's
separate Ed25519 verification stays enabled and its key must be retained.

No SignPath secrets or variables are needed. If previously added, the
`SIGNPATH_API_TOKEN` secret and `SIGNPATH_ORGANIZATION_ID` and
`WINDOWS_SIGNING_ENABLED` repository variables can be removed. Keep
`FLEAB_UPDATE_PRIVATE_KEY`: it authenticates application updates.

After acceptance, attribution is: Free code signing provided by
[SignPath.io](https://signpath.io/), certificate by
[SignPath Foundation](https://signpath.org/). The certificate belongs to the
Foundation; Windows will identify that publisher. All contributors involved
in signing must use multi-factor authentication for GitHub and SignPath.

## Scope and responsibilities

If re-enabled, only Fleab's own Windows x64 and ARM64 executables, built by the
public release workflow on GitHub-hosted runners, would be submitted. The
Unity package, other
projects' binaries and users' game builds are outside this signing policy.
macOS packaging currently uses an ad-hoc signature; Developer ID and
notarisation are separate requirements.

Harvey Ball is the project's author, reviewer and release approver. External
contributions require maintainer review. Every production signing request
requires explicit approval in SignPath, as required by the
[Foundation's conditions](https://signpath.org/terms.html). The workflow has no
permission to approve its own requests. The SignPath API token is a submitter
token; it is not an approver credential.

## Optional future signing flow

The following applies only after publisher signing has been re-enabled.

1. Raise a PR and merge it into `main` after its checks pass.
2. GitHub assigns the SemVer tag and builds all six native targets.
3. Open the release run's **Summary**. The completed job named
   **Open this job's Summary for the SignPath approval link** contains a direct
   **Open SignPath and approve this release** link. SignPath also sends its
   configured approver notifications.
4. Open that link, sign in, check the Fleab version and linked GitHub build,
   then click **Approve**. One request covers both Windows architectures.
5. GitHub receives the signed executables, verifies trusted, timestamped
   Authenticode signatures, product versions and CPU architectures, then
   packages and publishes the release automatically.

No extra commit, tag, download or manual upload is needed. The signing request
job finishes before the waiting job starts, so its Summary link is available
while approval is pending.

The waiting job allows five hours for approval. If it times out, approve the
same request and choose **Re-run failed jobs** on that GitHub run. The waiting
job reuses its original request ID. A saved request also allows retrying a
failed submission job without a second request when its binaries are unchanged.
Changed binaries, tag or SignPath account cannot reuse that saved identity.
Rejected/cancelled signing requests fail publication; they are never bypassed.

## Re-enabling in the future

Do not perform this setup for normal unsigned releases. A production
certificate and provider acceptance are prerequisites.

1. Merge this policy and integration so SignPath can inspect them publicly.
2. Apply at [SignPath Foundation](https://signpath.org/apply). Provide the
   repository, this policy, [privacy policy](privacy.md) and existing releases.
   SignPath decides eligibility; acceptance is not automatic.
3. Finish account setup and enable MFA. Configure Harvey Ball as the release
   approver. Install the SignPath GitHub App with access to this repository,
   and link the **GitHub.com** trusted build system to the SignPath project.
4. Create project **`fleab`**, artifact configuration **`windows-v1`** using
   [the checked-in XML](../.github/signpath/windows.xml), and policy
   **`release-signing`** using the Foundation production certificate. Require
   manual approval by the maintainer. Allow only this repository's trusted
   release workflow, including its reusable-workflow invocation from main.
   Its immutable release snapshot is checked out by tag before building.
5. In GitHub **Settings > Secrets and variables > Actions**, add repository
   secret **`SIGNPATH_API_TOKEN`** for a submitter with access to that policy,
   and repository variable **`SIGNPATH_ORGANIZATION_ID`** with the account's UUID.
   The updater's separate **`FLEAB_UPDATE_PRIVATE_KEY`** secret remains required.
6. Once a production certificate and policy are ready, change the release
   workflow's **`WINDOWS_SIGNING_ENABLED`** value to **`'true'`** in a reviewed PR.
   A repository variable of that name is not used. Before enabling, releases
   continue
   with an explicit unsigned-status notice. After enabling, missing settings,
   failed signing or invalid signatures stop publication; there is no automatic
   unsigned fallback. Do not turn signing off merely to bypass a failed request.

Only release workflows use the signing credentials; ordinary PR checks do not
submit requests. The official GitHub action is pinned by commit and the
official PowerShell client by version. Saved request records contain IDs,
approval links and input hashes, never the API token.

Authenticode signing precedes both portable packaging and updater hashes.
The portable ZIP and updater TAR contain the same signed executable bytes.
The updater's Ed25519 signature authenticates its manifest independently of
Windows publisher trust. SmartScreen can still warn while publisher reputation
builds, even when the Authenticode signature is valid.

## Privacy, removal and reporting

See [privacy](privacy.md) for GitHub update checks, network requests and local
storage. Fleab is portable: close it and remove its extracted application
folder to uninstall it. Removing the app does not delete profiles or games.
Configuration, updater cache and retained backups can be removed separately
when no update or recovery is pending; see [application updates](updates.md).

Report suspicious signed releases to the maintainer through the repository and
to [SignPath support](https://signpath.org/terms.html). Maintainers will
investigate source/build provenance, stop affected releases and cooperate
with certificate revocation when required.
