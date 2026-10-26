#!/usr/bin/env bash
# Debian packages for the CI runners, from a folder of .deb files the workflow keeps in the
# Actions cache, so a slow Ubuntu mirror is met once instead of in every job:
#   src-tauri/tests/ci-apt.sh download <folder> <package>...   fills the folder when it is empty
#   src-tauri/tests/ci-apt.sh install <folder> <package>...    the same, then installs from it
#
# The folder holds what the runner image lacks, without recommended extras. When installing from
# it fails (a newer runner image wanting other versions), apt installs from the mirror instead.

set -euo pipefail

[ $# -ge 3 ] || {
    echo "usage: $0 download|install <folder> <package>..." >&2
    exit 2
}
mode="$1"
dir="$2"
shift 2
apt=(-o Acquire::Retries=3 -o Acquire::http::Timeout=30)

has_debs() {
    compgen -G "$dir/*.deb" >/dev/null
}

fill() {
    if has_debs; then
        echo "$(ls "$dir"/*.deb | wc -l) packages from the cache" >&2
        return
    fi
    mkdir -p "$dir/partial"
    sudo apt-get "${apt[@]}" update
    sudo apt-get "${apt[@]}" install -y --no-install-recommends --download-only -o Dir::Cache::archives="$dir" "$@"
    sudo rm -rf "$dir/partial" "$dir/lock"
    sudo chown -R "$(id -u):$(id -g)" "$dir"
    echo "downloaded $(find "$dir" -name '*.deb' | wc -l) packages" >&2
}

case "$mode" in
    download) fill "$@" ;;
    install)
        fill "$@"
        # Nothing downloaded: the image already has every package
        if has_debs && ! sudo dpkg -i "$dir"/*.deb >/dev/null; then
            echo "the cached packages didn't install; using the mirror" >&2
            sudo apt-get "${apt[@]}" update
            sudo apt-get "${apt[@]}" install -y -f --no-install-recommends "$@"
        fi
        ;;
    *)
        echo "usage: $0 download|install <folder> <package>..." >&2
        exit 2
        ;;
esac
