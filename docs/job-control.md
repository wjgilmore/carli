# Basic job control

carli tracks commands stopped with <kbd>Ctrl-Z</kbd> and provides `jobs`, `fg`,
and `bg` built-ins for inspecting and resuming them. Job control builds on
carli's foreground process-group and terminal-ownership support.

## Stopping a foreground command

Press <kbd>Ctrl-Z</kbd> while a foreground command is running to send `SIGTSTP`
to its process group:

```text
carli $ sleep 300
^Z[1] Stopped sleep 300
carli $
```

carli assigns the stopped command a numeric job ID, records its process ID and
process group, reclaims the terminal, and displays a new prompt. The value of
`$?` is `128 + stop_signal`; for `SIGTSTP`, that is normally `148`.

## Listing jobs

The `jobs` built-in displays every job currently tracked by carli:

```text
carli $ jobs
[1] Stopped sleep 300
[2] Running another-command
```

`jobs` writes to standard output, so its output can be redirected:

```sh
jobs > current-jobs.txt
```

Before each prompt, carli checks tracked children without blocking. Exited or
signaled jobs are reaped and removed, and state changes caused by `SIGSTOP`,
`SIGTSTP`, or `SIGCONT` are reflected in later `jobs` output.

## Resuming in the foreground

`fg` gives a job control of the terminal and waits for it again:

```sh
fg %1
```

The percent sign is optional:

```sh
fg 1
```

With no argument, `fg` selects the job with the highest job ID, which is the
most recently created tracked job:

```sh
fg
```

If the job was stopped, carli sends `SIGCONT` to its entire process group. The
job's saved terminal attributes are restored before it continues, so a
full-screen program resumes in the mode it was using. The job may then exit
normally, be interrupted with <kbd>Ctrl-C</kbd>, or be stopped again with
<kbd>Ctrl-Z</kbd>. Carli always attempts to reclaim the terminal and restore its
own attributes before presenting another prompt.

## Resuming in the background

`bg` sends `SIGCONT` without transferring terminal ownership:

```sh
bg %1
```

As with `fg`, the percent sign is optional and omitting the argument selects the
most recently created job.

A background job must not read from the terminal. If it attempts to do so, the
terminal driver normally stops it with `SIGTTIN`; a later `jobs` invocation will
show it as stopped. Background output is not yet redirected automatically and
may appear alongside the prompt.

## Errors and exit statuses

The job-control built-ins return status `0` on success and `1` for errors such
as:

- no tracked jobs;
- an unknown or malformed job ID;
- too many arguments;
- attempting `bg` on a job already marked as running; or
- using `fg` or `bg` when carli does not have an interactive terminal.

If a foregrounded job exits or is terminated, its result becomes `$?` just as
it would for a newly launched foreground command.

## Shell exit and cleanup

When carli exits, it sends `SIGHUP` and then `SIGCONT` to every remaining job
process group. `SIGCONT` ensures that a stopped job can observe the hangup
instead of remaining suspended after its shell disappears.

## Current limitations

This is basic job control. The following features are not implemented yet:

- launching a new command directly in the background with `&`;
- `%+`, `%-`, and textual job selectors;
- job markers and the complete formatting used by Bash or Zsh;
- pipelines containing multiple processes in one job; and
- immediate asynchronous notifications while the line editor is waiting for
  input.

These limitations do not prevent stopped foreground commands from being safely
recovered, resumed, or cleaned up.
