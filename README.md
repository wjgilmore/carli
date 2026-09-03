# carli Shell

`carli` is a small Unix shell written in Rust as a learning project. It is not yet suitable for use as a login shell.

## Features

### Completed

- Interactive prompt with customizable content
- Interactive line editing with command history navigation
- Command parsing with whitespace-separated arguments
- Single quotes, double quotes, and backslash escapes
- External program lookup through `PATH`
- External program execution and waiting for completion
- Built-in commands for shell state and command lookup
- Environment-variable export for child processes
- `$NAME` environment-variable expansion in unquoted and double-quoted text,
  using names that begin with a letter or underscore and continue with letters,
  digits, or underscores
- Literal variable text inside single quotes or after a backslash
- Graceful handling of blank input, EOF, parse errors, and command errors

### Planned

- Braced variable expansion with `${NAME}`
- Previous-command status expansion with `$?`
- Persistent command history across carli sessions
- Input and output redirection
- Pipelines
- Signal handling
- Basic foreground and background job control
- Startup configuration files
- Login-shell behavior
- Safe installation, registration in `/etc/shells`, and use with `chsh`

carli is not yet suitable for use as a login shell. Signal handling, job control,
and login-shell behavior should be completed before registering it with `chsh`.

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

carli keeps commands entered during the current session in memory. Press the Up
and Down Arrow keys to move through that history, or use the Left and Right
Arrow keys to edit the current line before running it.

At the prompt, Ctrl-C cancels the current input and presents a fresh prompt.
Ctrl-D on an empty line exits carli.

History is not yet saved between carli sessions. Persistent history and a
history file are planned features.

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

Run parser tests with `cargo test`.
