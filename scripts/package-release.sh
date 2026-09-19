#!/bin/sh
set -eu

repository_root=$(CDPATH= cd "$(dirname "$0")/.." && pwd)
output_directory=$repository_root/dist
target=
binary=

usage() {
    cat >&2 <<EOF
usage: $0 [--target RUST_TARGET] [--binary PATH] [--output DIRECTORY]

Build a release binary (unless --binary is supplied), create a versioned
tar.gz archive, and regenerate SHA256SUMS for all release archives.
EOF
    exit "${1:-2}"
}

while [ "$#" -gt 0 ]; do
    case "$1" in
        --target|--binary|--output)
            [ "$#" -ge 2 ] || usage
            option=$1
            value=$2
            shift 2
            case "$option" in
                --target) target=$value ;;
                --binary) binary=$value ;;
                --output) output_directory=$value ;;
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

case "$target$output_directory$binary" in
    *'
'*) echo "carli package: paths and target must not contain newlines" >&2; exit 2 ;;
esac

version=$(sed -n 's/^version = "\([^"]*\)"$/\1/p' "$repository_root/Cargo.toml" | head -n 1)
[ -n "$version" ] || { echo "carli package: could not read package version" >&2; exit 1; }

if [ -z "$target" ]; then
    target=$(rustc -vV | sed -n 's/^host: //p')
fi
[ -n "$target" ] || { echo "carli package: could not determine Rust target" >&2; exit 1; }

if [ -z "$binary" ]; then
    cargo build --manifest-path "$repository_root/Cargo.toml" --locked --release --target "$target"
    binary=$repository_root/target/$target/release/carli
fi
[ -f "$binary" ] || { echo "carli package: binary not found: $binary" >&2; exit 1; }
[ -x "$binary" ] || { echo "carli package: binary is not executable: $binary" >&2; exit 1; }

mkdir -p "$output_directory"
output_directory=$(CDPATH= cd "$output_directory" && pwd)
bundle=carli-v$version-$target
staging_directory=$(mktemp -d "${TMPDIR:-/tmp}/carli-package.XXXXXX")

cleanup() {
    status=$?
    rm -rf "$staging_directory"
    trap - EXIT HUP INT TERM
    exit "$status"
}
trap cleanup EXIT
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM

mkdir -p "$staging_directory/$bundle/scripts"
install -m 0755 "$binary" "$staging_directory/$bundle/carli"
install -m 0755 "$repository_root/scripts/install.sh" "$staging_directory/$bundle/scripts/install.sh"
install -m 0755 "$repository_root/scripts/uninstall.sh" "$staging_directory/$bundle/scripts/uninstall.sh"
cp "$repository_root/README.md" "$staging_directory/$bundle/README.md"
cp "$repository_root/LICENSE" "$staging_directory/$bundle/LICENSE"
cp "$repository_root/CHANGELOG.md" "$staging_directory/$bundle/CHANGELOG.md"

archive=$output_directory/$bundle.tar.gz
COPYFILE_DISABLE=1 tar -czf "$archive" -C "$staging_directory" "$bundle"

(
    cd "$output_directory"
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum ./*.tar.gz | sed 's#  \./#  #'
    elif command -v shasum >/dev/null 2>&1; then
        shasum -a 256 ./*.tar.gz | sed 's#  \./#  #'
    else
        echo "carli package: sha256sum or shasum is required" >&2
        exit 1
    fi
) > "$output_directory/SHA256SUMS"

printf 'Created %s\n' "$archive"
printf 'Updated %s\n' "$output_directory/SHA256SUMS"
