#!/usr/bin/env bash
# A shortcut kept for habit: the MySQL 8.4 and 8.0 and MariaDB 10.6 and 11.4 servers at once,
# through servers.sh, which starts any version of the test matrix one at a time:
#   src-tauri/tests/mysql-docker.sh up     starts them (or reuses them) and prints the variables
#   src-tauri/tests/mysql-docker.sh down   removes every dbunny-test-* container
#
#   eval "$(src-tauri/tests/mysql-docker.sh up)"
#   cargo test --manifest-path src-tauri/Cargo.toml --lib -- mysql:: mysql_common:: --ignored
# For MySQL 8.0, MariaDB 10.6 and MariaDB 11.4, run the suite again with DBUNNY_TEST_MYSQL_URL set to
# DBUNNY_TEST_MYSQL80_URL, DBUNNY_TEST_MARIADB106_URL or DBUNNY_TEST_MARIADB_URL; or use
# src-tauri/tests/matrix.sh, which runs every version.

set -euo pipefail

SERVERS="$(dirname "$0")/servers.sh"

# The value of variable $2 in the export lines $1.
value() {
    printf '%s\n' "$1" | sed -n "s/^export $2=//p"
}

up() {
    local mysql84 mysql80 mariadb106 mariadb114
    mysql84="$("$SERVERS" up mysql 8.4)"
    mysql80="$("$SERVERS" up mysql 8.0)"
    mariadb106="$(DBUNNY_TEST_NEIGHBOUR=0 "$SERVERS" up mariadb 10.6)"
    mariadb114="$(DBUNNY_TEST_NEIGHBOUR=0 "$SERVERS" up mariadb 11.4)"
    echo "export DBUNNY_TEST_MYSQL_URL=$(value "$mysql84" DBUNNY_TEST_MYSQL_URL)"
    echo "export DBUNNY_TEST_MYSQL80_URL=$(value "$mysql80" DBUNNY_TEST_MYSQL_URL)"
    echo "export DBUNNY_TEST_MARIADB106_URL=$(value "$mariadb106" DBUNNY_TEST_MYSQL_URL)"
    echo "export DBUNNY_TEST_MARIADB_URL=$(value "$mariadb114" DBUNNY_TEST_MYSQL_URL)"
    echo "export DBUNNY_TEST_MYSQL_SSL_DIR=$(value "$mysql84" DBUNNY_TEST_MYSQL_SSL_DIR)"
    echo "export DBUNNY_TEST_MYSQL_SSH=$(value "$mysql84" DBUNNY_TEST_MYSQL_SSH)"
    echo "export DBUNNY_TEST_MYSQL_SSH_TARGET=$(value "$mysql84" DBUNNY_TEST_MYSQL_SSH_TARGET)"
}

case "${1:-}" in
    up) up ;;
    down) "$SERVERS" down all ;;
    *)
        echo "usage: $0 up|down" >&2
        exit 2
        ;;
esac
