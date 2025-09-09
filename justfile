_list:
    @just --list

# Check project formatting.
check: && clippy
    just --unstable --fmt --check
    fd --hidden --type=file -e=md -e=yml --exec-batch prettier --check
    fd --hidden -e=toml --exec-batch taplo format --check
    fd --hidden -e=toml --exec-batch taplo lint
    cargo +nightly fmt -- --check

# Format project.
[group("lint")]
fmt: update-readmes
    just --unstable --fmt
    nixpkgs-fmt .
    fd --hidden --type=file -e=md -e=yml --exec-batch prettier --write
    fd --type=file --hidden -e=toml --exec-batch taplo format
    cargo +nightly fmt

# Update READMEs from crate root documentation.
[group("lint")]
update-readmes:
    cd ./russe && cargo rdme --force
    cd ./err-report && cargo rdme --force

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

# Run tests on all crates in workspace using specified (or default) toolchain and watch for changes.
clippy-watch toolchain="":
    cargo watch -- just clippy {{ toolchain }}

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
doc *args: && doc-set-workspace-crates
    rm -f "$(cargo metadata --format-version=1 | jq -r '.target_directory')/doc/crates.js"
    RUSTDOCFLAGS="--cfg=docsrs -Dwarnings" cargo +nightly doc --no-deps --workspace --all-features {{ args }}

[private]
doc-set-workspace-crates:
    #!/usr/bin/env bash
    (
        echo "window.ALL_CRATES ="
        cargo metadata --format-version=1 \
        | jq '[.packages[] | select(.source == null) | .targets | map(select(.doc) | .name)] | flatten'
        echo ";"
    ) > "$(cargo metadata --format-version=1 | jq -r '.target_directory')/doc/crates.js"

# Build rustdoc for all crates in workspace and watch for changes.
doc-watch:
    RUSTDOCFLAGS="--cfg=docsrs" cargo +nightly doc --no-deps --workspace --all-features --open
    cargo watch -- RUSTDOCFLAGS="--cfg=docsrs" cargo +nightly doc --no-deps --workspace --all-features

# Check for unintentional external type exposure on all crates in workspace.
check-external-types-all toolchain="+nightly-2024-05-01":
    #!/usr/bin/env bash
    set -euo pipefail
    exit=0
    for f in $(find . -mindepth 2 -maxdepth 2 -name Cargo.toml | grep -vE "\-codegen/|\-derive/|\-macros/"); do
        if ! just check-external-types-manifest "$f" {{ toolchain }}; then exit=1; fi
        echo
        echo
    done
    exit $exit

# Check for unintentional external type exposure on all crates in workspace.
check-external-types-all-table toolchain="+nightly-2024-05-01":
    #!/usr/bin/env bash
    set -euo pipefail
    for f in $(find . -mindepth 2 -maxdepth 2 -name Cargo.toml | grep -vE "\-codegen/|\-derive/|\-macros/"); do
        echo
        echo "Checking for $f"
        just check-external-types-manifest "$f" {{ toolchain }} --output-format=markdown-table
    done

# Check for unintentional external type exposure on a crate.
check-external-types-manifest manifest_path toolchain="+nightly-2024-05-01" *extra_args="":
    cargo {{ toolchain }} check-external-types --manifest-path "{{ manifest_path }}" {{ extra_args }}
