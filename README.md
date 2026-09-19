# carli Shell

`carli` is a small Unix shell written in Rust. It is a learning project focused
on predictable interactive behavior, job control, and safe recovery when
something goes wrong.

carli runs on Linux and macOS. It is not a replacement for Bash or Zsh yet, and
it cannot run general-purpose shell scripts. You can safely try it without
changing your login shell.

## Quick start

You need Rust 1.88 or newer. Clone the repository, enter its directory, and run:

```sh
cargo run
```

When the `carli $` prompt appears, try a few commands:

```text
carli $ pwd
carli $ echo "hello from carli"
carli $ printf 'beta\nalpha\n' | sort
carli $ export GREETING=hello
carli $ echo "$GREETING"
carli $ exit
```

These commands run a development build directly from the repository. They do
not install anything or change your account's login shell.

To run one command without opening an interactive prompt:

```sh
cargo run -- -c 'echo "hello from carli"'
```

## Installation

Install the latest published version with Cargo:

```sh
cargo install carli-shell
```

The package is called `carli-shell`, but the command it installs is `carli`.
Cargo places the executable in a user-managed directory, normally
`~/.cargo/bin`.

This installation is ideal for trying carli. Do not register the Cargo-managed
executable as your login shell because Cargo may replace or remove it. To put
carli at a stable system path and cautiously test it as a login shell, follow
the [safe login-shell installation guide](docs/installation.md).

### Installing from a release archive

Releases may also include a platform-specific archive and a `SHA256SUMS` file.
Download both files into the same directory. Before extracting the archive,
verify that it has not been corrupted or replaced.

On Linux:

```sh
archive=carli-vVERSION-TARGET.tar.gz
awk -v file="$archive" '$2 == file' SHA256SUMS | sha256sum --check -
```

On macOS:

```sh
archive=carli-vVERSION-TARGET.tar.gz
awk -v file="$archive" '$2 == file' SHA256SUMS | shasum -a 256 -c -
```

Replace `VERSION` and `TARGET` with the values in the downloaded filename. A
successful check prints the filename followed by `OK`. Do not extract or run
the archive if the check fails or prints nothing.

## What carli can do

carli currently supports:

- interactive line editing and persistent command history;
- single and double quotes, backslash escapes, and comments;
- external programs found through `PATH`;
- environment variables, including `$NAME` and `${NAME}` expansion;
- the previous command's status through `$?`;
- input and output redirection with `<`, `>`, and `>>`;
- multi-stage pipelines with `|`;
- foreground process groups and Ctrl-C/Ctrl-Z signal handling;
- basic job control with `jobs`, `fg`, and `bg`;
- startup configuration and a customizable prompt;
- command mode with `-c` and line-oriented batch input; and
- guarded installation and uninstallation on Linux and macOS.

The detailed guides explain [pipelines](docs/pipelines.md), [redirection](docs/redirection.md),
[job control](docs/job-control.md), [invocation modes](docs/invocation-modes.md),
and [startup configuration](docs/startup-configuration.md).

## Important limitations

carli is not compatible with Bash, Zsh, or POSIX shell scripts. It does not yet
support command separators, conditionals, command substitution, functions,
loops, globbing, or background launch with `&`. Pipelines do not yet support
`pipefail` or `|&`.

These missing features may break remote SSH commands, scripts, and graphical
login sessions that expect a POSIX-compatible account shell. Before adding
carli to `/etc/shells` or using `chsh`, read the
[safe login-shell installation guide](docs/installation.md). Keep a separate,
authenticated recovery terminal open while testing a new login.

## Built-in commands

carli provides these commands itself:

- `cd [DIRECTORY]` changes the current directory. With no argument, it uses
  `HOME`.
- `pwd` prints the current directory.
- `export NAME=VALUE` sets an environment variable for carli and programs it
  starts.
- `which COMMAND` identifies a built-in or prints the first matching program
  found through `PATH`.
- `jobs` lists jobs managed by carli.
- `fg [JOB]` resumes a job in the foreground.
- `bg [JOB]` resumes a stopped job in the background.
- `exit [STATUS]` exits carli with an optional status from 0 to 255.

Write a job number as either `%1` or `1`. With no job number, `fg` and `bg`
select the newest job.

All other commands are external programs. For example, carli finds and runs
`ls`, `git`, and `cargo` through `PATH`.

## Pipelines and redirection

Connect programs with `|`:

```sh
printf 'beta\nalpha\n' | sort | tr a-z A-Z
```

The stages run together as one job. `$?` contains the status of the final
stage. A built-in inside a pipeline runs in an isolated child process, so a
command such as `cd /tmp | cat` does not change the main shell's directory.

Redirect input or output with `<`, `>`, and `>>`:

```sh
sort < unsorted.txt > sorted.txt
echo "another line" >> notes.txt
pwd > current-directory.txt
```

See the [pipeline guide](docs/pipelines.md) and
[redirection guide](docs/redirection.md) for status rules, precedence, and
limitations.

## Line editing and history

carli saves history in `~/.carli_history`.

- Press Up or Down to browse earlier commands.
- Press Left or Right to edit the current command.
- Press Ctrl-C to cancel the current input and show a fresh prompt.
- Press Ctrl-D on an empty line to exit.

History is saved when carli exits through Ctrl-D or the `exit` built-in.

## Jobs, signals, and terminal recovery

Each interactive command or pipeline gets its own foreground process group.
Ctrl-C and Ctrl-Z therefore affect the running job instead of carli. carli
reclaims the terminal and restores its saved terminal settings before showing
the next prompt.

If a job stops, carli remembers its terminal settings and restores them when
`fg` resumes it. These behaviors are explained in the guides to
[foreground process groups](docs/foreground-process-groups.md),
[signal handling](docs/signal-handling.md), and
[terminal-state restoration](docs/terminal-state-restoration.md).

## Customizing the prompt

The `CARLI_PROMPT` environment variable controls the prompt. Set it inside
carli with `export`:

```text
carli $ export CARLI_PROMPT="{user}:{dir}$ "
```

The prompt supports four placeholders:

- `{cwd}` is the full current directory.
- `{dir}` is the current directory's name.
- `{user}` is the value of the `USER` environment variable.
- `{shell}` is `carli`.

For example:

```text
carli $ export CARLI_PROMPT="{shell}:{cwd}> "
```

carli rebuilds the prompt after each command, so directory placeholders update
immediately after `cd`. Put the `export` command in carli's user startup file to
make the change permanent. See [startup configuration](docs/startup-configuration.md)
for the file location and loading rules.

## Planned output formatting

> **This feature is only a proposal. The commands and syntax in this section do
> not work yet and may change.**

One of carli's goals is to make structured output pleasant to explore while
preserving the Unix convention that programs exchange raw data. A future
`view` command could display CSV, TSV, JSON, or log data in a searchable,
terminal-width-aware layout:

```sh
view customers.csv
generate-report | view --csv
```

A future presentation pipe, written `|>`, could explicitly request
human-friendly formatting:

```sh
generate-report |> table
git log |> timeline
```

The existing `|` operator would continue to transfer unmodified data. Rich
formatting would remain opt-in so redirection, scripts, and command-line tools
continue to behave predictably.

## Contributing to carli Shell

Contributions are welcome. The steps below let you make and test changes
without installing carli or making it your login shell.

### What you need

- Rust 1.88 or newer, including `cargo` and `rustfmt`;
- `python3`, which runs the pseudo-terminal tests; and
- a Linux or macOS terminal.

Check the installed tools:

```sh
rustc --version
cargo --version
python3 --version
```

### Build and run a development copy

From the repository root, build carli:

```sh
cargo build
```

The development executable is `target/debug/carli`. Run it interactively:

```sh
./target/debug/carli
```

Or test one command at a time:

```sh
./target/debug/carli -c 'printf hello | cat'
```

`cargo run` combines the build and run steps. Arguments after `--` go to carli
rather than Cargo:

```sh
cargo run -- -c 'echo "$HOME"'
```

### Run the tests

Run every committed test with:

```sh
cargo test
```

This single command includes unit tests, process-level integration tests, and
interactive pseudo-terminal tests. The full suite requires `python3`.

To work on one area, filter tests by part of their name:

```sh
cargo test pipeline
cargo test redirection
cargo test job_control
```

Show the complete test inventory with:

```sh
cargo test -- --list
```

If a test fails and its output is hidden, rerun it with output enabled:

```sh
cargo test pipeline -- --nocapture
```

The [testing guide](docs/testing.md) maps every advertised feature to its
automated coverage.

### Debug a problem

First, reduce the problem to a single command when possible:

```sh
cargo run -- -c 'COMMAND TO INVESTIGATE'
echo $?
```

The second line runs in your current shell and prints carli's exit status. A
status of `0` means success; any other value reports a failure.

Enable Rust panic backtraces while running carli or a test:

```sh
RUST_BACKTRACE=1 cargo run
RUST_BACKTRACE=1 cargo test TEST_NAME -- --nocapture
```

For interactive signal, terminal, or job-control problems, reproduce the issue
with `target/debug/carli` in a normal terminal. Include the exact commands,
keys such as Ctrl-C or Ctrl-Z, operating system, and observed output in the bug
report. Do not test an unfinished change by making it your login shell.

If you need a native debugger, build first and then use the Rust wrapper for
the debugger installed on your system:

```sh
rust-gdb target/debug/carli
# or
rust-lldb target/debug/carli
```

### Check a change before submitting it

Run the same local checks used for a release:

```sh
cargo fmt --check
cargo test
cargo clippy --all-targets -- -D warnings
sh -n scripts/install.sh scripts/uninstall.sh scripts/package-release.sh
cargo package --locked --allow-dirty
git diff --check
```

The source is split into two main files: `src/lib.rs` contains command parsing,
and `src/main.rs` contains the shell loop, built-ins, process execution, signal
handling, and job control. Integration tests live in `tests/`, while detailed
design and safety notes live in `docs/`.
