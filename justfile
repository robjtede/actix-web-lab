_list:
    @just --list

test-msrv:
    @just test +1.70.0

test toolchain="":
    cargo {{toolchain}} hack --workspace test --no-fail-fast --no-default-features
    cargo {{toolchain}} hack --workspace test --no-fail-fast
    cargo {{toolchain}} hack --workspace test --no-fail-fast --all-features

check:
    just --unstable --fmt --check
    npx -y prettier --check '**/*.md'
    taplo lint
    cargo +nightly fmt -- --check

fmt:
    just --unstable --fmt
    npx -y prettier --write '**/*.md'
    taplo format
    cargo +nightly fmt
