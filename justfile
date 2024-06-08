_list:
    @just --list

# Check project formatting.
check:
    just --unstable --fmt --check
    npx -y prettier --check $(fd --type=file --hidden -e=md -e=yml)
    taplo lint
    cargo +nightly fmt -- --check

# Format project.
fmt:
    just --unstable --fmt
    nix fmt
    npx -y prettier --write $(fd --type=file --hidden -e=md -e=yml)
    taplo format
    cargo +nightly fmt

msrv := ```
    cargo metadata --format-version=1 \
    | jq -r 'first(.packages[] | select(.source == null and .rust_version)) | .rust_version' \
    | sed -E 's/^1\.([0-9]{2})$/1\.\1\.0/'
```
msrv_rustup := "+" + msrv

# Run tests on all crates in workspace using specified (or default) toolchain.
clippy toolchain="":
    cargo {{ toolchain }} clippy --workspace --all-targets --all-features

# Run tests on all crates in workspace using its MSRV.
test-msrv: (test msrv_rustup)

# Run tests on all crates in workspace using specified (or default) toolchain.
test toolchain="":
    cargo {{ toolchain }} hack --workspace test --no-fail-fast --no-default-features
    cargo {{ toolchain }} hack --workspace test --no-fail-fast
    cargo {{ toolchain }} hack --workspace test --no-fail-fast --all-features

# Run tests on all crates in workspace and produce coverage file (Codecov format).
test-coverage-codecov toolchain="":
    cargo {{ toolchain }} llvm-cov --workspace --all-features --codecov --output-path codecov.json

# Run tests on all crates in workspace and produce coverage file (lcov format).
test-coverage-lcov toolchain="":
    cargo {{ toolchain }} llvm-cov --workspace --all-features --lcov --output-path lcov.info

# Build rustdoc for all crates in workspace and watch for changes.
doc-watch:
    RUSTDOCFLAGS="--cfg=docsrs" cargo +nightly doc --no-deps --workspace --all-features --open
    cargo watch -- RUSTDOCFLAGS="--cfg=docsrs" cargo +nightly doc --no-deps --workspace --all-features
