# Why Signal Handling Matters in Shell Development

An interactive shell does more than start programs. It coordinates the terminal,
the programs running inside it, and controls such as <kbd>Ctrl-C</kbd> and
<kbd>Ctrl-Z</kbd>. Unix signals are a central part of that coordination.

Without deliberate signal handling, a shell might exit along with a command,
suspend itself accidentally, or leave the terminal without a process able to
accept input. These failures are especially serious for a login shell because
there may not be another shell underneath it to recover the session.

## Signals used by interactive shells

Several signals commonly affect a shell and its child processes:

- `SIGINT` requests an interrupt. A terminal normally sends it when the user
  presses <kbd>Ctrl-C</kbd>.
- `SIGTSTP` requests a suspension. A terminal normally sends it when the user
  presses <kbd>Ctrl-Z</kbd>.
- `SIGCONT` resumes a suspended process.
- `SIGCHLD` informs the shell that a child has stopped, continued, or exited.
- `SIGWINCH` reports that the terminal window changed size.
- `SIGHUP` commonly reports that a controlling terminal has closed.
- `SIGTTIN` and `SIGTTOU` stop background processes that attempt certain
  terminal operations.

Signals do not represent ordinary keyboard input. The terminal driver interprets
special key combinations and sends signals to the terminal's foreground process
group.

## Why process groups matter

A shell and a command should not compete for control of the terminal. When carli
starts an interactive program such as Vim, the usual relationship should be:

```text
Terminal
  |
  +-- foreground process group: Vim
  |     receives input, Ctrl-C, and Ctrl-Z
  |
  +-- carli
        waits until the foreground job stops or exits
```

The shell normally creates a new process group for a command and transfers
foreground terminal ownership to that group. After the command exits or stops,
the shell takes ownership back before displaying another prompt.

If carli and its child remain in the same foreground process group, a terminal
signal can reach both. For example, <kbd>Ctrl-C</kbd> intended for `sleep` could
also interrupt carli, while <kbd>Ctrl-Z</kbd> could suspend both processes and
leave no active shell to regain control.

## Parent and child responsibilities

While a foreground command is running, the shell should avoid reacting to
interactive signals meant for that command. The child, however, should begin
with the normal signal behavior expected by Unix programs.

A shell therefore generally needs to:

1. Create a process group for the foreground command or pipeline.
2. Restore appropriate default signal behavior in the child.
3. Give the child's process group control of the terminal.
4. Wait for the job to exit, stop, or continue.
5. Record how the job ended.
6. Return terminal control to the shell.
7. Restore the shell's terminal settings and display the next prompt.

This sequence must also work when process creation fails or the child terminates
abnormally. Cleanup paths are just as important as the successful path.

## Exit statuses and signals

Shell users expect `$?` to describe how the previous command ended. A normal
exit provides a numeric status directly. When a signal terminates a command,
shells conventionally expose `128 + signal_number`.

For example, `SIGINT` is commonly signal 2, so interrupting a command with
<kbd>Ctrl-C</kbd> normally produces status 130. Tracking wait results correctly
is therefore necessary for both signal handling and accurate `$?` expansion.

## Signal handling enables job control

Foreground execution is the first part of job control. Once a shell can observe
that a process group has stopped, it can retain that job and later support
commands such as:

- `jobs` to list known jobs;
- `fg` to resume a job in the foreground; and
- `bg` to resume a job in the background.

Pipelines make this especially important because every process in a pipeline
belongs to the same job and normally shares one process group.

## What to test

Signal behavior should be tested through a pseudo-terminal, not only with unit
tests. Useful scenarios include:

```sh
sleep 60       # Ctrl-C should stop sleep and leave carli running
cat            # Ctrl-Z should stop cat and return control to carli
vim /tmp/test  # exiting Vim should restore a usable prompt
```

Tests should also cover terminal resizing, child crashes, failed program starts,
EOF, repeated interrupts at the prompt, and closing the terminal while children
are active.

Correct signal handling is not merely a convenience feature. It is what allows
an interactive shell to remain in control of the session while giving each
foreground command temporary, predictable ownership of the terminal.
