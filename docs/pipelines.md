# Pipelines

`carli` connects commands with the ordinary Unix pipeline operator, `|`. The
standard output of each stage becomes the standard input of the next stage:

```sh
printf 'beta\nalpha\n' | sort
cat access.log | grep ' 500 ' | wc -l
```

Pipelines work in interactive sessions, `-c` command mode, batch input, and
startup files.

## Parsing and quoting

An unquoted, unescaped `|` separates pipeline stages. Whitespace around it is
optional:

```sh
printf hello|wc -c
```

Quote or escape a pipe to pass it as ordinary argument text:

```sh
printf '%s\n' '|'
printf '%s\n' \|
```

Every stage must contain a command. Leading, trailing, or consecutive pipeline
operators are syntax errors and set status 2:

```sh
| cat
cat |
cat || sort
```

`||` is not yet a conditional operator; it is parsed as two adjacent pipeline
operators and therefore rejected.

## Exit status

The pipeline's status is the status of its last stage. `$?` exposes that value:

```sh
sh -c 'exit 23' | true
printf '%s\n' "$?"  # prints 0

true | sh -c 'exit 7'
printf '%s\n' "$?"  # prints 7
```

`carli` does not currently implement a `pipefail` option. A failure in an
earlier stage therefore does not make the pipeline fail when the last stage
succeeds. If a program cannot be started, that stage receives status 126 or
127, its pipe endpoints are still closed correctly, and the other stages still
run.

## Redirection

Input and output redirections belong to the stage in which they appear. An
explicit redirection overrides that end of the pipe:

```sh
generate-data | sort > sorted.txt
printf ignored | cat < saved-input.txt
```

In the second example, `cat` reads `saved-input.txt`, not the output from
`printf`. The unused pipe is closed, so neither process waits indefinitely.

All redirection files are opened before any pipeline stage starts. If opening a
file fails, no stage is launched and the command returns status 1.

## Built-ins

A built-in used as a command by itself runs in the main shell and can change
shell state. A built-in in a multi-stage pipeline runs in an isolated child
process so every stage can execute concurrently:

```sh
pwd | wc -c
cd /tmp | cat
pwd
```

The `cd` in this example does not change the main shell's directory. The same
rule applies to `export` and `exit`: environment changes do not persist, and
`exit` terminates only its pipeline stage. Job-control built-ins have no access
to the parent shell's job table when used in a pipeline; use `jobs`, `fg`, and
`bg` as standalone commands.

## Signals, terminal ownership, and jobs

Every stage is placed in one process group. In an interactive session, `carli`
gives that group foreground terminal ownership and waits for all its processes.
Consequently:

- Ctrl-C interrupts the whole pipeline.
- Ctrl-Z stops the whole pipeline as one job.
- `jobs` shows one entry for the pipeline.
- `fg` and `bg` resume every remaining stage together.
- terminal modes are restored after the pipeline exits, is signaled, or stops.
- shell shutdown and `SIGHUP` cleanup signal the entire pipeline group.

The job's eventual status remains the last stage's status, even if that stage
finishes before another stage.

## Current limitations

- There is no `pipefail` option.
- `|&` for piping standard error is not implemented.
- Background launch with `&` is not implemented.
- Pipelines do not add Bash, Zsh, or POSIX script compatibility; command lists,
  conditionals, command substitution, functions, loops, and globbing remain
  unsupported.
