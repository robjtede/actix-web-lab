_list:
    @just --list

test:
    cargo hack --workspace test --no-fail-fast --no-default-features
    cargo hack --workspace test --no-fail-fast
    cargo hack --workspace test --no-fail-fast --all-features

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
