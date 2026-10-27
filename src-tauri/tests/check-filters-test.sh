#!/usr/bin/env bash
# Checks check-filters.sh against the test lists in fixtures/filters.
set -uo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
FIXTURES="$HERE/fixtures/filters"
passed=0
failed=0

check() {
    if "$2"; then
        passed=$((passed + 1))
    else
        failed=$((failed + 1))
        echo "FAILED: $1" >&2
    fi
}

every_server_test_has_a_filter() {
    "$HERE/check-filters.sh" <"$FIXTURES/covered.txt" >/dev/null 2>&1
}

a_test_no_filter_matches_is_named() {
    local out
    out="$("$HERE/check-filters.sh" <"$FIXTURES/uncovered.txt" 2>&1)"
    [ $? -eq 1 ] && printf '%s\n' "$out" | grep -q 'db::sqlserver::server_tests::a_new_engine_nobody_filtered'
}

an_empty_list_fails() {
    ! "$HERE/check-filters.sh" </dev/null >/dev/null 2>&1
}

check every_server_test_has_a_filter every_server_test_has_a_filter
check a_test_no_filter_matches_is_named a_test_no_filter_matches_is_named
check an_empty_list_fails an_empty_list_fails

echo "$passed passed, $failed failed"
[ "$failed" -eq 0 ]
