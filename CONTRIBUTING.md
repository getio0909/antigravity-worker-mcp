# Contributing

Use an existing Rust toolchain meeting the declared minimum version. Build with the committed lock file:

```bash
cargo fmt --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-features
cargo build --locked --release --bin antigravity-worker-mcp
```

Host-mode tests run on Linux, macOS and Windows. Linux isolation tests also need Bubblewrap, Git and unprivileged user namespaces. Tests use a native local CLI fixture and do not invoke a provider model or require credentials. Real-provider checks are optional and must record their actual scope and results separately. CI covers x86-64 and ARM64 natively for each operating system.

For behavior changes, update the protocol, RPD, verification record and changelog. Add tests for externally observable failures and permission boundaries. Do not include private configuration, credentials, raw conversations, real task instructions, or sensitive source material in issues, commits or fixtures. Use small synthetic examples.

Describe the concrete failure and resulting behavior in pull requests, with the checks performed. Preserve existing client behavior unless a versioned change is documented. Keep dependency features and runtime requirements limited to the implemented scope.

Use English for documentation, comments, public examples, issue templates and release notes. Keep Unicode test data in escaped literals when the test needs non-ASCII characters.
