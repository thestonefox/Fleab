# Development

Fleab is one repository: `unity-package/` is the UPM Editor extension and
`deployer/` is the native Rust application. `docs/archive-format.md` is the
interoperability contract. Both components are versioned together at 0.1.0.

Use Conventional Commits: `type(scope): subject`, a blank line, then a body
explaining the change. Every commit message line is limited to 72 characters.
Implement and validate changes in focused milestones.

Rust: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets
-- -D warnings`, `cargo test --workspace`, `cargo build --release`.

Unity integration requires Unity 6 with desktop platform modules. Target is
Unity 6.6. Standalone C# archive tests do not replace Editor integration tests.

