# Releasing carli

This document is the authoritative release checklist. Releases use Semantic
Versioning, annotated Git tags named `vVERSION`, versioned release notes, a
crates.io package named `carli-shell`, and checksummed binary archives. The
installed executable remains `carli`.

Publishing a crate, pushing a tag, and creating a GitHub release are public and
effectively irreversible. Complete all validation before performing those
steps. Never put a crates.io token in the repository, a command argument, shell
history, release notes, or chat.

## 1. Prepare the release

1. Start from a clean `master` branch synchronized with `origin/master`.
2. Set the same version in `Cargo.toml`, `Cargo.lock`, `CHANGELOG.md`, and the
   release-notes filename.
3. Move completed entries from `Unreleased` into a dated version section.
4. Write `release-notes/vVERSION.md` for users, including installation and
   compatibility warnings.
5. Run the complete validation suite:

```sh
cargo fmt --check
cargo test
cargo clippy --all-targets -- -D warnings
sh -n scripts/install.sh scripts/uninstall.sh scripts/package-release.sh
cargo package --locked
git diff --check
```

Inspect the exact crate contents before publishing:

```sh
cargo package --locked --list
```

Install the packaged crate into a temporary root and smoke-test the resulting
executable:

```sh
temporary_root=$(mktemp -d)
cargo install --locked --path . --root "$temporary_root"
"$temporary_root/bin/carli" -c 'exit 0'
```

Remove the temporary directory after verification.

## 2. Commit the release

Commit the release preparation and push `master`. Confirm that the pushed
commit is exactly the commit intended for release. The working tree must be
clean before tagging or packaging.

## 3. Authenticate to crates.io

Use `cargo login` interactively, or expose the token only to the publishing
process through `CARGO_REGISTRY_TOKEN`. Prefer a short-lived, scoped token where
available. Do not include the token directly in a recorded command.

Run the final publish preflight, then publish:

```sh
cargo publish --locked --dry-run
cargo publish --locked
```

Wait for `carli-shell VERSION` to appear on crates.io and verify installation:

```sh
cargo install carli-shell --version VERSION
carli -c 'exit 0'
```

## 4. Tag the published commit

Create an annotated tag only after crates.io accepts the package:

```sh
git tag -a vVERSION -m "carli VERSION"
git push origin vVERSION
```

Use a signed tag instead when a configured signing identity is available.
Never move or reuse a published release tag.

## 5. Build binary archives

Build each archive on the operating system and architecture it targets. The
script builds with the lockfile, places the executable and installation files
under one versioned directory, and regenerates `dist/SHA256SUMS`:

```sh
./scripts/package-release.sh
```

The default target is the current Rust host triple. An explicit Rust target can
be supplied when the required linker and system libraries are available:

```sh
./scripts/package-release.sh --target aarch64-apple-darwin
```

Do not relabel an archive built for a different host. Smoke-test the extracted
binary on the target system and independently verify the checksum before
uploading it.

## 6. Create the GitHub release

Create the release from the immutable tag and attach every archive plus the
single checksum manifest:

```sh
gh release create vVERSION \
  --verify-tag \
  --title "carli VERSION" \
  --notes-file release-notes/vVERSION.md \
  dist/*.tar.gz dist/SHA256SUMS
```

Verify the rendered release notes, download links, checksums, crates.io page,
and clean installation instructions. Announce the release only after those
checks succeed.
