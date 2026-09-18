# Testing carli

Run every committed test from the repository root:

```sh
cargo test
```

The suite contains library unit tests, non-interactive process integration
tests, and pseudo-terminal integration tests. The PTY test is launched by a
Rust test wrapper and requires `python3`; it still runs as part of ordinary
`cargo test`.

## Coverage map

The tests are organized by behavior rather than by implementation function:

| Feature | Automated coverage |
| --- | --- |
| Word splitting, quotes, escapes, and empty arguments | `src/lib.rs` unit tests |
| `$NAME`, `${NAME}`, unset variables, and `$?` expansion | `src/lib.rs` unit tests |
| Comments and literal special characters | `src/lib.rs` unit tests |
| Redirection parsing and malformed syntax | `src/lib.rs` unit tests |
| Real `<`, `>`, and `>>` file behavior | `tests/non_interactive.rs` |
| Redirection file and syntax failures | `tests/builtins_and_failures.rs` |
| External lookup through PATH, waiting, output, and statuses | `tests/builtins_and_failures.rs` |
| `cd`, `pwd`, `export`, `which`, and `exit` success and errors | `tests/builtins_and_failures.rs` |
| Blank input, EOF, parse errors, and command errors | `tests/builtins_and_failures.rs` |
| `-c` output, status propagation, usage, and history isolation | `tests/non_interactive.rs` |
| Batch state, EOF status, `cd`, and exported child environment | Both non-interactive integration files |
| XDG and HOME startup files, precedence, errors, exit, and default PATH | Both non-interactive integration files |
| Prompt placeholders and changes after `cd` | `tests/pty_features.py` |
| Cursor editing and Ctrl-C at the prompt | `tests/pty_features.py` |
| Persistent history save, load, and Up-arrow recall | `tests/pty_features.py` |
| Foreground Ctrl-C and Ctrl-\\ statuses | `tests/pty_features.py` |
| Ctrl-Z, multiple jobs, `jobs`, `bg`, `fg`, and job selection | `tests/pty_features.py` |

## Complete verification

Before committing, run:

```sh
cargo fmt --check
cargo test
cargo clippy --all-targets -- -D warnings
git diff --check
```

`cargo test` is the authoritative feature test command. The other commands
check formatting, lints, and patch whitespace rather than runtime behavior.
