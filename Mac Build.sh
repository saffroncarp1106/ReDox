#!/bin/sh
set -e

case "$#:${1-}" in
    0:|1:--release) ;;
    *)
        printf 'Usage: %s [--release]\n' "$0" >&2
        exit 2
        ;;
esac

CDPATH= cd -- "$(dirname -- "$0")"
exec cargo run --locked --bin redox --target-dir "$PWD/target" "$@"
