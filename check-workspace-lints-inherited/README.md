# Check Workspace Lints

GitHub Action that ensures every local crate in a Rust workspace inherits lints from the workspace manifest using:

```toml
[lints]
workspace = true
```

The workspace root must define `[workspace.lints]`.
