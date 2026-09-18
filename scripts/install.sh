#!/bin/sh
set -eu

binary=target/release/carli
destination=/usr/local/bin/carli
shells_file=/etc/shells

usage() {
    echo "usage: $0 [--binary PATH] [--destination ABSOLUTE_PATH] [--shells-file ABSOLUTE_PATH]" >&2
    exit "${1:-2}"
}

while [ "$#" -gt 0 ]; do
    case "$1" in
        --binary|--destination|--shells-file)
            [ "$#" -ge 2 ] || usage
            option=$1
            value=$2
            shift 2
            case "$option" in
                --binary) binary=$value ;;
                --destination) destination=$value ;;
                --shells-file) shells_file=$value ;;
            esac
            ;;
        -h|--help)
            usage 0
            ;;
        *)
            usage
            ;;
    esac
done

case "$destination" in /*) ;; *) echo "carli install: destination must be absolute" >&2; exit 2 ;; esac
case "$shells_file" in /*) ;; *) echo "carli install: shells file must be absolute" >&2; exit 2 ;; esac
case "$destination$shells_file" in *'
'*) echo "carli install: paths must not contain newlines" >&2; exit 2 ;; esac

[ -f "$binary" ] || { echo "carli install: binary not found: $binary" >&2; exit 1; }
[ -x "$binary" ] || { echo "carli install: binary is not executable: $binary" >&2; exit 1; }
[ -f "$shells_file" ] || { echo "carli install: shells file is not a regular file: $shells_file" >&2; exit 1; }
[ ! -L "$shells_file" ] || { echo "carli install: refusing symlinked shells file: $shells_file" >&2; exit 1; }

destination_directory=$(dirname "$destination")
shells_directory=$(dirname "$shells_file")
[ -d "$destination_directory" ] || { echo "carli install: destination directory does not exist: $destination_directory" >&2; exit 1; }
[ -d "$shells_directory" ] || { echo "carli install: shells directory does not exist: $shells_directory" >&2; exit 1; }
destination_directory=$(cd -P "$destination_directory" && pwd)
shells_directory=$(cd -P "$shells_directory" && pwd)
destination=$destination_directory/$(basename "$destination")
shells_file=$shells_directory/$(basename "$shells_file")
[ ! -L "$destination" ] || { echo "carli install: refusing symlinked destination: $destination" >&2; exit 1; }
if [ -e "$destination" ] && [ ! -f "$destination" ]; then
    echo "carli install: destination exists and is not a regular file: $destination" >&2
    exit 1
fi

staged_binary=$(mktemp "$destination_directory/.carli-install.XXXXXX")
staged_shells=$(mktemp "$shells_directory/.shells-install.XXXXXX")
backup_binary=
binary_committed=0
had_binary=0

cleanup() {
    status=$?
    if [ "$status" -ne 0 ] && [ "$binary_committed" -eq 1 ]; then
        if [ "$had_binary" -eq 1 ]; then
            mv -f -- "$backup_binary" "$destination"
        else
            rm -f -- "$destination"
        fi
    fi
    rm -f -- "$staged_binary" "$staged_shells"
    if [ -n "$backup_binary" ]; then
        rm -f -- "$backup_binary"
    fi
    trap - EXIT HUP INT TERM
    exit "$status"
}
trap cleanup EXIT
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM

install -m 0755 -- "$binary" "$staged_binary"
"$staged_binary" -c 'exit 0'

chmod --reference="$shells_file" "$staged_shells"
chown --reference="$shells_file" "$staged_shells"
awk -v shell="$destination" '$0 != shell { print } END { print shell }' "$shells_file" > "$staged_shells"

if [ -f "$destination" ]; then
    had_binary=1
    backup_binary=$(mktemp "$destination_directory/.carli-backup.XXXXXX")
    cp -p -- "$destination" "$backup_binary"
fi

mv -f -- "$staged_binary" "$destination"
binary_committed=1
mv -f -- "$staged_shells" "$shells_file"
binary_committed=0

echo "Installed $destination and registered it in $shells_file"
