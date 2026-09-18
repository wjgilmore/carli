# Invocation modes

carli supports interactive terminals, one-command execution with `-c`, and
line-oriented batch input. Keeping these modes separate prevents prompts and
history operations from interfering with automation, SSH commands, and other
programs that invoke a user's configured shell.

## Interactive mode

Running carli with no arguments while standard input is a terminal starts the
interactive line editor:

```sh
carli
```

Interactive mode displays the configured prompt, loads and saves command
history, and enables foreground process groups and job control.

## One command with `-c`

The `-c` option executes the following argument as one carli command and then
exits:

```sh
carli -c 'pwd'
carli -c 'echo "hello world" > greeting.txt'
```

The command uses the same parser, variable expansion, built-ins, redirection,
and external-program lookup as interactive input. Carli does not display a
prompt or read or write its history file in this mode.

The process exit status is the command's status:

```sh
carli -c 'sh -c "exit 7"'
echo $?
# 7
```

A command that cannot be found returns `127`, a command that cannot be started
returns `126`, and a carli parse error returns `2`. An explicit `exit STATUS`
uses the requested status.

Exactly one argument must follow `-c`. Invalid command-line usage returns
status `2` and prints the accepted form:

```text
usage: carli [-c COMMAND]
```

## Batch input

When carli has no command-line arguments and standard input is not a terminal,
it reads one command per line until EOF:

```sh
printf '%s\n' 'export NAME=carli' 'echo $NAME' | carli
```

State changes such as `cd` and `export` persist between lines because all lines
run in the same carli process. Blank lines preserve the previous status. An
`exit` command stops processing immediately; otherwise, carli exits with the
status of the last line it executed.

Batch mode does not display prompts, initialize the interactive line editor,
load history, save history, or perform terminal job-control operations.

## Login-shell detection

Unix login programs commonly mark a login shell by placing a leading `-` on
the executable name in `argv[0]`. Carli detects this form and loads its system
and user startup files. Interactive login sessions also use persistent history,
terminal process groups, terminal-mode restoration, and graceful `SIGHUP`
cleanup. See [Safe login-shell installation](installation.md).

## SSH and system integration

SSH servers commonly run a remote command through the user's configured shell
using an invocation equivalent to:

```sh
carli -c 'requested command'
```

Supporting this form prevents carli from opening an interactive prompt when a
remote command expects ordinary stdout, stderr, and an exit status. However,
remote commands that depend on POSIX shell syntax such as pipelines, command
separators, substitutions, or conditionals remain incompatible. Do not select
carli for an account that relies on those SSH command forms.

## Current syntax limitations

`-c` receives one carli command, not a complete shell script. Command
separators, pipelines, conditionals, functions, and loops are not implemented.
For example, `carli -c 'first; second'` does not currently execute two commands.
Use batch input with one command per line when multiple sequential commands are
needed.
