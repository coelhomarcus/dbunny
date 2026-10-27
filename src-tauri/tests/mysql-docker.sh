#!/usr/bin/env bash
# A shortcut kept for habit: the MySQL 8.4 and 8.0 and MariaDB 10.6 and 11.4 servers at once,
# through servers.sh, which starts any version of the test matrix one at a time:
#   src-tauri/tests/mysql-docker.sh up     starts them (or reuses them) and prints the variables
#   src-tauri/tests/mysql-docker.sh down   removes every dbunny-test-* container
#
#   eval "$(src-tauri/tests/mysql-docker.sh up)"
#   cargo test --manifest-path src-tauri/Cargo.toml --lib -- mysql:: mysql_common:: --ignored
# That runs the suite against MySQL 8.4. For another version use servers.sh, whose variables
# (certificates and SSH target included) belong to the server it started, or matrix.sh for all of
# them: pointing DBUNNY_TEST_MYSQL_URL at another server here leaves the TLS and SSH tests on 8.4's.

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
