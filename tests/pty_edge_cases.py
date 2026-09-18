#!/usr/bin/env python3
import errno
import os
import pty
import select
import shutil
import signal
import sys
import tempfile
import time


CARLI = os.path.abspath(sys.argv[1])
SCENARIO = sys.argv[2]
HOME = tempfile.mkdtemp(prefix=f"carli-pty-{SCENARIO}-", dir="/tmp")
PROMPT = b"edge> "


def start():
    pid, fd = pty.fork()
    if pid == 0:
        environment = os.environ.copy()
        environment["HOME"] = HOME
        environment["CARLI_PROMPT"] = PROMPT.decode()
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


def stop_command(fd, command):
    os.write(fd, command.encode() + b"\r")
    time.sleep(0.2)
    os.write(fd, b"\x1a")
    return read_until(fd)


def interrupt_foreground(fd):
    time.sleep(0.2)
    os.write(fd, b"\x03")
    return read_until(fd)


def exit_shell(pid, fd):
    os.write(fd, b"exit 0\r")
    _, raw_status = os.waitpid(pid, 0)
    return os.waitstatus_to_exitcode(raw_status)


def scenario_repeated_prompt_interrupts():
    pid, fd = start()
    read_until(fd)
    for _ in range(3):
        os.write(fd, b"partial input\x03")
        read_until(fd)
        assert b"130" in send(fd, "/usr/bin/printf %s $?")
    assert b"alive" in send(fd, "/usr/bin/printf alive")
    assert exit_shell(pid, fd) == 0


def scenario_job_selection_errors():
    pid, fd = start()
    read_until(fd)
    for command, message in [
        ("fg", b"no current job"),
        ("bg", b"no current job"),
    ]:
        assert message in send(fd, command)
    stop_command(fd, "sleep 30")
    for command, message in [
        ("fg nonsense", b"invalid job identifier"),
        ("bg %999", b"no such job"),
        ("fg 1 extra", b"too many arguments"),
        ("bg 1 extra", b"too many arguments"),
    ]:
        assert message in send(fd, command)
        assert b"1" in send(fd, "/usr/bin/printf %s $?")
    os.write(fd, b"fg %1\r")
    interrupt_foreground(fd)
    assert exit_shell(pid, fd) == 0


def scenario_bg_rejects_running_job():
    pid, fd = start()
    read_until(fd)
    stop_command(fd, "sleep 30")
    assert b"[1] sleep 30" in send(fd, "bg %1")
    already = send(fd, "bg 1")
    assert b"already running" in already
    assert b"1" in send(fd, "/usr/bin/printf %s $?")
    os.write(fd, b"fg\r")
    interrupt_foreground(fd)
    assert exit_shell(pid, fd) == 0


def scenario_fg_default_and_normal_exit_status():
    pid, fd = start()
    read_until(fd)
    stop_command(fd, "sh -c 'sleep 30; exit 17'")
    os.write(fd, b"fg\r")
    time.sleep(0.2)
    os.write(fd, b"\x03")
    read_until(fd)
    assert b"130" in send(fd, "/usr/bin/printf %s $?")

    stop_command(fd, "sh -c 'sleep 0.2; exit 17'")
    foregrounded = send(fd, "fg")
    assert b"sh -c sleep 0.2; exit 17" in foregrounded
    assert b"17" in send(fd, "/usr/bin/printf %s $?")
    assert exit_shell(pid, fd) == 0


def scenario_background_terminal_read_stops_again():
    pid, fd = start()
    read_until(fd)
    stop_command(fd, "cat")
    send(fd, "bg")
    time.sleep(0.2)
    send(fd, "")
    listed = send(fd, "jobs")
    assert b"Stopped cat" in listed, listed
    os.write(fd, b"fg\r")
    interrupt_foreground(fd)
    assert exit_shell(pid, fd) == 0


def scenario_shell_hangs_up_stopped_jobs():
    pid_file = os.path.join(HOME, "child.pid")
    pid, fd = start()
    read_until(fd)
    stop_command(fd, f"sh -c 'echo $$ > {pid_file}; sleep 30'")
    with open(pid_file, encoding="utf-8") as child_pid_file:
        child_pid = int(child_pid_file.read())
    assert exit_shell(pid, fd) == 0
    deadline = time.monotonic() + 3
    while True:
        try:
            os.kill(child_pid, 0)
        except OSError as error:
            if error.errno == errno.ESRCH:
                break
            raise
        if time.monotonic() >= deadline:
            raise AssertionError(f"job {child_pid} survived shell exit")
        time.sleep(0.05)


def scenario_prompt_root_and_unknown_user():
    pid, fd = pty.fork()
    if pid == 0:
        environment = os.environ.copy()
        environment["HOME"] = HOME
        environment["CARLI_PROMPT"] = "{user}:{dir}:{cwd}> "
        environment.pop("USER", None)
        environment.pop("XDG_CONFIG_HOME", None)
        environment.pop("CARLI_SYSTEM_CONFIG", None)
        os.chdir("/")
        os.execve(CARLI, [CARLI], environment)
    read_until(fd, b"unknown:/:/> ")
    os.write(fd, b"exit 0\r")
    _, raw_status = os.waitpid(pid, 0)
    assert os.waitstatus_to_exitcode(raw_status) == 0


def scenario_job_completion_notifications():
    pid_file = os.path.join(HOME, "signal-child.pid")
    pid, fd = start()
    read_until(fd)

    stop_command(fd, "sh -c 'sleep 0.2; exit 7'")
    send(fd, "bg")
    time.sleep(0.4)
    completed = send(fd, "")
    assert b"[1] Done (7)" in completed, completed
    assert b"sh -c sleep 0.2; exit 7" not in send(fd, "jobs")

    stop_command(fd, f"sh -c 'echo $$ > {pid_file}; sleep 30'")
    with open(pid_file, encoding="utf-8") as child_pid_file:
        child_pid = int(child_pid_file.read())
    send(fd, "bg")
    os.killpg(child_pid, signal.SIGTERM)
    time.sleep(0.2)
    terminated = send(fd, "")
    assert b"[2] Terminated (SIGTERM)" in terminated, terminated
    assert b"sleep 30" not in send(fd, "jobs")
    assert exit_shell(pid, fd) == 0


def assert_shell_modes_are_canonical_and_echoing(fd):
    command = (
        "/usr/bin/python3 -c 'import sys,termios; f=termios.tcgetattr(0)[3]; "
        "sys.exit(0 if (f&termios.ECHO and f&termios.ICANON) else 42)'"
    )
    send(fd, command)
    assert b"0" in send(fd, "/usr/bin/printf %s $?")


def scenario_terminal_modes_normal_exit():
    pid, fd = start()
    read_until(fd)
    assert_shell_modes_are_canonical_and_echoing(fd)
    command = (
        "/usr/bin/python3 -c 'import os,termios; "
        "a=termios.tcgetattr(0); a[3]&=~(termios.ECHO|termios.ICANON); "
        "termios.tcsetattr(0,termios.TCSANOW,a); os._exit(0)'"
    )
    send(fd, command)
    assert_shell_modes_are_canonical_and_echoing(fd)
    assert b"usable" in send(fd, "/usr/bin/printf usable")
    assert exit_shell(pid, fd) == 0


def scenario_terminal_modes_signal_exit():
    pid, fd = start()
    read_until(fd)
    command = (
        "/usr/bin/python3 -c 'import termios,time; "
        "a=termios.tcgetattr(0); a[3]&=~(termios.ECHO|termios.ICANON); "
        "termios.tcsetattr(0,termios.TCSANOW,a); time.sleep(30)'"
    )
    os.write(fd, command.encode() + b"\r")
    time.sleep(0.2)
    os.write(fd, b"\x03")
    read_until(fd)
    assert b"130" in send(fd, "/usr/bin/printf %s $?")
    assert_shell_modes_are_canonical_and_echoing(fd)
    assert exit_shell(pid, fd) == 0


def scenario_terminal_modes_stop_resume():
    pid, fd = start()
    read_until(fd)
    command = (
        "/usr/bin/python3 -c 'import os,signal,sys,termios; "
        "a=termios.tcgetattr(0); a[3]&=~termios.ECHO; "
        "termios.tcsetattr(0,termios.TCSANOW,a); "
        "os.kill(os.getpid(),signal.SIGTSTP); "
        "sys.exit(0 if not (termios.tcgetattr(0)[3]&termios.ECHO) else 42)'"
    )
    stopped = send(fd, command)
    assert b"Stopped" in stopped, stopped
    assert_shell_modes_are_canonical_and_echoing(fd)
    foregrounded = send(fd, "fg")
    assert b"python3 -c" in foregrounded, foregrounded
    assert b"0" in send(fd, "/usr/bin/printf %s $?")
    assert_shell_modes_are_canonical_and_echoing(fd)
    assert exit_shell(pid, fd) == 0


SCENARIOS = {
    "repeated_prompt_interrupts": scenario_repeated_prompt_interrupts,
    "job_selection_errors": scenario_job_selection_errors,
    "bg_rejects_running_job": scenario_bg_rejects_running_job,
    "fg_default_and_normal_exit_status": scenario_fg_default_and_normal_exit_status,
    "background_terminal_read_stops_again": scenario_background_terminal_read_stops_again,
    "shell_hangs_up_stopped_jobs": scenario_shell_hangs_up_stopped_jobs,
    "prompt_root_and_unknown_user": scenario_prompt_root_and_unknown_user,
    "job_completion_notifications": scenario_job_completion_notifications,
    "terminal_modes_normal_exit": scenario_terminal_modes_normal_exit,
    "terminal_modes_signal_exit": scenario_terminal_modes_signal_exit,
    "terminal_modes_stop_resume": scenario_terminal_modes_stop_resume,
}

try:
    SCENARIOS[SCENARIO]()
finally:
    shutil.rmtree(HOME, ignore_errors=True)
