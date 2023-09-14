_list:
    @just --list

# Run tests on all crates in workspace using its MSRV.
test-msrv:
    @just test +1.70.0

# Run tests on all crates in workspace using specified (or default) toolchain.
test toolchain="":
    cargo {{toolchain}} hack --workspace test --no-fail-fast --no-default-features
    cargo {{toolchain}} hack --workspace test --no-fail-fast
    cargo {{toolchain}} hack --workspace test --no-fail-fast --all-features

# Check project.
check:
    just --unstable --fmt --check
    npx -y prettier --check '**/*.md'
    taplo lint
    cargo +nightly fmt -- --check

# Format project.
fmt:
    just --unstable --fmt
    npx -y prettier --write '**/*.md'
    taplo format
    cargo +nightly fmt

# Build rustdoc for all crates in workspace and watch for changes.
doc-watch:
    RUSTDOCFLAGS="--cfg=docsrs" cargo +nightly doc --no-deps --workspace --all-features --open
    cargo watch -- RUSTDOCFLAGS="--cfg=docsrs" cargo +nightly doc --no-deps --workspace --all-features
