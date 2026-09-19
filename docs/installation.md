# Safe login-shell installation

`carli` provides guarded scripts for copying a tested release binary to a stable
absolute path, registering that exact path in `/etc/shells`, and reversing the
installation. Follow the recovery procedure below; changing an account shell
always carries a risk of locking yourself out if the binary or its dependencies
become unavailable.

## Compatibility boundary

Installation safety does not imply Bash, Zsh, or POSIX shell compatibility.
`carli` currently lacks pipelines, command separators, conditionals, command
substitution, functions, loops, globbing, and many standard built-ins. Do not
use it as the login shell for an account whose SSH commands, file-transfer
tools, automation, or recovery procedures require that syntax.

Use a system account only after testing the actual programs and login paths it
needs. Keep another administrator or root recovery method available.

## Build and verify

From a clean checkout, run the complete test and lint suite, then build a
release binary:

```sh
cargo test
cargo clippy --all-targets -- -D warnings
cargo build --release
./target/release/carli -c 'exit 0'
```

Do not register a binary inside `target/`; `cargo clean` could remove it and
make future logins fail. The installer copies it to `/usr/local/bin/carli` by
default.

## Preserve a recovery path

Before changing the shell:

1. Record the current shell. On Linux use `getent passwd "$USER"`; on macOS use
   `dscl . -read "/Users/$USER" UserShell`.
2. Open and keep authenticated a second terminal running the current shell.
3. Confirm that you can obtain root access from that recovery terminal.
4. Do not close it until a completely separate carli login succeeds.

## Install and register

Run the installer from the repository root:

```sh
sudo ./scripts/install.sh --binary "$(pwd)/target/release/carli"
```

The installer requires absolute destination and registry paths, rejects a
symlinked `/etc/shells`, stages the executable and registry update in their
destination directories, smoke-tests the staged binary, removes duplicate
registry entries, and uses atomic renames. If registry commit fails after the
binary changes, it restores the previous binary.

Verify the stable copy and registration:

```sh
/usr/local/bin/carli -c 'exit 0'
grep -Fx /usr/local/bin/carli /etc/shells
```

Then change the account shell:

```sh
chsh -s /usr/local/bin/carli
```

Open a separate login session and test the prompt, `cd`, external commands,
Ctrl-C, Ctrl-Z with `fg`, and exit. Keep the recovery terminal open throughout.

## Roll back

From the recovery terminal, change the account back to the previously recorded
shell before unregistering carli:

```sh
chsh -s /bin/bash
```

Use the actual previous path, which may instead be `/bin/zsh` or another shell.
After confirming `getent passwd "$USER"` on Linux or the corresponding `dscl`
command on macOS shows the restored shell, unregister carli while retaining the
executable:

```sh
sudo ./scripts/uninstall.sh
```

Once no account uses carli and a fresh login with the restored shell succeeds,
remove the binary too:

```sh
sudo ./scripts/uninstall.sh --remove-binary
```

The uninstall script checks `/etc/passwd` on Linux and macOS Directory Services
through `dscl`, and refuses to unregister or remove carli while any account
still names that exact path. A failed macOS account query also aborts without
changes. By default the script only removes the `/etc/shells` entry, leaving the
executable available as an additional recovery safeguard. With
`--remove-binary`, it validates that the destination is a regular non-symlink
before changing `/etc/shells`, so an unsafe removal target leaves both the
registry and filesystem untouched. Repeated install and uninstall operations
are idempotent.

## Custom paths

Both scripts accept `--destination` and `--shells-file`. The uninstall script
also accepts `--passwd-file`, primarily for isolated testing. Production use
should retain the defaults unless the operating system uses different canonical
files.

Run `./scripts/install.sh --help` or `./scripts/uninstall.sh --help` for the
accepted options.

For macOS prerequisites, platform-specific verification, and recovery commands,
see [macOS support](macos.md).
