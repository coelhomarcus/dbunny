#!/usr/bin/env bash
# The database servers the server tests run against, one version at a time, in Docker:
#   src-tauri/tests/servers.sh up <engine> <version>     starts it (or reuses it) and prints the variables
#   src-tauri/tests/servers.sh env <engine> <version>    prints the variables of one already running
#   src-tauri/tests/servers.sh down <engine> <version>   removes it
#   src-tauri/tests/servers.sh down all                  removes every dbunny-test-* container and the network
#   src-tauri/tests/servers.sh filters <engine>          the cargo test filters of that engine's server tests
#   src-tauri/tests/servers.sh list                      the matrix, one "<engine> <version>" per line
#   src-tauri/tests/servers.sh prune                     down all, and the matrix images too (about 13 GB)
#
# Engines and versions (the test matrix):
#   postgres 13 14 15 16 17 18
#   mysql    8.0 8.4 9.7 26.10
#   mariadb  10.6 10.11 11.4 11.8 12.3 13.0
#
# Progress goes to stderr and the variables to stdout, so this works:
#   eval "$(src-tauri/tests/servers.sh up postgres 17)"
#   cargo test --manifest-path src-tauri/Cargo.toml --lib -- $(src-tauri/tests/servers.sh filters postgres) --ignored
#
# Only containers named dbunny-test-* and the network dbunny-test-net are touched, and every
# port is bound to 127.0.0.1.

set -euo pipefail

# Bump when the way a container is made changes: containers made by an older script are recreated
SCRIPT_CONFIG=2

NETWORK=dbunny-test-net
SSH=dbunny-test-ssh
SSH_PORT=32222
PREFIX=dbunny-test-
# The MySQL server a MariaDB run also starts, for the tests that open the wrong engine
NEIGHBOUR_MYSQL=26.10

SERVER_EXTENSIONS="subjectAltName=DNS:localhost,IP:127.0.0.1
extendedKeyUsage=serverAuth"
CLIENT_EXTENSIONS="extendedKeyUsage=clientAuth"

TMP_ROOT="${TMPDIR:-/tmp}"
TMP_ROOT="${TMP_ROOT%/}"
PG_SSL_DIR="$TMP_ROOT/dbunny-test-pg-ssl"
SSH_KEYS_DIR="$TMP_ROOT/dbunny-test-ssh-keys"

log() {
    echo "$@" >&2
}

die() {
    log "$@"
    exit 1
}

exists() {
    docker container inspect "$1" >/dev/null 2>&1
}

running() {
    [ "$(docker container inspect -f '{{.State.Running}}' "$1" 2>/dev/null)" = "true" ]
}

# Whether something listens on 127.0.0.1:$1.
port_busy() {
    (exec 3<>"/dev/tcp/127.0.0.1/$1") 2>/dev/null
}

# The name of the container of engine $1, version $2.
container() {
    case "$1" in
        postgres) echo "${PREFIX}pg${2//./}" ;;
        mysql) echo "${PREFIX}mysql${2//./}" ;;
        mariadb) echo "${PREFIX}mariadb${2//./}" ;;
        *) die "unknown engine \"$1\"; use postgres, mysql or mariadb" ;;
    esac
}

# The port on 127.0.0.1 of engine $1, version $2; versions outside the matrix are refused.
port() {
    case "$1 $2" in
        "postgres 13" | "postgres 14" | "postgres 15" | "postgres 16" | "postgres 17" | "postgres 18") echo "354$2" ;;
        "mysql 8.0") echo 33080 ;;
        "mysql 8.4") echo 33084 ;;
        "mysql 9.7") echo 33097 ;;
        "mysql 26.10") echo 32610 ;;
        "mariadb 10.6") echo 33106 ;;
        "mariadb 10.11") echo 33111 ;;
        "mariadb 11.4") echo 33114 ;;
        "mariadb 11.8") echo 33118 ;;
        "mariadb 12.3") echo 33123 ;;
        "mariadb 13.0") echo 33130 ;;
        *) die "$1 $2 isn't in the test matrix (see the top of $0)" ;;
    esac
}

# Starts container $1 publishing port $2, with the rest as `docker run` arguments; reuses
# one already running, starts one stopped, and recreates one made by an older script.
start() {
    local name="$1" host_port="$2"
    shift 2
    if exists "$name"; then
        local made_by
        made_by="$(docker container inspect -f '{{index .Config.Labels "dbunny.test.config"}}' "$name")"
        if [ "$made_by" != "$SCRIPT_CONFIG" ]; then
            log "recreating $name (made by an older servers.sh)"
            docker rm -f -v "$name" >/dev/null
        fi
    fi
    if running "$name"; then
        log "$name is already running"
        return
    fi
    if port_busy "$host_port"; then
        die "port $host_port on 127.0.0.1 is already in use by something that isn't $name"
    fi
    if exists "$name"; then
        log "starting $name"
        docker start "$name" >/dev/null
    else
        log "creating $name"
        docker run -d --name "$name" --network "$NETWORK" --label "dbunny.test.config=$SCRIPT_CONFIG" \
            -p "127.0.0.1:$host_port:${CONTAINER_PORT}" "$@" >/dev/null
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
        if ! running "$name"; then
            docker logs --tail 30 "$name" >&2 || true
            die "$name stopped while starting"
        fi
        sleep 1
    done
    docker logs --tail 30 "$name" >&2 || true
    die "$name didn't get ready in 3 minutes"
}

network() {
    if ! docker network inspect "$NETWORK" >/dev/null 2>&1; then
        log "creating network $NETWORK"
        docker network create "$NETWORK" >/dev/null
    fi
}

# Two SSH keys the tunnel tests log in with: id_plain, and id_locked with the passphrase "testphrase".
make_ssh_keys() {
    log "making SSH keys in $SSH_KEYS_DIR"
    rm -rf "$SSH_KEYS_DIR"
    mkdir -p "$SSH_KEYS_DIR/public"
    ssh-keygen -q -t ed25519 -N "" -C dbunny-test-plain -f "$SSH_KEYS_DIR/id_plain"
    ssh-keygen -q -t ed25519 -N testphrase -C dbunny-test-locked -f "$SSH_KEYS_DIR/id_locked"
    cp "$SSH_KEYS_DIR/id_plain.pub" "$SSH_KEYS_DIR/id_locked.pub" "$SSH_KEYS_DIR/public/"
}

# Removes the containers whose names start with $1, because files they were made with were made
# again ($2 says which), for example after the system cleared the temporary folder.
remove_stale() {
    local name
    for name in $(docker ps -a --format '{{.Names}}'); do
        case "$name" in
            "$1"*)
                log "recreating $name ($2 were made again)"
                docker rm -f -v "$name" >/dev/null
                ;;
        esac
    done
}

# The SSH server every engine's tunnel tests go through; it reaches the databases by container name.
ssh_server() {
    # Not in a condition, where set -e would let a failing ssh-keygen through
    if [ ! -f "$SSH_KEYS_DIR/id_plain" ] || [ ! -f "$SSH_KEYS_DIR/id_locked" ]; then
        make_ssh_keys
        remove_stale "$SSH" "its keys"
    fi
    CONTAINER_PORT=2222 start "$SSH" "$SSH_PORT" \
        -e PASSWORD_ACCESS=true -e USER_NAME=dbunny -e USER_PASSWORD=test \
        -e PUBLIC_KEY_DIR=/keys -v "$SSH_KEYS_DIR/public:/keys:ro" \
        -e DOCKER_MODS=linuxserver/mods:openssh-server-ssh-tunnel \
        linuxserver/openssh-server
    log "waiting for $SSH"
    for _ in $(seq 1 180); do
        if docker logs "$SSH" 2>&1 | grep -q "ls.io-init\] done"; then
            return 0
        fi
        sleep 1
    done
    docker logs --tail 30 "$SSH" >&2 || true
    die "$SSH didn't get ready in 3 minutes"
}

# Signs a certificate for $2 (a subject) with the CA $1, writing $3.pem and $3.key; $4 are its
# X509v3 extensions, one per line. macOS checks server certificates by Apple's rules: they need
# extendedKeyUsage=serverAuth and at most 825 days.
sign() {
    local ca="$1" subject="$2" out="$3" extensions="$4"
    openssl genrsa -out "$out.key" 2048 2>/dev/null
    openssl req -new -key "$out.key" -subj "$subject" -out "$out.csr" 2>/dev/null
    printf '%s\n' "$extensions" >"$out.ext"
    openssl x509 -req -in "$out.csr" -CA "$ca.pem" -CAkey "$ca.key" -CAcreateserial -days 825 -extfile "$out.ext" -out "$out.pem" 2>/dev/null
    rm -f "$out.csr" "$out.ext"
}

# A self-signed CA, $1.pem and $1.key.
make_ca() {
    openssl req -x509 -newkey rsa:2048 -nodes -keyout "$1.key" -out "$1.pem" -days 3650 -subj "/CN=$2" 2>/dev/null
}

# The PostgreSQL certificates: a server certificate for localhost and 127.0.0.1 signed by ca.pem,
# another CA, and a client certificate for certuser as PEM, PKCS#8 and PKCS#1.
pg_certificates() {
    if [ -f "$PG_SSL_DIR/client.key" ]; then
        return
    fi
    log "making PostgreSQL certificates in $PG_SSL_DIR"
    # Every PostgreSQL container copied the old ones into its data folder when it was made
    remove_stale "${PREFIX}pg" "its certificates"
    rm -rf "$PG_SSL_DIR"
    mkdir -p "$PG_SSL_DIR"
    make_ca "$PG_SSL_DIR/ca" "dbunny test CA"
    make_ca "$PG_SSL_DIR/other-ca" "dbunny other CA"
    sign "$PG_SSL_DIR/ca" "/CN=localhost" "$PG_SSL_DIR/server" "$SERVER_EXTENSIONS"
    sign "$PG_SSL_DIR/ca" "/CN=certuser" "$PG_SSL_DIR/client" "$CLIENT_EXTENSIONS"
    openssl pkcs8 -topk8 -nocrypt -in "$PG_SSL_DIR/client.key" -out "$PG_SSL_DIR/client.pk8.pem"
    openssl rsa -in "$PG_SSL_DIR/client.pk8.pem" -traditional -out "$PG_SSL_DIR/client.key" 2>/dev/null
    chmod 600 "$PG_SSL_DIR"/*.key "$PG_SSL_DIR/client.pk8.pem"
    # The init script reads it as the postgres user, whose id a Linux host doesn't share; the copy
    # it makes in the data folder is the one the server uses, at 600
    chmod 644 "$PG_SSL_DIR/server.key"
    # What the server runs when it first starts: SSL on (the key has to belong to postgres,
    # so it is copied into the data folder), certuser by certificate only, and the fixtures
    mkdir -p "$PG_SSL_DIR/init"
    cat >"$PG_SSL_DIR/init/10-dbunny.sh" <<'INIT'
#!/usr/bin/env bash
set -e
cp /dbunny-ssl/server.pem /dbunny-ssl/server.key /dbunny-ssl/ca.pem "$PGDATA/"
chmod 600 "$PGDATA/server.key"
cat >>"$PGDATA/postgresql.conf" <<CONF
ssl = on
ssl_cert_file = 'server.pem'
ssl_key_file = 'server.key'
ssl_ca_file = 'ca.pem'
CONF
sed -i '1i hostssl all certuser all cert' "$PGDATA/pg_hba.conf"
psql -v ON_ERROR_STOP=1 --username postgres <<SQL
CREATE ROLE demo LOGIN PASSWORD 'demo';
CREATE DATABASE demo OWNER demo;
CREATE ROLE certuser LOGIN;
CREATE DATABASE dbunny_second;
SQL
INIT
    # Docker Desktop shows mounted files as executable, so the image runs it instead of sourcing it
    chmod 755 "$PG_SSL_DIR/init/10-dbunny.sh"
}

# The folder a Linux host shares with the container for the Unix socket test; empty elsewhere.
pg_socket_dir() {
    if [ "$(uname -s)" = "Linux" ]; then
        echo "$TMP_ROOT/dbunny-test-pg$1-sock"
    fi
}

up_postgres() {
    local version="$1" name host_port socket
    name="$(container postgres "$version")"
    host_port="$(port postgres "$version")"
    socket="$(pg_socket_dir "$version")"
    network
    pg_certificates
    local extra=() command=()
    if [ -n "$socket" ]; then
        # A folder of its own, not /var/run/postgresql: the image would take that one over for its
        # user, and the next run on this host could neither reuse nor remove it
        mkdir -p "$socket"
        chmod 777 "$socket"
        extra=(-v "$socket:/dbunny-sock")
        command=(postgres -c "unix_socket_directories=/var/run/postgresql,/dbunny-sock")
    fi
    CONTAINER_PORT=5432 start "$name" "$host_port" \
        -e POSTGRES_PASSWORD=test \
        -v "$PG_SSL_DIR:/dbunny-ssl:ro" -v "$PG_SSL_DIR/init:/docker-entrypoint-initdb.d:ro" \
        ${extra[@]+"${extra[@]}"} \
        "postgres:$version" ${command[@]+"${command[@]}"}
    ssh_server
    # Over TCP: while the image initializes, its temporary server only listens on the socket
    wait_for "$name" pg_isready -h 127.0.0.1 -U postgres -q
    if [ -z "$socket" ]; then
        log "notice: no Unix socket test on Docker Desktop; a_unix_socket_opens_a_session will skip"
    fi
    env_postgres "$version"
}

env_postgres() {
    local version="$1" name host_port socket
    name="$(container postgres "$version")"
    host_port="$(port postgres "$version")"
    socket="$(pg_socket_dir "$version")"
    running "$name" || die "$name isn't running; use: $0 up postgres $version"
    echo "export DBUNNY_TEST_DATABASE_URL=postgresql://postgres:test@127.0.0.1:$host_port/postgres"
    echo "export DBUNNY_TEST_SSL_DIR=$PG_SSL_DIR"
    echo "export DBUNNY_TEST_SSL_PORT=$host_port"
    echo "export DBUNNY_TEST_SSH=dbunny:test@127.0.0.1:$SSH_PORT"
    echo "export DBUNNY_TEST_SSH_KEYS=$SSH_KEYS_DIR"
    echo "export DBUNNY_TEST_SSH_TARGET=$name:5432"
    echo "export DBUNNY_TEST_SSH_DATABASE_URL=postgresql://postgres:test@$name:5432/postgres"
    if [ -n "$socket" ]; then
        echo "export DBUNNY_TEST_SOCKET_URL=postgresql://postgres:test@${socket//\//%2F}:5432/postgres"
    fi
}

# The MySQL client files the TLS tests read: the CA and client certificate the server made for
# itself, and a CA that didn't sign it.
mysql_ssl_dir() {
    echo "$TMP_ROOT/dbunny-test-$1$2-ssl"
}

# A CA that signed none of the servers, for the "wrong CA" tests.
other_ca() {
    local dir="$TMP_ROOT/dbunny-test-other-ca"
    if [ ! -f "$dir/other-ca.pem" ]; then
        mkdir -p "$dir"
        make_ca "$dir/other-ca" "dbunny other CA"
    fi
    echo "$dir/other-ca.pem"
}

# MariaDB doesn't write certificates of its own: the script makes them, with localhost and
# 127.0.0.1 named, and the server is started with them.
mariadb_certificates() {
    local dir="$1" name="$2"
    if [ -f "$dir/client-key.pem" ]; then
        return
    fi
    log "making MariaDB certificates in $dir"
    # The server loaded the old ones when it started
    remove_stale "$name" "its certificates"
    rm -rf "$dir"
    mkdir -p "$dir"
    make_ca "$dir/ca" "dbunny test CA"
    sign "$dir/ca" "/CN=localhost" "$dir/server" "$SERVER_EXTENSIONS"
    sign "$dir/ca" "/CN=dbunny-client" "$dir/client" "$CLIENT_EXTENSIONS"
    mv "$dir/server.pem" "$dir/server-cert.pem"
    mv "$dir/server.key" "$dir/server-key.pem"
    mv "$dir/client.pem" "$dir/client-cert.pem"
    mv "$dir/client.key" "$dir/client-key.pem"
    # The server reads its key as the mysql user, whose id a Linux host doesn't share
    chmod 644 "$dir/server-key.pem"
    chmod 600 "$dir/client-key.pem" "$dir/ca.key"
    cp "$(other_ca)" "$dir/other-ca.pem"
}

up_mysql() {
    local version="$1" name host_port dir
    name="$(container mysql "$version")"
    host_port="$(port mysql "$version")"
    dir="$(mysql_ssl_dir mysql "$version")"
    network
    CONTAINER_PORT=3306 start "$name" "$host_port" -e MYSQL_ROOT_PASSWORD=test "mysql:$version"
    ssh_server
    # Over TCP: while the image initializes, its temporary server only listens on the socket
    wait_for "$name" mysqladmin ping -h 127.0.0.1 -uroot -ptest --silent
    rm -rf "$dir"
    mkdir -p "$dir"
    local file
    for file in ca.pem client-cert.pem client-key.pem; do
        docker cp "$name:/var/lib/mysql/$file" "$dir/$file" >/dev/null
    done
    chmod 600 "$dir/client-key.pem"
    cp "$(other_ca)" "$dir/other-ca.pem"
    env_mysql "$version"
}

up_mariadb() {
    local version="$1" name host_port dir
    name="$(container mariadb "$version")"
    host_port="$(port mariadb "$version")"
    dir="$(mysql_ssl_dir mariadb "$version")"
    network
    mariadb_certificates "$dir" "$name"
    CONTAINER_PORT=3306 start "$name" "$host_port" -e MARIADB_ROOT_PASSWORD=test \
        -v "$dir:/dbunny-ssl:ro" "mariadb:$version" \
        --ssl-ca=/dbunny-ssl/ca.pem --ssl-cert=/dbunny-ssl/server-cert.pem --ssl-key=/dbunny-ssl/server-key.pem
    ssh_server
    wait_for "$name" mariadb-admin ping -h 127.0.0.1 -uroot -ptest --silent
    # The refusal tests open a MySQL server with the MariaDB driver: the newest MySQL is the neighbour
    if [ "${DBUNNY_TEST_NEIGHBOUR:-1}" = 1 ]; then
        DBUNNY_TEST_NEIGHBOUR=0 up_mysql "$NEIGHBOUR_MYSQL" >/dev/null
    fi
    env_mariadb "$version"
}

# The variables the MySQL and MariaDB suites read; $1 is the engine, $2 the version.
env_server() {
    local engine="$1" version="$2" name host_port
    name="$(container "$engine" "$version")"
    host_port="$(port "$engine" "$version")"
    running "$name" || die "$name isn't running; use: $0 up $engine $version"
    echo "export DBUNNY_TEST_MYSQL_URL=mysql://root:test@127.0.0.1:$host_port"
    echo "export DBUNNY_TEST_MYSQL_SSL_DIR=$(mysql_ssl_dir "$engine" "$version")"
    echo "export DBUNNY_TEST_MYSQL_SSH=dbunny:test@127.0.0.1:$SSH_PORT"
    echo "export DBUNNY_TEST_MYSQL_SSH_TARGET=$name:3306"
}

env_mysql() {
    env_server mysql "$1"
    echo "export DBUNNY_TEST_MYSQL80_URL=mysql://root:test@127.0.0.1:$(port mysql "$1")"
}

env_mariadb() {
    env_server mariadb "$1"
    echo "export DBUNNY_TEST_MARIADB_URL=mysql://root:test@127.0.0.1:$(port mariadb "$1")"
    if running "$(container mysql "$NEIGHBOUR_MYSQL")"; then
        echo "export DBUNNY_TEST_MYSQL80_URL=mysql://root:test@127.0.0.1:$(port mysql "$NEIGHBOUR_MYSQL")"
    fi
}

down_one() {
    local name
    name="$(container "$1" "$2")"
    port "$1" "$2" >/dev/null
    if exists "$name"; then
        log "removing $name"
        docker rm -f -v "$name" >/dev/null
    fi
}

# Containers go with their anonymous volumes (-v): the database images keep their data in one,
# and each run would otherwise leave a few hundred MB behind.
down_all() {
    local name
    for name in $(docker ps -a --format '{{.Names}}'); do
        case "$name" in
            "$PREFIX"*)
                log "removing $name"
                docker rm -f -v "$name" >/dev/null
                ;;
        esac
    done
    if docker network inspect "$NETWORK" >/dev/null 2>&1; then
        log "removing network $NETWORK"
        docker network rm "$NETWORK" >/dev/null
    fi
    rm -rf "$TMP_ROOT"/dbunny-test-*
}

# The test matrix, in the order matrix.sh and the CI run it.
list() {
    local version
    for version in 13 14 15 16 17 18; do echo "postgres $version"; done
    for version in 8.0 8.4 9.7 26.10; do echo "mysql $version"; done
    for version in 10.6 10.11 11.4 11.8 12.3 13.0; do echo "mariadb $version"; done
}

# down all, and also the images of the matrix and of the SSH server: frees the disk, and the next
# run downloads them again.
prune() {
    down_all
    local engine version image
    while read -r engine version; do
        image="$engine:$version"
        if docker image inspect "$image" >/dev/null 2>&1; then
            log "removing image $image"
            docker image rm "$image" >/dev/null
        fi
    done < <(list)
    if docker image inspect linuxserver/openssh-server >/dev/null 2>&1; then
        log "removing image linuxserver/openssh-server"
        docker image rm linuxserver/openssh-server >/dev/null
    fi
}

filters() {
    case "$1" in
        postgres) echo "postgres:: net::ssh" ;;
        mysql) echo "mysql:: mysql_common::" ;;
        mariadb) echo "mariadb:: mysql_common::" ;;
        *) die "unknown engine \"$1\"; use postgres, mysql or mariadb" ;;
    esac
}

usage() {
    log "usage: $0 up|env|down <engine> <version> | down all | filters <engine> | list | prune"
    exit 2
}

case "${1:-}" in
    up | env)
        [ $# -eq 3 ] || usage
        port "$2" "$3" >/dev/null
        case "$2" in
            postgres | mysql | mariadb) "$1_$2" "$3" ;;
            *) die "unknown engine \"$2\"; use postgres, mysql or mariadb" ;;
        esac
        ;;
    down)
        if [ "${2:-}" = "all" ] && [ $# -eq 2 ]; then
            down_all
        else
            [ $# -eq 3 ] || usage
            down_one "$2" "$3"
        fi
        ;;
    filters)
        [ $# -eq 2 ] || usage
        filters "$2"
        ;;
    list)
        [ $# -eq 1 ] || usage
        list
        ;;
    prune)
        [ $# -eq 1 ] || usage
        prune
        ;;
    *) usage ;;
esac
