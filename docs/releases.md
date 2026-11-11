# Automatic semantic releases

The release rules use semantic-release's official `@semantic-release/commit-analyzer`
and `@semantic-release/release-notes-generator` plugins, the Conventional Commits
preset, and the standard `semver` package. Versions are locked in
`tools/release-engine/package-lock.json`; the adapter contains no separate
Conventional Commit parser or SemVer increment implementation.

Shared actions such as `cycjimmy/semantic-release-action` wrap the same tooling.
Fleab calls the plugins as libraries so it can read each PR's original commits,
preserve the exact per-PR release policy after squash/rebase merges, and stamp
both Rust and UPM package versions in an immutable snapshot. The npm publishing
pipeline is unused; its peer dependencies are omitted from installation.

Push the initial version once using the existing tag workflow:

```sh
git push origin main
git tag -a v0.1.0 -m "Release Fleab 0.1.0"
git push origin v0.1.0
```

After that, every PR merged into `main` receives an automatic stable SemVer
release. The highest-impact original commit in that PR determines its bump:

| PR commits | Release change | Example from 1.2.3 |
|---|---|---|
| Any breaking change | major, reset minor/patch | 2.0.0 |
| Otherwise any `feat` | minor, reset patch | 1.3.0 |
| Otherwise, including `fix`, `refactor`, `docs` | patch | 1.2.4 |

This policy also applies before 1.0: a breaking PR after 0.1.0 produces 1.0.0.
Other types (`test`, `build`, `ci`, `chore`, `perf`, etc.) and nonconventional
subjects default to patch, so every merged PR qualifies. Direct pushes to main
run validation but do not allocate an automatic release.

Breaking-change syntax:

```text
feat!: replace the profile schema
feat(profiles)!: replace the profile schema
```

Or put this footer in any commit's body:

```text
BREAKING CHANGE: existing profiles require migration.
```

`BREAKING-CHANGE:` is also recognised. Conventional scopes are optional; commits
without one appear under `general`. The original PR commits are fetched through
paginated GraphQL, including messages beyond the REST endpoint's 250-commit
limit. Squash or rebase merging therefore retains the bump information even
when the resulting main-branch subject omits `feat` or `!`.

## Release notes

Notes contain the PR link and a type → scope → subject hierarchy, for example:

```markdown
- **feat**
  - **publisher**
    - Add managed compression (commit link)
- **fix**
  - **deployer**
    - Handle disconnected shares (commit link)
  - **publisher**
    - Preserve original output (commit link)
```

Breaking subjects receive an explicit marker. Subjects and scopes are escaped
as display text; commit bodies are used for breaking detection but are not
published wholesale. Commit links refer to the original PR commits.

## Versions, tags and protected main

Versions are allocated from the highest stable `vX.Y.Z` Git tag, not the
development version stored on main. The workflow creates an annotated tag on a
release commit whose parent is that PR's exact merge commit. This snapshot
updates `Cargo.toml`, Fleab's `Cargo.lock` entry and `unity-package/package.json`
together and stores its release notes and PR identity under `.github/`.

Main's files, working tree and index are untouched. Its manifests can retain
the initial development version; the tag contains the authoritative released
version. UPM users should pin a release tag. This avoids bot commits/bypass
permissions on protected main while producing correctly versioned binaries and
Unity packages. Tags never move, and reruns do not allocate another version for
the same PR. A later manually created stable tag acts as an explicit new
baseline for already-included PRs.

Version allocation is serialised. Each run catches up all merged PRs since the
initial/manual baseline, so merges are not lost if queued runs are replaced or
reordered. Pending tags whose builds failed are retried. Catch-up builds run in
batches of at most 32 releases; successful batches dispatch a continuation when
more remain. A failed batch stops rather than repeatedly consuming build time.

## Builds and publication

Windows Authenticode signing has a one-time SignPath onboarding step. Once
enabled, publication pauses for one maintainer approval in SignPath covering
both Windows architectures. The completed request job's Summary has the direct
approval link; the waiting job resumes automatically after approval. This is
separate from PR review. See the [code signing policy](code-signing.md).

Before publishing an updater-enabled release, add the matching Ed25519 PEM as
the repository Actions secret **FLEAB_UPDATE_PRIVATE_KEY**. Its public key is
embedded in the desktop app. See [signing setup and recovery](updates.md).
The private key must remain stable across releases; keep an offline backup.

The automation calls the existing reusable six-platform release workflow
directly. This is necessary because a `GITHUB_TOKEN` tag push does not trigger
another push workflow. No personal access token, GitHub App, central service or
release PR approval is required.

Each release builds/tests Windows, Linux and macOS on x64/ARM64, packages the
portable artifacts, SHA-256 files and dedicated updater archives, and uploads
assets to a draft. The publishing job authenticates the six updater archives
and signs `fleab-update.json` with the repository secret. Only after
all six builds and asset uploads succeed does it publish the release. Artifact
names include the version to isolate concurrent catch-up builds. An incomplete
draft can be resumed; already published assets are preserved. GitHub's semantic
version-aware legacy latest policy handles releases finishing out of order.

In GitHub, enable Actions and allow the workflow's requested token permissions:
contents write for tags/releases, pull requests read for commit history, and
actions write only for large-backlog continuation dispatch. Tag rules must let
the Actions bot create `v*` tags. Main branch protection can remain unchanged.
The initial tag must be on main's history, and native runner availability still
applies to all six builds.

If a build or upload fails, fix the cause and rerun the failed Actions jobs.
Alternatively run **Release merged pull requests** manually from Actions to
retry every reserved/unpublished release and catch up any unprocessed merges.
Do not delete or retarget reserved tags to reuse a version.

Local verification uses isolated Git repositories and mocked API responses:

```sh
npm ci --prefix tools/release-engine --ignore-scripts --omit=peer
python -m unittest discover -s tests/release -v
```

Use Node 24 and Python 3.11 or newer for these automation tests. They are
development/CI dependencies only; the portable Fleab app needs neither runtime.

These tests do not publish, push to GitHub or change this repository's tags.
Live token/ruleset permissions and hosted runner execution must be verified by
the first real merged-PR release.

References: [workflow token triggers](https://docs.github.com/en/actions/how-tos/write-workflows/choose-when-workflows-run/trigger-a-workflow),
[commit analyzer](https://github.com/semantic-release/commit-analyzer),
[release notes generator](https://github.com/semantic-release/release-notes-generator),
[reusable workflows](https://docs.github.com/en/actions/how-tos/reuse-automations/reuse-workflows),
[release API](https://docs.github.com/en/rest/releases/releases).
