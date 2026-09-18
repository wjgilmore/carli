# carli Shell

`carli` is a small Unix shell written in Rust as a learning project. It is not yet suitable for use as a login shell.

## Features

### Completed

- Interactive prompt with customizable content
- Interactive line editing with command history navigation
- Persistent command history stored in `~/.carli_history`
- Command parsing with whitespace-separated arguments
- Single quotes, double quotes, and backslash escapes
- External program lookup through `PATH`
- External program execution and waiting for completion
- Built-in commands for shell state and command lookup
- Environment-variable export for child processes
- `$NAME` environment-variable expansion in unquoted and double-quoted text,
  using names that begin with a letter or underscore and continue with letters,
  digits, or underscores
- Braced environment-variable expansion with `${NAME}`
- Previous-command status expansion with `$?`
- Literal variable text inside single quotes or after a backslash
- Graceful handling of blank input, EOF, parse errors, and command errors

### Planned

- Input and output redirection
- Pipelines
- Human-friendly output formatting
- Signal handling
- Basic foreground and background job control
- Startup configuration files
- Login-shell behavior
- Safe installation, registration in `/etc/shells`, and use with `chsh`

carli is not yet suitable for use as a login shell. Signal handling, job control,
and login-shell behavior should be completed before registering it with `chsh`.

## Planned output formatting

> **This feature is a design proposal and has not been implemented yet.** The
> commands and syntax below do not currently work in carli and may change as the
> design develops.

One of carli's goals is to make structured output pleasant to explore without
breaking the Unix convention that programs exchange raw data. The guiding rule
will be: preserve exact output when it is redirected or passed to another
program, but allow rich presentation when output is intentionally displayed to
a person in an interactive terminal.

The first planned step is a `view` command for opening structured files:

```sh
view customers.csv
view results.json
view server.log
```

The viewer could provide aligned and scrollable tables, search, column
selection, terminal-width-aware layouts, and a way to inspect the original raw
content. Initial support would focus on CSV, with TSV and JSON following later.

After carli gains pipelines, `view` could also act as an explicit final stage:

```sh
generate-report | view --csv
curl example.com/data.json | view --json
```

A possible future presentation pipe, written `|>`, could make the distinction
between data transport and human-facing rendering especially clear:

```sh
generate-report |> table
git log |> timeline
cargo test |> test-report
```

The ordinary `|` pipe would continue to transfer unmodified data between
programs. The proposed `|>` operator would explicitly request formatting for
interactive display. Formatting would remain opt-in so that redirection,
scripts, and existing command-line tools continue to behave predictably.

## Available commands

carli currently provides these built-in commands:

- `cd [DIRECTORY]` changes carli's working directory. With no argument, it uses
  `HOME`.
- `pwd` prints carli's current working directory.
- `export NAME=VALUE` adds or updates an environment variable inherited by
  programs started from carli. `NAME` must begin with a letter or underscore
  and may then contain letters, digits, or underscores.
- `which COMMAND` reports whether a command is a carli built-in or prints the
  first matching file found through `PATH`.
- `exit [STATUS]` exits carli, optionally with a numeric status from 0 to 255.

Commands that are not built-ins are treated as external programs. For example,
`ls -al`, `cargo test`, and `printenv HOME` are located through `PATH` and run as
child processes.

## Line editing and history

carli stores command history in `~/.carli_history`. Press the Up and Down Arrow
keys to move through commands from the current or previous sessions, or use the
Left and Right Arrow keys to edit the current line before running it.

At the prompt, Ctrl-C cancels the current input and presents a fresh prompt.
Ctrl-D on an empty line exits carli. History is saved when carli exits through
either Ctrl-D or the `exit` built-in.

## Customizing the prompt

The `CARLI_PROMPT` environment variable controls carli's prompt. Set it from
inside carli with `export`:

```text
export CARLI_PROMPT="{user}:{dir}$ "
```

The prompt supports these placeholders:

- `{cwd}`: full path to the current working directory
- `{dir}`: name of the current directory
- `{user}`: value of the `USER` environment variable
- `{shell}`: the name `carli`

For example:

```text
export CARLI_PROMPT="{shell}:{cwd}> "
```

carli rebuilds the prompt before reading each command, so `{cwd}` and `{dir}`
change immediately after `cd`. Unknown placeholders remain unchanged. When
`CARLI_PROMPT` is not set, carli uses its default prompt.

Prompt customization does not yet persist after carli exits because startup
configuration files are still planned. The prompt can also be configured for a
single session when starting carli from another shell:

```sh
CARLI_PROMPT='{user}:{dir}$ ' cargo run
```

## Try it

```sh
cargo run
```

Then try:

```text
carli $ pwd
/home/wjgilmore/carli
carli $ which cargo
/home/wjgilmore/.cargo/bin/cargo
carli $ export GREETING=hello
carli $ printenv GREETING
hello
carli $ echo "$GREETING"
hello
carli $ echo '$GREETING'
$GREETING
carli $ echo "hello from carli"
hello from carli
carli $ cd /tmp
carli $ pwd
/tmp
$ carli exit
-> carli git:(master) 
```

## Testing

Run the complete test suite from the project directory:

```sh
cargo test
```

Cargo will compile carli, run its unit tests, and report whether each test
passed or failed.
