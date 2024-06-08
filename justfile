_list:
    @just --list

# Check project formatting.
check:
    just --unstable --fmt --check
    fd --hidden --type=file -e=md -e=yml --exec-batch prettier --check
    fd --hidden -e=toml --exec-batch taplo format --check
    fd --hidden -e=toml --exec-batch taplo lint
    cargo +nightly fmt -- --check

# Format project.
fmt:
    just --unstable --fmt
    nixpkgs-fmt .
    fd --hidden --type=file -e=md -e=yml --exec-batch prettier --write
    fd --type=file --hidden -e=toml --exec-batch taplo format
    cargo +nightly fmt

msrv := ```
    cargo metadata --format-version=1 \
    | jq -r 'first(.packages[] | select(.source == null and .rust_version)) | .rust_version' \
    | sed -E 's/^1\.([0-9]{2})$/1\.\1\.0/'
```
msrv_rustup := "+" + msrv

# Downgrade dev-dependencies necessary to run MSRV checks/tests.
[private]
downgrade-for-msrv:
    @ echo "No downgrades currently needed for MSRV testing"

# Run tests on all crates in workspace using specified (or default) toolchain.
clippy toolchain="":
    cargo {{ toolchain }} clippy --workspace --all-targets --all-features

# Run tests on all crates in workspace using its MSRV.
test-msrv: downgrade-for-msrv (test msrv_rustup)

# Run tests on all crates in workspace using specified (or default) toolchain.
test toolchain="":
    cargo {{ toolchain }} nextest run --no-default-features
    cargo {{ toolchain }} nextest run
    cargo {{ toolchain }} nextest run --all-features

# Run tests on all crates in workspace and produce coverage file (Codecov format).
test-coverage-codecov toolchain="":
    cargo {{ toolchain }} llvm-cov --workspace --all-features --codecov --output-path codecov.json

# Run tests on all crates in workspace and produce coverage file (lcov format).
test-coverage-lcov toolchain="":
    cargo {{ toolchain }} llvm-cov --workspace --all-features --lcov --output-path lcov.info

# Test workspace docs.
test-docs toolchain="": && doc
    cargo {{ toolchain }} test --doc --workspace --all-features --no-fail-fast -- --nocapture

# Document crates in workspace.
doc *args:
    RUSTDOCFLAGS="--cfg=docsrs -Dwarnings" cargo +nightly doc --no-deps --workspace --all-features {{ args }}

# Build rustdoc for all crates in workspace and watch for changes.
doc-watch:
    RUSTDOCFLAGS="--cfg=docsrs" cargo +nightly doc --no-deps --workspace --all-features --open
    cargo watch -- RUSTDOCFLAGS="--cfg=docsrs" cargo +nightly doc --no-deps --workspace --all-features
