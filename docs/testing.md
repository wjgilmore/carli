# Testing carli

Run every committed test from the repository root:

```sh
cargo test
```

The suite contains library unit tests, non-interactive process integration
tests, and pseudo-terminal integration tests. The PTY test is launched by a
Rust test wrapper and requires `python3`; it still runs as part of ordinary
`cargo test`.

The suite deliberately uses separately named tests for independent behavior.
This keeps a failure in an invocation edge case, parser boundary, built-in,
startup action, or interactive job-control path from being hidden inside one
large smoke test. `tests/feature_matrix.rs` contains the broad process-level
behavior matrix, while `tests/pty_features.rs` runs isolated terminal scenarios.

## README coverage map

Every item listed under README "Completed" features maps to committed automated
coverage:

| Completed README feature | Automated coverage |
| --- | --- |
| Interactive customizable prompt | Startup-configured and runtime-exported prompts, every placeholder, unknown placeholders, default prompt, and post-`cd` rebuilding in `tests/pty_features.py` |
| Interactive line editing with history navigation | Left/Right editing plus Up/Down current-session and previous-session navigation in `tests/pty_features.py` |
| Persistent `~/.carli_history` | Ctrl-D save, `exit` save, file contents, reload, and recall in `tests/pty_features.py`; automation isolation in `tests/non_interactive.rs` |
| Whitespace-separated command parsing | `splits_words_on_whitespace` in `src/lib.rs` |
| Single quotes, double quotes, and escapes | Quote, empty-argument, escaped-character, and literal-expansion unit tests in `src/lib.rs` |
| External lookup through PATH | Custom executable lookup and missing/non-executable cases in `tests/builtins_and_failures.rs` |
| External execution and waiting | Output plus statuses 23, 126, 127, and signal status 143 in integration tests |
| Built-ins for state and lookup | Success, boundary, and error cases for `cd`, `pwd`, `export`, `which`, and `exit` in `tests/builtins_and_failures.rs` |
| Exported child environment | Initial value, update, embedded `=`, valid/invalid names, and child inheritance in `tests/builtins_and_failures.rs` |
| `$NAME` expansion | Unquoted, double-quoted, unset, empty, underscore/digit names, invalid starts, single-quoted, and escaped cases in `src/lib.rs` |
| `${NAME}` expansion | Unquoted, double-quoted, single-quoted, invalid, and unclosed cases in `src/lib.rs` |
| `$?` expansion | Parser quoting tests plus external, built-in, parse, Ctrl-C, Ctrl-\\, and startup statuses across integration tests |
| `<`, `>`, and `>>` redirection | Parser syntax plus real input, truncate, append, quoted paths, built-in output, and all documented failures across both non-interactive integration files |
| Foreground process groups and terminal signals | Prompt Ctrl-C, foreground Ctrl-C/Ctrl-\\, Ctrl-Z, shell survival, and terminal return in `tests/pty_features.py` |
| Terminal-mode preservation | Exact termios snapshots, normal exit, signal termination, repeated stops, `bg`→`fg`, shell recovery, and job-mode restoration in isolated `tests/pty_edge_cases.py` scenarios |
| `jobs`, `fg`, and `bg` | Stopped/running states, output success/failure, percent/numeric/default selectors, multiple jobs, repeated lifecycle transitions, completion, and errors in PTY tests |
| `-c` and batch input | Output, statuses, usage errors, state persistence, blank lines, EOF, history isolation, and last-status behavior across both non-interactive integration files |
| XDG-aware startup configuration | XDG precedence, HOME fallback, interactive/login loading, automation isolation, comments, continued errors, startup status, `exit`, prompt configuration, and default PATH across integration and PTY tests |
| Literal variables in single quotes or after escapes | Dedicated literal-expansion unit tests in `src/lib.rs` |
| Graceful blank input, EOF, parse errors, and command errors | Unit parse errors plus batch blank/EOF statuses, interactive Ctrl-D status and job cleanup, history I/O failures, terminal recovery after interactive errors, command-not-found, invalid invocation, and redirection failures in integration tests |

The coverage map is a summary rather than the test inventory. Run
`cargo test -- --list` to see every independently named test.

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
