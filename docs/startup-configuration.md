# Startup configuration

carli can run system-wide and per-user configuration files before starting an
interactive session or executing a login-shell command. Startup files use the
same command parser, built-ins, expansion, redirection, and command lookup as
ordinary carli input.

## File locations and order

carli checks startup files in this order:

1. `/etc/carli/config`
2. `$XDG_CONFIG_HOME/carli/config`, when `XDG_CONFIG_HOME` is set and nonempty
3. `$HOME/.config/carli/config`, when `XDG_CONFIG_HOME` is unavailable

Only one user path is selected. The user configuration runs after the system
configuration and can therefore replace environment variables or other state
established system-wide.

`CARLI_SYSTEM_CONFIG` may override `/etc/carli/config` with another path. This
supports nonstandard installation prefixes and allows the system layer to be
tested without modifying `/etc`. An unset or empty value uses the default path.

Missing files are normal and are skipped without an error. An unreadable file
produces a diagnostic, but carli continues to the next startup file so a broken
optional configuration does not make the shell completely inaccessible.

## When files are loaded

Startup files are loaded for:

- interactive terminal sessions; and
- invocations whose executable name begins with `-`, the traditional marker
  for a login shell.

Ordinary `carli -c 'COMMAND'` and non-terminal batch input do not load startup
files. This keeps automation predictable and prevents personal interactive
customization from changing remote commands or scripts unexpectedly. A `-c`
invocation marked as a login shell does load them.

Interactive history is initialized after startup processing, so commands in a
configuration file are not added to command history.

## Syntax

Each line contains one carli command. Blank lines and comments are allowed:

```sh
# ~/.config/carli/config
export CARLI_PROMPT="{user}:{dir}$ "
export EDITOR=vim
export PROJECTS="$HOME/projects"
```

An unquoted `#` that begins a word starts a comment. A `#` inside a word, inside
quotes, or following a backslash remains literal:

```sh
export COLOR="#88c0d0"
```

State-changing built-ins such as `cd` and `export` affect the session that
follows. External commands, redirection, and pipelines are also supported.
Conditionals, command separators, functions, and loops remain unavailable.

## PATH behavior

carli preserves an inherited `PATH`, including an explicitly empty value. If
`PATH` is completely unset for an interactive or login-shell invocation, carli
sets this conservative default before reading startup files:

```text
/usr/local/bin:/usr/bin:/bin
```

The system or user configuration may then replace it:

```sh
export PATH="$HOME/.local/bin:/usr/local/bin:/usr/bin:/bin"
```

This provides basic command lookup in a minimal login environment without
overwriting a PATH deliberately supplied by a terminal, display manager, SSH
server, or parent shell.

## Errors and statuses

Parse errors include the configuration path and line number:

```text
carli: /home/alice/.config/carli/config:4: unclosed double quote
```

After reporting a malformed line, carli continues with the next line. The
status is set to `2`, so a following configuration command can inspect it with
`$?`. File-reading errors set status `1`.

The final startup status becomes the initial value of `$?` for the session. An
`exit STATUS` line stops startup processing and exits carli with that status.

## Example

Create the configuration directory and file using ordinary filesystem tools:

```sh
mkdir -p ~/.config/carli
$EDITOR ~/.config/carli/config
```

Example contents:

```sh
# Personal interactive defaults
export PATH="$HOME/.local/bin:/usr/local/bin:/usr/bin:/bin"
export EDITOR=vim
export CARLI_PROMPT="{user}:{dir}$ "
```

Restart carli to load the changes. Configuration is read once during startup;
editing the file does not alter an already-running session.
