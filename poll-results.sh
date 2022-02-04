#!/bin/sh

set -euo pipefail

F="$(mktemp)"
echo 'votes,feature,"issue url"' >> "$F"

gh issue list \
    --search="is:issue is:open sort:reactions-+1-desc" \
    --json="title,url,reactionGroups" \
    | jq -r '
        .[]
        | {
            title,
            url,
            votes: ((.reactionGroups[]? | select(.content == "THUMBS_UP") | .users.totalCount) // 0)
        }
        | "\(.votes),\"\(.title)\",\(.url)"
        ' \
    | sed -E 's/(.*)\[poll\] (.*)/\1\2/' >> "$F"

cat "$F" | xsv table
