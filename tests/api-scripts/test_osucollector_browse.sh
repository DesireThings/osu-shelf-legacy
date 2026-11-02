#!/usr/bin/env bash
# tests: osucollector.com API - GET /api/collections/{search,popularv2,recent}
# rust source: osu-downloader/src/collection.rs
# model: CollectionPage { collections: [CollectionSummary], hasMore, nextPageCursor, results }

set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/common.sh"

BASE="https://osucollector.com/api/collections"
SEARCH_TEXT="stream"
PER_PAGE=50

# fetch a listing into $body, or fail the check named by $2
get_page() {
    local url="$1" label="$2" resp status
    echo "--- GET $url"
    resp=$(curl -s -w "\n%{http_code}" -H "User-Agent: $UA" "$url")
    status=$(echo "$resp" | tail -1)
    body=$(echo "$resp" | head -n -1)
    check_http_status "$status" 200 "$label" || { echo "$body" | head -5; return 1; }
}

check_page_shape() {
    local label="$1" count bad
    count=$(echo "$body" | jq '.collections | length')
    if [[ "$count" -gt 0 && "$count" -le "$PER_PAGE" ]]; then
        pass "$label: $count collections"
    else
        fail "$label: expected 1..$PER_PAGE collections, got $count"
    fi

    has_more=$(echo "$body" | jq -r '.hasMore | type')
    [[ "$has_more" == "boolean" ]] && pass "$label: hasMore is boolean" || fail "$label: hasMore is $has_more"

    bad=$(echo "$body" | jq '[.collections[] | select(
        (.id | type) != "number"
        or (.name | type) != "string"
        or (.uploader.id | type) != "number"
        or (.uploader.username | type) != "string"
        or (.beatmapCount | type) != "number"
        or (.favourites | type) != "number"
        or (.dateUploaded._seconds | type) != "number"
        or (.modes | type) != "object"
        or (.difficultySpread | type) != "object"
    )] | length')
    [[ "$bad" == "0" ]] && pass "$label: every row has the model's fields" || fail "$label: $bad rows differ from the model"
}

# the field must never increase down the page
check_descending() {
    local jq_field="$1" label="$2" rising
    rising=$(echo "$body" | jq "[.collections[] | $jq_field] | [range(1; length) as \$i | select(.[\$i] > .[\$i - 1])] | length")
    [[ "$rising" == "0" ]] && pass "$label: sorted highest first" || fail "$label: $rising rows out of order"
}

echo "=== osucollector API: collection listings ==="

get_page "$BASE/search?search=$SEARCH_TEXT&perPage=$PER_PAGE" "search"
check_page_shape "search"
first_id=$(echo "$body" | jq '.collections[0].id')
cursor=$(echo "$body" | jq '.nextPageCursor')
total=$(echo "$body" | jq -r '.results | type')
[[ "$total" == "number" ]] && pass "search: results is a number" || fail "search: results is $total"

get_page "$BASE/search?search=$SEARCH_TEXT&perPage=$PER_PAGE&cursor=$cursor" "search next page"
check_page_shape "search next page"
[[ "$(echo "$body" | jq '.collections[0].id')" != "$first_id" ]] && pass "search: the cursor moves to a new page" || fail "search: the cursor returned the same page"

get_page "$BASE/search?search=$SEARCH_TEXT&sortBy=dateUploaded&orderBy=desc&perPage=$PER_PAGE" "search by newest"
check_descending ".dateUploaded._seconds" "search by newest"

get_page "$BASE/search?search=$SEARCH_TEXT&sortBy=favourites&orderBy=desc&perPage=$PER_PAGE" "search by favourites"
check_descending ".favourites" "search by favourites"

get_page "$BASE/search?search=$SEARCH_TEXT&sortBy=beatmapCount&orderBy=desc&perPage=$PER_PAGE" "search by beatmaps"
check_descending ".beatmapCount" "search by beatmaps"

for range in today week month year alltime; do
    get_page "$BASE/popularv2?range=$range&perPage=$PER_PAGE" "popular $range"
    check_page_shape "popular $range"
done

get_page "$BASE/recent?perPage=$PER_PAGE" "recent"
check_page_shape "recent"
check_descending ".dateUploaded._seconds" "recent"

echo ""
if [[ $FAILURES -eq 0 ]]; then
    echo "result: PASS"
else
    echo "result: FAIL ($FAILURES failure(s))"
    exit 1
fi
