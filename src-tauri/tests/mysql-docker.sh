#!/usr/bin/env bash
# The servers the MySQL server tests run against, in Docker:
#   src-tauri/tests/mysql-docker.sh up     starts them (or reuses them) and prints the variables
#   src-tauri/tests/mysql-docker.sh down   removes them
#
# Progress goes to stderr and the variables to stdout, so this works:
#   eval "$(src-tauri/tests/mysql-docker.sh up)"
#   cargo test --manifest-path src-tauri/Cargo.toml --lib mysql -- --ignored
# For MySQL 8.0, run the suite again with DBUNNY_TEST_MYSQL_URL set to DBUNNY_TEST_MYSQL80_URL.
#
# Only the containers named here (all dbunny-test-*) and the network dbunny-test-net are
# touched, and every port is bound to 127.0.0.1.

set -euo pipefail

NETWORK=dbunny-test-net
MYSQL84=dbunny-test-mysql84
MYSQL80=dbunny-test-mysql80
MARIADB=dbunny-test-mariadb
SSH=dbunny-test-ssh
CONTAINERS=("$MYSQL84" "$MYSQL80" "$MARIADB" "$SSH")

TMP_ROOT="${TMPDIR:-/tmp}"
SSL_DIR="${TMP_ROOT%/}/dbunny-test-mysql-ssl"

log() {
    echo "$@" >&2
}

exists() {
    docker container inspect "$1" >/dev/null 2>&1
}

running() {
    [ "$(docker container inspect -f '{{.State.Running}}' "$1" 2>/dev/null)" = "true" ]
}

# Starts container $1 with the rest as `docker run` arguments, or starts the one already there.
start() {
    local name="$1"
    shift
    if running "$name"; then
        log "$name is already running"
    elif exists "$name"; then
        log "starting $name"
        docker start "$name" >/dev/null
    else
        log "creating $name"
        docker run -d --name "$name" --network "$NETWORK" "$@" >/dev/null
    fi
}

# Waits until `docker exec $1 <the rest>` succeeds.
wait_for() {
    local name="$1"
    shift
    log "waiting for $name"
    for _ in $(seq 1 180); do
        if docker exec "$name" "$@" >/dev/null 2>&1; then
            return 0
        fi
        sleep 1
    done
    log "$name didn't get ready in 3 minutes"
    docker logs --tail 30 "$name" >&2 || true
    exit 1
}

# Waits until the SSH container finished setting up (users, the tunnel mod, sshd).
wait_for_ssh() {
    log "waiting for $SSH"
    for _ in $(seq 1 180); do
        if docker logs "$SSH" 2>&1 | grep -q "ls.io-init\] done"; then
            return 0
        fi
        sleep 1
    done
    log "$SSH didn't get ready in 3 minutes"
    docker logs --tail 30 "$SSH" >&2 || true
    exit 1
}

up() {
    if ! docker network inspect "$NETWORK" >/dev/null 2>&1; then
        log "creating network $NETWORK"
        docker network create "$NETWORK" >/dev/null
    fi

    start "$MYSQL84" -p 127.0.0.1:33084:3306 -e MYSQL_ROOT_PASSWORD=test mysql:8.4
    start "$MYSQL80" -p 127.0.0.1:33080:3306 -e MYSQL_ROOT_PASSWORD=test mysql:8.0
    start "$MARIADB" -p 127.0.0.1:33114:3306 -e MARIADB_ROOT_PASSWORD=test mariadb:11.4

    # The SSH server is optional: without its image the tunnel tests are skipped
    local ssh_ok=true
    if ! exists "$SSH" && ! docker image inspect linuxserver/openssh-server >/dev/null 2>&1; then
        if ! docker pull linuxserver/openssh-server >&2; then
            log "notice: couldn't pull linuxserver/openssh-server; the SSH tunnel tests won't run"
            ssh_ok=false
        fi
    fi
    if $ssh_ok; then
        start "$SSH" -p 127.0.0.1:32222:2222 \
            -e PASSWORD_ACCESS=true -e USER_NAME=dbunny -e USER_PASSWORD=test \
            -e DOCKER_MODS=linuxserver/mods:openssh-server-ssh-tunnel \
            linuxserver/openssh-server
    fi

    # Over TCP: while the image initializes, its temporary server only listens on the socket
    wait_for "$MYSQL84" mysqladmin ping -h 127.0.0.1 -uroot -ptest --silent
    wait_for "$MYSQL80" mysqladmin ping -h 127.0.0.1 -uroot -ptest --silent
    wait_for "$MARIADB" mariadb-admin ping -h 127.0.0.1 -uroot -ptest --silent
    if $ssh_ok; then
        wait_for_ssh
    fi

    # The certificates MySQL generated for itself; the 8.0 one has a different CA
    mkdir -p "$SSL_DIR"
    for file in ca.pem client-cert.pem client-key.pem; do
        docker cp "$MYSQL84:/var/lib/mysql/$file" "$SSL_DIR/$file" >/dev/null
    done
    docker cp "$MYSQL80:/var/lib/mysql/ca.pem" "$SSL_DIR/other-ca.pem" >/dev/null
    chmod 600 "$SSL_DIR/client-key.pem"

    echo "export DBUNNY_TEST_MYSQL_URL=mysql://root:test@127.0.0.1:33084"
    echo "export DBUNNY_TEST_MYSQL80_URL=mysql://root:test@127.0.0.1:33080"
    echo "export DBUNNY_TEST_MARIADB_URL=mysql://root:test@127.0.0.1:33114"
    echo "export DBUNNY_TEST_MYSQL_SSL_DIR=$SSL_DIR"
    if $ssh_ok; then
        echo "export DBUNNY_TEST_MYSQL_SSH=dbunny:test@127.0.0.1:32222"
        echo "export DBUNNY_TEST_MYSQL_SSH_TARGET=$MYSQL84:3306"
    fi
}

down() {
    for name in "${CONTAINERS[@]}"; do
        if exists "$name"; then
            log "removing $name"
            docker rm -f "$name" >/dev/null
        fi
    done
    if docker network inspect "$NETWORK" >/dev/null 2>&1; then
        log "removing network $NETWORK"
        docker network rm "$NETWORK" >/dev/null
    fi
}

case "${1:-}" in
    up) up ;;
    down) down ;;
    *)
        log "usage: $0 up|down"
        exit 2
        ;;
esac
