# carli

`carli` is a small Unix shell written in Rust as a learning project. It is not
yet suitable for use as a login shell.

## Features

### Completed

- Interactive `carli$` prompt
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

## Try it

```sh
cargo run
```

Then try:

```text
pwd
which cargo
export GREETING=hello
printenv GREETING
echo "$GREETING"
echo '$GREETING'
echo "hello from carli"
cd /tmp
pwd
exit
```

Run parser tests with `cargo test`.
