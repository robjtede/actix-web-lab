#!/usr/bin/env bash

set -eEuo pipefail

if ! awk '/^\[workspace\.lints(\.|])/{ found = 1 } END { exit found ? 0 : 1 }' Cargo.toml; then
    echo "Workspace manifest does not define [workspace.lints]" >&2
    exit 1
fi

exit_code=0

while IFS= read -r manifest_path; do
    if ! awk '
        /^\[/ { in_lints = ($0 == "[lints]") }
        in_lints && /^[[:space:]]*workspace[[:space:]]*=[[:space:]]*true([[:space:]]*(#.*)?)?$/ { found = 1 }
        END { exit found ? 0 : 1 }
    ' "$manifest_path"; then
        printf 'Crate manifest does not inherit workspace lints: %s\n' "${manifest_path#"$PWD"/}" >&2
        exit_code=1
    fi
done < <(
    cargo --frozen metadata --no-deps --format-version=1 \
        | jq -r '.packages[] | select(.source == null) | .manifest_path'
)

exit "$exit_code"
