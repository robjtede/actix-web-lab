#!/bin/sh

set -euxo pipefail

gh issue create \
    --title="[poll] $1" \
    --body="React to this issue with a \":+1:\" to vote for this feature. Highest voted features will graduate to Actix Web sooner." \
    --label="poll"
