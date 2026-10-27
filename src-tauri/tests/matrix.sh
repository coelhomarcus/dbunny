#!/usr/bin/env bash
# Runs the server tests on every database version of the test matrix, one version at a time:
#   src-tauri/tests/matrix.sh                     every version
#   src-tauri/tests/matrix.sh postgres            every PostgreSQL version
#   src-tauri/tests/matrix.sh mariadb 11.8        one version
#   ... --keep                                    leaves the containers running
#
# Each version: servers.sh up, cargo test with that engine's filters and --ignored, servers.sh down.
# Logs go to src-tauri/target/matrix/<engine>-<version>.log and the summary to
# src-tauri/target/matrix/summary.md. Exits 1 when any version failed.

set -uo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
OUT="$ROOT/src-tauri/target/matrix"
SERVERS="$HERE/servers.sh"

engine=""
version=""
keep=0
for arg in "$@"; do
    case "$arg" in
        --keep) keep=1 ;;
        *)
            if [ -z "$engine" ]; then
                engine="$arg"
            elif [ -z "$version" ]; then
                version="$arg"
            else
                echo "usage: $0 [engine] [version] [--keep]" >&2
                exit 2
            fi
            ;;
    esac
done

selected=""
while read -r e v; do
    if [ -n "$engine" ] && [ "$e" != "$engine" ]; then
        continue
    fi
    if [ -n "$version" ] && [ "$v" != "$version" ]; then
        continue
    fi
    selected="$selected $e:$v"
done < <("$SERVERS" list)
if [ -z "$selected" ]; then
    echo "nothing in the matrix matches \"$engine $version\"" >&2
    exit 2
fi

mkdir -p "$OUT"
echo "building the tests" >&2
cargo test --manifest-path "$ROOT/src-tauri/Cargo.toml" --lib --no-run --quiet || exit 1
# Every server test has to match some engine's filters, or it would never run here
cargo test --manifest-path "$ROOT/src-tauri/Cargo.toml" --lib --quiet -- --ignored --list 2>/dev/null | "$HERE/check-filters.sh" >&2 || exit 1

logs=()
for entry in $selected; do
    e="${entry%%:*}"
    v="${entry#*:}"
    log="$OUT/$e-$v.log"
    logs+=("$log")
    echo "== $e $v" >&2
    started=$(date +%s)
    if variables="$("$SERVERS" up "$e" "$v" 2>"$log.servers")"; then
        (
            eval "$variables"
            # shellcheck disable=SC2046
            cargo test --manifest-path "$ROOT/src-tauri/Cargo.toml" --lib -- $("$SERVERS" filters "$e") --ignored --nocapture
        ) >"$log" 2>&1
    else
        cat "$log.servers" >"$log"
    fi
    rm -f "$log.servers"
    echo "dbunny-matrix-time: $(($(date +%s) - started))" >>"$log"
    if [ "$keep" -eq 0 ]; then
        "$SERVERS" down "$e" "$v" 2>/dev/null
    fi
    grep -E '^test result: ' "$log" | tail -1 >&2 || echo "no test result, see $log" >&2
done

if [ "$keep" -eq 0 ]; then
    "$SERVERS" down all 2>/dev/null
fi

"$HERE/matrix-summary.sh" "${logs[@]}" >"$OUT/summary.md"
status=$?
cat "$OUT/summary.md"
exit "$status"
