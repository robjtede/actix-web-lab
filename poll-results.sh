#!/bin/sh

set -euo pipefail

F="$(mktemp)"

HAS_XSV="$(command -v xsv)"
# HAS_XSV=""

if [ "$(command -v gh)" ]; then
    echo "This script requires the GitHub CLI."
    echo "https://cli.github.com"
    exit 1
fi


if [ "$HAS_XSV" ]; then
    echo 'votes,feature,"issue url"' >> "$F"
else
    echo "votes \tfeature \tissue url" >> "$F"
fi

gh issue list \
    --repo="robjtede/actix-web-lab" \
    --search="is:issue is:open sort:reactions-+1-desc" \
    --json="title,url,reactionGroups" \
    --jq '
        .[]
        | {
            title,
            url,
            votes: ((.reactionGroups[]? | select(.content == "THUMBS_UP") | .users.totalCount) // 0)
        }
        | "\(.votes),\"\(.title)\",\(.url)"
        ' \
    | sed -E 's/(.*)\[poll\] (.*)/\1\2/' >> "$F"

if [ "$HAS_XSV" ]; then
    cat "$F" | xsv table
else
    cat "$F" | awk -F "\"*,\"*" '{ print $1 "\t" $2 "\t" $3 }'
fi
