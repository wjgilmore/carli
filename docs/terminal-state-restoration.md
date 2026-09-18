# Terminal-state preservation and restoration

Interactive terminal programs temporarily change attributes such as input
canonicalization, character echo, signal generation, and special control
characters. Well-behaved programs normally restore those attributes before
exiting. A shell cannot rely on that behavior: a crash, signal, or suspended
program can otherwise leave the login terminal in raw mode or with echo
disabled.

Carli preserves terminal state around every interactive foreground external
command. This is an installation-safety feature; it reduces the risk that a
failed program leaves the only login shell apparently frozen or unusable.

## Foreground command lifecycle

Immediately before launching an interactive foreground command, carli reads and
saves its current terminal attributes with `tcgetattr`. After creating the
child's process group, carli transfers terminal ownership to that group and
waits for it.

When the command exits normally, is terminated by a signal, or stops, carli:

1. returns terminal ownership to carli's process group;
2. restores the saved shell attributes with `tcsetattr(TCSADRAIN)`; and
3. only then displays another prompt.

Restoration includes the complete termios state, not only the `ECHO` flag. It
therefore recovers canonical input, signal processing, input/output flags,
control characters, and configured speeds as one consistent snapshot.

If saving the shell snapshot or restoring it fails, carli reports the error and
returns a failure status rather than silently presenting a prompt on a terminal
whose state is unknown. If capturing a stopped job's attributes fails, carli
reports that failure and retains the job; `fg` can still resume it using the
terminal's current attributes.

## Stopped jobs and `fg`

A stopped full-screen program must later resume with the terminal state it was
using, while carli needs its own state to read commands in the meantime. When a
foreground job stops, carli captures the job's current attributes before
reclaiming the terminal and restoring the shell snapshot.

When `fg` resumes that job, carli performs the reverse transition:

1. save the shell's current attributes;
2. give the terminal to the job's process group;
3. restore the job's captured attributes;
4. send `SIGCONT`; and
5. wait for the job again.

If the job stops repeatedly, its saved snapshot is replaced with the newest
state each time. `bg` does not apply foreground terminal attributes because a
background job does not own the terminal.

## Scope and limitations

Terminal restoration applies only to interactive foreground execution. Batch
input and `-c` mode do not manipulate terminal ownership or attributes.

The current implementation deliberately restores the pre-command snapshot even
after a normal exit. As a consequence, an external command such as `stty` cannot
make a persistent terminal-mode change to carli's interactive session. This is
the safer behavior for the current login-shell readiness phase.

No recovery is possible after carli itself is forcibly killed with `SIGKILL` or
after the terminal device disappears. Those conditions cannot be handled by
process cleanup code.

## Automated verification

The PTY integration suite runs isolated scenarios in which helper processes:

- disable both echo and canonical input, then exit without restoring either;
- disable those modes and receive terminal-generated `SIGINT`; and
- disable echo, stop, verify carli recovered its own mode, resume through `fg`,
  and verify the job received its saved mode again.

All scenarios also execute another command afterward, proving that the prompt
remains usable. Run them with the rest of the committed suite:

```sh
cargo test
```
