# Input and output redirection

carli can connect a command's standard input or standard output to a file. This
allows commands to read saved data and lets their results be saved without
changing the commands themselves.

## Input redirection

The `<` operator uses a file as the command's standard input:

```sh
sort < unsorted.txt
```

This is equivalent to starting `sort` and supplying the contents of
`unsorted.txt` through its standard input. If the file does not exist or cannot
be opened, carli reports the error, does not run the command, and records status
`1` for `$?`.

## Output redirection

The `>` operator creates an output file or replaces its existing contents:

```sh
echo "first line" > notes.txt
```

The `>>` operator creates an output file or appends to its existing contents:

```sh
echo "another line" >> notes.txt
```

Output redirection works for external programs and for built-ins that write to
standard output, including `pwd` and `which`:

```sh
pwd > current-directory.txt
which cargo > cargo-location.txt
```

Errors still go to the terminal because carli does not yet implement standard
error redirection.

## Combining input and output

A command may use one input redirection and one output redirection together:

```sh
sort < unsorted.txt > sorted.txt
```

Spaces around operators are optional, so this is also valid:

```sh
sort<unsorted.txt>sorted.txt
```

Variable expansion and `$?` expansion apply to redirection paths in the same
way they apply to command arguments. Quoted or escaped operators remain literal
arguments:

```sh
echo ">"
echo \>
```

## Current limitations

carli currently accepts at most one input redirection and one output
redirection per command. It reports a parse error for a missing filename or a
duplicate redirection. File-descriptor syntax such as `2>`, standard-error
redirection, here-documents, and here-strings are not implemented yet.

In a pipeline, redirections apply to individual stages and override the
corresponding pipe endpoint. Ordinary pipelines and redirection always carry
raw data, so they remain suitable for scripts and existing Unix tools. See
[Pipelines](pipelines.md) for examples and precedence rules.
