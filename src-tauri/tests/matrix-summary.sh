#!/usr/bin/env bash
# The summary of server test logs named <engine>-<version>.log, one line per log:
#   src-tauri/tests/matrix-summary.sh src-tauri/target/matrix/*.log
# passed and failed come from cargo's "test result:" line, skipped counts the dbunny-skip lines
# (tests that returned early because the server lacks what they check) and time the
# dbunny-matrix-time line matrix.sh writes. A version where no test ran counts as failed.
# Exits 1 when any version failed.

set -uo pipefail

[ $# -gt 0 ] || {
    echo "usage: $0 <engine>-<version>.log..." >&2
    exit 2
}

any_failed=0
printf '%-8s %-8s %6s %6s %7s %6s  %s\n' engine version passed failed skipped time result
for log in "$@"; do
    base="$(basename "$log" .log)"
    engine="${base%%-*}"
    version="${base#*-}"
    if [ ! -f "$log" ]; then
        printf '%-8s %-8s %6s %6s %7s %6s  %s\n' "$engine" "$version" - - - - "FAILED (no log)"
        any_failed=1
        continue
    fi
    result_line="$(grep -E '^test result: ' "$log" | tail -1)"
    passed="$(printf '%s' "$result_line" | sed -nE 's/.* ([0-9]+) passed;.*/\1/p')"
    failed="$(printf '%s' "$result_line" | sed -nE 's/.* ([0-9]+) failed;.*/\1/p')"
    passed="${passed:-0}"
    failed="${failed:-0}"
    skipped="$(grep -c '^dbunny-skip:' "$log")"
    seconds="$(sed -nE 's/^dbunny-matrix-time: ([0-9]+)$/\1/p' "$log" | tail -1)"
    time="${seconds:+${seconds}s}"
    if [ $((passed + failed)) -eq 0 ]; then
        result="FAILED (no tests ran)"
        any_failed=1
    elif [ "$failed" -gt 0 ]; then
        result=FAILED
        any_failed=1
    else
        result=ok
    fi
    printf '%-8s %-8s %6s %6s %7s %6s  %s\n' "$engine" "$version" "$passed" "$failed" "$skipped" "${time:--}" "$result"
done
exit "$any_failed"
