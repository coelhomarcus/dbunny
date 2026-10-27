#!/usr/bin/env bash
# Checks that every server test (the #[ignore] ones) runs in the matrix: each must match the
# filters of some engine (servers.sh filters), so a typo in a filter can't leave tests out while
# the version still shows green. Reads the test list from stdin:
#   cargo test --manifest-path src-tauri/Cargo.toml --lib -- --ignored --list | src-tauri/tests/check-filters.sh
# Exits 1 and names the tests no filter matches.

set -uo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"

# Ignored tests that aren't server tests: they talk to the real assistant CLIs, need an account,
# and don't depend on the database's version
OUTSIDE_THE_MATRIX="assistant::claude_tests:: assistant::codex_tests::"

filters="$OUTSIDE_THE_MATRIX"
for engine in $("$HERE/servers.sh" list | awk '{print $1}' | sort -u); do
    filters="$filters $("$HERE/servers.sh" filters "$engine")"
done

total=0
missing=()
while IFS= read -r line; do
    case "$line" in
        *": test") ;;
        *) continue ;;
    esac
    name="${line%: test}"
    total=$((total + 1))
    covered=0
    for filter in $filters; do
        case "$name" in
            *"$filter"*)
                covered=1
                break
                ;;
        esac
    done
    [ "$covered" -eq 1 ] || missing+=("$name")
done

if [ "$total" -eq 0 ]; then
    echo "no tests in the list (expected the output of cargo test -- --ignored --list)" >&2
    exit 1
fi
if [ "${#missing[@]}" -gt 0 ]; then
    echo "${#missing[@]} of $total server tests match no engine's filters (servers.sh filters):" >&2
    printf '  %s\n' "${missing[@]}" >&2
    exit 1
fi
echo "all $total server tests are in the matrix"
