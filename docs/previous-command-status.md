# Previous-command status expansion

carli supports `$?`, which expands to the exit status of the most recently
completed command. This makes it possible to check whether a command succeeded
and to distinguish common failure modes.

```sh
ls README.md
echo $?
```

A status of `0` indicates success. A nonzero value indicates failure or another
command-specific result. For example, `grep` uses status `1` when it does not
find a match, even though the program itself ran correctly.

```sh
grep -q missing-word README.md
echo $?
```

## Expansion rules

`$?` expands in unquoted text and inside double quotes:

```sh
echo status=$?
echo "the previous status was $?"
```

Single quotes and backslash escaping preserve it literally:

```sh
echo '$?'
echo \$?
```

An empty input line does not replace the saved status. A parsing error sets the
status to `2`, and cancelling input with <kbd>Ctrl-C</kbd> sets it to `130`.

## Statuses produced by carli

carli records statuses from external programs and its own built-in commands:

- `0` indicates success.
- `1` indicates a general built-in failure, such as an invalid directory.
- `2` indicates a carli parsing error or an invalid numeric argument to `exit`.
- `126` indicates that an external command was found but could not be started.
- `127` indicates that an external command was not found.
- `128 + signal_number` indicates termination by a signal. For example,
  termination by `SIGINT` produces status `130` on Unix systems.

These conventions provide the foundation for future conditional operators such
as `&&` and `||`.
