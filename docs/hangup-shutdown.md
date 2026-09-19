# Hangup and session shutdown

A login shell must handle loss of its terminal without abandoning children or
leaving stopped processes behind. `carli` treats `SIGHUP` as a request for an
orderly session shutdown.

## Signal-handler design

The signal handler performs no allocation, file I/O, terminal operation, or job
list traversal. It stores a lock-free atomic flag and uses the async-signal-safe
`raise` operation to wake the line editor when it is blocked waiting for input.
All cleanup occurs afterward in ordinary Rust control flow.

`carli` installs the handler without `SA_RESTART`, allowing a foreground
`waitpid` to return after a hangup. External children restore the default
`SIGHUP` disposition before execution, so they do not inherit the shell's
handler.

## Cleanup behavior

After receiving `SIGHUP`, carli:

1. stops accepting commands;
2. wakes a blocked interactive prompt or foreground wait;
3. sends `SIGHUP` and then `SIGCONT` to the current foreground job when needed;
4. saves interactive history where possible;
5. sends `SIGHUP` and `SIGCONT` to all retained background and stopped jobs; and
6. exits with status `129`, which is `128 + SIGHUP`.

Sending `SIGCONT` after `SIGHUP` lets a stopped process run its default action or
signal handler instead of remaining suspended indefinitely.

History or terminal errors are reported but cannot prevent shutdown. As with
all Unix programs, `SIGKILL` cannot be caught and therefore bypasses this path.

## Automated verification

Separate pseudo-terminal tests deliver `SIGHUP` while carli is:

- blocked at its prompt;
- waiting for a foreground job;
- tracking a running background job; and
- tracking a stopped job.

Another test closes the PTY master to exercise a real controlling-terminal
disconnect rather than directly signaling the shell.

The tests verify status `129`, saved history, and disappearance of every child.
Run them through the complete suite with `cargo test`.
