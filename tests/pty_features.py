#!/usr/bin/env python3
import os
import pty
import select
import shutil
import sys
import tempfile
import time


CARLI = os.path.abspath(sys.argv[1])
HOME = tempfile.mkdtemp(prefix="carli-pty-tests-", dir="/tmp")
START_DIRECTORY = os.getcwd()
PROMPT = f"carli:carli-test:carli:{START_DIRECTORY}:{{unknown}}> ".encode()
os.makedirs(os.path.join(HOME, ".config", "carli"))
with open(os.path.join(HOME, ".config", "carli", "config"), "w", encoding="utf-8") as config:
    config.write('export CARLI_PROMPT="{shell}:{user}:{dir}:{cwd}:{unknown}> "\n')
    config.write("export INTERACTIVE_STARTUP=loaded\n")


def start():
    pid, fd = pty.fork()
    if pid == 0:
        environment = os.environ.copy()
        environment["HOME"] = HOME
        environment["USER"] = "carli-test"
        environment.pop("CARLI_PROMPT", None)
        environment.pop("XDG_CONFIG_HOME", None)
        environment.pop("CARLI_SYSTEM_CONFIG", None)
        os.execve(CARLI, [CARLI], environment)
    return pid, fd


def read_until(fd, needle=PROMPT, timeout=5):
    data = b""
    deadline = time.monotonic() + timeout
    found = False
    while True:
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            raise AssertionError(f"timed out waiting for {needle!r}; received {data!r}")
        ready, _, _ = select.select([fd], [], [], min(remaining, 0.05) if found else remaining)
        if not ready and found:
            return data
        if ready:
            chunk = os.read(fd, 4096)
            if not chunk:
                raise AssertionError(f"PTY closed; received {data!r}")
            data += chunk
            found = needle in data


def send(fd, command):
    os.write(fd, command.encode() + b"\r")
    return read_until(fd)


try:
    pid, fd = start()
    read_until(fd)

    # The interactive startup file configures the prompt and environment.
    assert b"loaded" in send(fd, "/usr/bin/printenv INTERACTIVE_STARTUP")

    # Exporting CARLI_PROMPT inside carli takes effect at the next prompt.
    os.write(fd, b'export CARLI_PROMPT="changed:{dir}> "\r')
    read_until(fd, b"changed:carli> ")
    os.write(fd, b'export CARLI_PROMPT="{shell}:{user}:{dir}:{cwd}:{unknown}> "\r')
    read_until(fd)

    # Ctrl-C at the prompt cancels input and keeps the shell alive.
    os.write(fd, b"unfinished\x03")
    read_until(fd)

    # Prompt placeholders render and update after cd while unknown ones remain.
    changed_prompt = b"carli:carli-test:tmp:/tmp:{unknown}> "
    os.write(fd, b"cd /tmp\r")
    read_until(fd, changed_prompt)
    os.write(fd, f"cd {START_DIRECTORY}\r".encode())
    read_until(fd)

    # Cursor movement and insertion edit the line before execution.
    os.write(fd, b"/usr/bin/printf helo\x1b[D\x1b[C\x1b[Dl\r")
    edited = read_until(fd)
    assert b"hello" in edited, edited

    # Ctrl-C reaches the foreground child and becomes status 130.
    os.write(fd, b"sleep 30\r")
    time.sleep(0.2)
    os.write(fd, b"\x03")
    read_until(fd)
    interrupt_status = send(fd, "echo $?")
    assert b"130" in interrupt_status, interrupt_status

    # Ctrl-Z, jobs, bg, and fg exercise the complete job lifecycle.
    os.write(fd, b"sleep 30\r")
    time.sleep(0.2)
    os.write(fd, b"\x1a")
    stopped = read_until(fd)
    assert b"[1] Stopped sleep 30" in stopped, stopped
    assert b"[1] Stopped sleep 30" in send(fd, "jobs")
    assert b"[1] sleep 30" in send(fd, "bg %1")
    assert b"[1] Running sleep 30" in send(fd, "jobs")
    os.write(fd, b"fg %1\r")
    time.sleep(0.2)
    os.write(fd, b"\x03")
    read_until(fd)
    assert b"130" in send(fd, "echo $?")
    assert b"sleep 30" not in send(fd, "jobs")

    # Ctrl-\ reaches the child as SIGQUIT. The helper catches it and exits 131
    # so this verifies terminal signal routing without creating a core dump.
    os.write(
        fd,
        b"/usr/bin/python3 -c 'import signal,time,sys; signal.signal(signal.SIGQUIT, lambda *_: sys.exit(131)); time.sleep(30)'\r",
    )
    time.sleep(0.2)
    os.write(fd, b"\x1c")
    read_until(fd)
    assert b"131" in send(fd, "echo $?")

    # Multiple jobs support explicit IDs and the default newest-job selection.
    for expected_id in (2, 3):
        os.write(fd, b"sleep 30\r")
        time.sleep(0.2)
        os.write(fd, b"\x1a")
        stopped = read_until(fd)
        assert f"[{expected_id}] Stopped sleep 30".encode() in stopped, stopped
    listed = send(fd, "jobs")
    assert b"[2] Stopped sleep 30" in listed and b"[3] Stopped sleep 30" in listed
    jobs_file = os.path.join(HOME, "jobs.txt")
    send(fd, f'jobs > "{jobs_file}"')
    with open(jobs_file, "rb") as saved_jobs:
        contents = saved_jobs.read()
    assert b"[2] Stopped sleep 30" in contents and b"[3] Stopped sleep 30" in contents

    # bg with no argument selects the newest job; numeric IDs work without `%`.
    assert b"[3] sleep 30" in send(fd, "bg")
    os.write(fd, b"fg 3\r")
    time.sleep(0.2)
    os.write(fd, b"\x03")
    read_until(fd)
    os.write(fd, b"fg 2\r")
    time.sleep(0.2)
    os.write(fd, b"\x03")
    read_until(fd)
    assert b"sleep 30" not in send(fd, "jobs")
    jobs_error = send(fd, "jobs extra")
    assert b"too many arguments" in jobs_error
    assert b"1" in send(fd, "echo $?")
    missing_job = send(fd, "fg %99")
    assert b"no current job" in missing_job
    assert b"1" in send(fd, "echo $?")

    # Up and Down traverse current-session history in both directions.
    send(fd, "/usr/bin/printf navigation-one")
    send(fd, "/usr/bin/printf navigation-two")
    os.write(fd, b"\x1b[A\x1b[A\x1b[B\r")
    navigated = read_until(fd)
    assert b"navigation-two" in navigated, navigated

    # Leave a distinctive final history entry and exit through Ctrl-D.
    assert b"recalled-marker" in send(fd, "/usr/bin/printf recalled-marker")
    os.write(fd, b"\x04")
    _, raw_status = os.waitpid(pid, 0)
    assert os.waitstatus_to_exitcode(raw_status) == 0

    history = open(os.path.join(HOME, ".carli_history"), encoding="utf-8").read()
    assert "/usr/bin/printf recalled-marker" in history

    # A new session loads history; Up recalls and runs the previous command.
    pid, fd = start()
    read_until(fd)
    os.write(fd, b"\x1b[A\r")
    recalled = read_until(fd)
    assert b"recalled-marker" in recalled, recalled
    os.write(fd, b"exit\r")
    _, raw_status = os.waitpid(pid, 0)
    assert os.waitstatus_to_exitcode(raw_status) == 0
    history = open(os.path.join(HOME, ".carli_history"), encoding="utf-8").read()
    assert "exit" in history, "exit built-in did not save history"

    # With no CARLI_PROMPT and no startup config, carli uses its default prompt.
    default_home = tempfile.mkdtemp(prefix="carli-default-prompt-", dir="/tmp")
    pid, fd = pty.fork()
    if pid == 0:
        environment = os.environ.copy()
        environment["HOME"] = default_home
        environment.pop("CARLI_PROMPT", None)
        os.execve(CARLI, [CARLI], environment)
    read_until(fd, b"carli $ ")
    os.write(fd, b"\x04")
    _, raw_status = os.waitpid(pid, 0)
    assert os.waitstatus_to_exitcode(raw_status) == 0
    shutil.rmtree(default_home, ignore_errors=True)
finally:
    shutil.rmtree(HOME, ignore_errors=True)
