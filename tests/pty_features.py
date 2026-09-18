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


def start():
    pid, fd = pty.fork()
    if pid == 0:
        environment = os.environ.copy()
        environment["HOME"] = HOME
        environment["USER"] = "carli-test"
        environment["CARLI_PROMPT"] = "{shell}:{user}:{dir}:{cwd}:{unknown}> "
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
    os.write(fd, b"/usr/bin/printf helo\x1b[D\x1b[Dl\r")
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

    # Ctrl-\ reaches the child as SIGQUIT and becomes status 131.
    os.write(fd, b"sleep 30\r")
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
    os.write(fd, b"fg %2\r")
    time.sleep(0.2)
    os.write(fd, b"\x03")
    read_until(fd)
    os.write(fd, b"fg\r")
    time.sleep(0.2)
    os.write(fd, b"\x03")
    read_until(fd)
    assert b"sleep 30" not in send(fd, "jobs")
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
finally:
    shutil.rmtree(HOME, ignore_errors=True)
