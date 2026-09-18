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
PROMPT = b"test> "


def start():
    pid, fd = pty.fork()
    if pid == 0:
        environment = os.environ.copy()
        environment["HOME"] = HOME
        environment["CARLI_PROMPT"] = PROMPT.decode()
        os.execve(CARLI, [CARLI], environment)
    return pid, fd


def read_until(fd, needle=PROMPT, timeout=5):
    data = b""
    deadline = time.monotonic() + timeout
    while needle not in data:
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            raise AssertionError(f"timed out waiting for {needle!r}; received {data!r}")
        ready, _, _ = select.select([fd], [], [], remaining)
        if ready:
            chunk = os.read(fd, 4096)
            if not chunk:
                raise AssertionError(f"PTY closed; received {data!r}")
            data += chunk
    return data


def send(fd, command):
    os.write(fd, command.encode() + b"\r")
    return read_until(fd)


try:
    pid, fd = start()
    read_until(fd)

    # Ctrl-C at the prompt cancels input and keeps the shell alive.
    os.write(fd, b"unfinished\x03")
    read_until(fd)

    # Ctrl-C reaches the foreground child and becomes status 130.
    os.write(fd, b"sleep 30\r")
    time.sleep(0.2)
    os.write(fd, b"\x03")
    read_until(fd)
    assert b"130" in send(fd, "echo $?"), "Ctrl-C did not produce status 130"

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
