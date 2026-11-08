# Fleab Publisher

Open **Tools > Fleab > Settings**, choose a local or mounted network publishing
directory, and enable automatic publishing. Build normally with Build Profiles.
Fleab packages successful desktop players and Dedicated Servers as `.tar.zst`.

Leave Build name override empty to use Product Name. Version comes from Player
Settings. Output is discovered from the completed BuildReport; no output path
setting or custom Build button is required.

**Publish Last Build** works with automatic publishing disabled. **Retry
Publish** uses the existing successful output after a compression/share failure.
Unity's original output and older archives remain intact.

Settings live in `ProjectSettings/FleabSettings.asset`. Last-build retry records
live in `Library/Fleab/`. Unity 6.6 BuildHistory can recover an absent record.
Packaging uses bundled managed libraries; users do not install TAR/Zstandard.

See the repository's `docs/publisher.md` and `docs/archive-format.md` for output
discovery, metadata, naming rules, history recovery and platform limitations.
