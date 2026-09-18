# macOS support

Carli supports macOS as an interactive shell. Its test suite covers platform
differences in process groups, terminal handling, system utilities,
installation, and uninstallation.

## Prerequisites

Install the Xcode Command Line Tools and Rust, then confirm that `python3` is
available. Python is used only by the pseudo-terminal integration tests, not by
the carli executable.

```sh
xcode-select --install
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
python3 --version
```

Review third-party installation commands before running them. If Rust is
already installed, update the stable toolchain with `rustup update stable`.

## Build and test

From the repository root:

```sh
cargo test
cargo clippy --all-targets -- -D warnings
cargo build --release
./target/release/carli -c 'exit 0'
```

The handful of tests that depend specifically on Linux's `/dev/full` are
disabled on macOS. Equivalent redirection, built-in failure, and terminal
recovery behavior is covered by platform-independent tests.

## Install as a login shell

Read [Safe login-shell installation](installation.md) before changing an
account shell. Keep a separate authenticated terminal open with the old shell
until a new carli login succeeds.

The installer works with the BSD utilities shipped by macOS as well as their
GNU counterparts on Linux. It copies the release binary to
`/usr/local/bin/carli` and registers that path in `/etc/shells`:

```sh
sudo ./scripts/install.sh --binary "$(pwd)/target/release/carli"
/usr/local/bin/carli -c 'exit 0'
grep -Fx /usr/local/bin/carli /etc/shells
chsh -s /usr/local/bin/carli
```

On Apple silicon and Intel Macs alike, `/usr/local/bin/carli` is an explicit
default chosen by this project; it does not depend on a Homebrew prefix.

Record the current shell before changing it:

```sh
dscl . -read "/Users/$USER" UserShell
```

To roll back, run `chsh -s` with that previous absolute path. Confirm the
result with the same `dscl` command before uninstalling carli.

## macOS uninstall protection

macOS account records are managed by Directory Services and are not reliably
enumerated by `/etc/passwd`. During a normal macOS uninstall,
`scripts/uninstall.sh` therefore queries `dscl` and refuses to unregister or
remove carli while any local account still uses its exact path. If that query
fails, uninstallation fails closed and changes nothing.

```sh
sudo ./scripts/uninstall.sh
sudo ./scripts/uninstall.sh --remove-binary
```

The first command unregisters carli but retains the binary as a recovery
safeguard. Use the second only after every account has switched away and a new
login with the restored shell succeeds.

## Compatibility scope

macOS support means carli builds, its automated behavior suite passes, and its
guarded installer understands macOS. It does not make carli compatible with
Bash, Zsh, or POSIX shell scripts. Pipelines, command separators, conditionals,
command substitution, functions, loops, and globbing remain unsupported.
Programs and remote-login workflows that assume those features can still fail;
test the actual workflow before making carli the login shell for an important
account.
