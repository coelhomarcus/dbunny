#!/usr/bin/env bash
# Checks matrix-summary.sh against the logs in fixtures/matrix.
set -uo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
FIXTURES="$HERE/fixtures/matrix"
passed=0
failed=0

check() {
    local name="$1"
    shift
    if "$@"; then
        passed=$((passed + 1))
    else
        failed=$((failed + 1))
        echo "FAILED: $name" >&2
    fi
}

summary="$("$HERE/matrix-summary.sh" "$FIXTURES/postgres-17.log" "$FIXTURES/mysql-8.4.log" "$FIXTURES/mariadb-13.0.log")"
status=$?
line() {
    printf '%s\n' "$summary" | grep -E "^$1 +$2 "
}

summary_fixture_counts_skips() {
    line postgres 17 | grep -Eq '^postgres +17 +70 +0 +2 +41s +ok$'
}

summary_fixture_reports_failures() {
    line mysql 8.4 | grep -Eq '^mysql +8\.4 +94 +1 +0 +58s +FAILED$' && [ "$status" -eq 1 ]
}

summary_fixture_zero_tests_fails() {
    line mariadb 13.0 | grep -q 'FAILED (no tests ran)'
}

check summary_fixture_counts_skips summary_fixture_counts_skips
check summary_fixture_reports_failures summary_fixture_reports_failures
check summary_fixture_zero_tests_fails summary_fixture_zero_tests_fails

echo "$passed passed, $failed failed"
[ "$failed" -eq 0 ]
