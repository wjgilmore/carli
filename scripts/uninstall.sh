#!/bin/sh
set -eu

destination=/usr/local/bin/carli
shells_file=/etc/shells
passwd_file=/etc/passwd
remove_binary=0

usage() {
    echo "usage: $0 [--destination ABSOLUTE_PATH] [--shells-file ABSOLUTE_PATH] [--passwd-file ABSOLUTE_PATH] [--remove-binary]" >&2
    exit "${1:-2}"
}

while [ "$#" -gt 0 ]; do
    case "$1" in
        --destination|--shells-file|--passwd-file)
            [ "$#" -ge 2 ] || usage
            option=$1
            value=$2
            shift 2
            case "$option" in
                --destination) destination=$value ;;
                --shells-file) shells_file=$value ;;
                --passwd-file) passwd_file=$value ;;
            esac
            ;;
        --remove-binary)
            remove_binary=1
            shift
            ;;
        -h|--help)
            usage 0
            ;;
        *)
            usage
            ;;
    esac
done

case "$destination" in /*) ;; *) echo "carli uninstall: destination must be absolute" >&2; exit 2 ;; esac
case "$shells_file" in /*) ;; *) echo "carli uninstall: shells file must be absolute" >&2; exit 2 ;; esac
case "$passwd_file" in /*) ;; *) echo "carli uninstall: passwd file must be absolute" >&2; exit 2 ;; esac
case "$destination$shells_file$passwd_file" in *'
'*) echo "carli uninstall: paths must not contain newlines" >&2; exit 2 ;; esac
[ -f "$shells_file" ] || { echo "carli uninstall: shells file is not a regular file: $shells_file" >&2; exit 1; }
[ ! -L "$shells_file" ] || { echo "carli uninstall: refusing symlinked shells file: $shells_file" >&2; exit 1; }
[ -f "$passwd_file" ] || { echo "carli uninstall: passwd file is not a regular file: $passwd_file" >&2; exit 1; }

destination_directory=$(cd -P "$(dirname "$destination")" && pwd)
shells_directory=$(cd -P "$(dirname "$shells_file")" && pwd)
passwd_directory=$(cd -P "$(dirname "$passwd_file")" && pwd)
destination=$destination_directory/$(basename "$destination")
shells_file=$shells_directory/$(basename "$shells_file")
passwd_file=$passwd_directory/$(basename "$passwd_file")

if awk -F: -v shell="$destination" '$7 == shell { found = 1 } END { exit !found }' "$passwd_file"; then
    echo "carli uninstall: $destination is still assigned to at least one account; change those accounts first" >&2
    exit 1
fi

staged_shells=$(mktemp "$shells_directory/.shells-uninstall.XXXXXX")
cleanup() {
    status=$?
    rm -f -- "$staged_shells"
    trap - EXIT HUP INT TERM
    exit "$status"
}
trap cleanup EXIT
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM

chmod --reference="$shells_file" "$staged_shells"
chown --reference="$shells_file" "$staged_shells"
awk -v shell="$destination" '$0 != shell { print }' "$shells_file" > "$staged_shells"
mv -f -- "$staged_shells" "$shells_file"

if [ "$remove_binary" -eq 1 ]; then
    [ ! -L "$destination" ] || { echo "carli uninstall: refusing to remove symlinked destination: $destination" >&2; exit 1; }
    if [ -e "$destination" ] && [ ! -f "$destination" ]; then
        echo "carli uninstall: refusing to remove non-regular destination: $destination" >&2
        exit 1
    fi
    rm -f -- "$destination"
    echo "Unregistered and removed $destination"
else
    echo "Unregistered $destination; binary retained (use --remove-binary to remove it)"
fi
