#!/bin/sh
set -eu

destination=/usr/local/bin/carli
shells_file=/etc/shells
passwd_file=/etc/passwd
passwd_file_explicit=0
remove_binary=0

usage() {
    echo "usage: $0 [--destination ABSOLUTE_PATH] [--shells-file ABSOLUTE_PATH] [--passwd-file ABSOLUTE_PATH] [--remove-binary]" >&2
    exit "${1:-2}"
}

file_mode() {
    if mode=$(stat -f '%Lp' "$1" 2>/dev/null); then
        printf '%s\n' "$mode"
    else
        stat -c '%a' "$1"
    fi
}

file_owner() {
    if owner=$(stat -f '%u:%g' "$1" 2>/dev/null); then
        printf '%s\n' "$owner"
    else
        stat -c '%u:%g' "$1"
    fi
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
                --passwd-file) passwd_file=$value; passwd_file_explicit=1 ;;
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

if [ "$remove_binary" -eq 1 ]; then
    [ ! -L "$destination" ] || { echo "carli uninstall: refusing to remove symlinked destination: $destination" >&2; exit 1; }
    if [ -e "$destination" ] && [ ! -f "$destination" ]; then
        echo "carli uninstall: refusing to remove non-regular destination: $destination" >&2
        exit 1
    fi
fi

account_uses_shell() {
    if [ "$(uname -s)" = Darwin ] && [ "$passwd_file_explicit" -eq 0 ]; then
        directory_users=$(dscl . -list /Users UserShell) || return 2
        printf '%s\n' "$directory_users" | awk -v shell="$destination" '$NF == shell { found = 1 } END { exit !found }'
    else
        awk -F: -v shell="$destination" '$7 == shell { found = 1 } END { exit !found }' "$passwd_file"
    fi
}

if account_uses_shell; then
    echo "carli uninstall: $destination is still assigned to at least one account; change those accounts first" >&2
    exit 1
else
    account_check_status=$?
    if [ "$account_check_status" -ne 1 ]; then
        echo "carli uninstall: could not determine whether $destination is assigned to a macOS account" >&2
        exit 1
    fi
fi

staged_shells=$(mktemp "$shells_directory/.shells-uninstall.XXXXXX")
cleanup() {
    status=$?
    rm -f "$staged_shells"
    trap - EXIT HUP INT TERM
    exit "$status"
}
trap cleanup EXIT
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM

chmod "$(file_mode "$shells_file")" "$staged_shells"
chown "$(file_owner "$shells_file")" "$staged_shells"
awk -v shell="$destination" '$0 != shell { print }' "$shells_file" > "$staged_shells"
mv -f "$staged_shells" "$shells_file"

if [ "$remove_binary" -eq 1 ]; then
    rm -f "$destination"
    echo "Unregistered and removed $destination"
else
    echo "Unregistered $destination; binary retained (use --remove-binary to remove it)"
fi
