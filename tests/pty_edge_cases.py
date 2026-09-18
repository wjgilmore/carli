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


def wait_process_gone(process_id, description):
    deadline = time.monotonic() + 3
    while True:
        try:
            os.kill(process_id, 0)
        except OSError as error:
            if error.errno == errno.ESRCH:
                return
            raise
        if time.monotonic() >= deadline:
            raise AssertionError(f"{description} {process_id} survived shell cleanup")
        time.sleep(0.05)


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
    wait_process_gone(child_pid, "stopped job")


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


def scenario_complete_termios_snapshot_restored():
    before = os.path.join(HOME, "termios-before")
    after = os.path.join(HOME, "termios-after")
    pid, fd = start()
    read_until(fd)
    send(
        fd,
        f"/usr/bin/python3 -c 'import termios; open(\"{before}\",\"w\").write(repr(termios.tcgetattr(0)))'",
    )
    command = (
        "/usr/bin/python3 -c 'import os,termios; a=termios.tcgetattr(0); "
        "a[0]^=termios.ICRNL|termios.IXON; a[1]^=termios.OPOST; "
        "a[3]^=termios.ECHO|termios.ICANON|termios.IEXTEN; "
        "a[6][termios.VEOF]=b\"x\"; termios.tcsetattr(0,termios.TCSANOW,a); "
        "os._exit(0)'"
    )
    send(fd, command)
    send(
        fd,
        f"/usr/bin/python3 -c 'import termios; open(\"{after}\",\"w\").write(repr(termios.tcgetattr(0)))'",
    )
    with open(before, encoding="utf-8") as before_file:
        before_modes = before_file.read()
    with open(after, encoding="utf-8") as after_file:
        after_modes = after_file.read()
    assert after_modes == before_modes, (before_modes, after_modes)
    assert exit_shell(pid, fd) == 0


def scenario_terminal_modes_repeated_stop_resume():
    pid, fd = start()
    read_until(fd)
    command = (
        "/usr/bin/python3 -c 'import os,signal,sys,termios; "
        "a=termios.tcgetattr(0); a[3]&=~termios.ECHO; "
        "termios.tcsetattr(0,termios.TCSANOW,a); os.kill(os.getpid(),signal.SIGTSTP); "
        "a=termios.tcgetattr(0); "
        "sys.exit(41) if a[3]&termios.ECHO else None; "
        "a[3]|=termios.ECHO; a[3]&=~termios.ICANON; "
        "termios.tcsetattr(0,termios.TCSANOW,a); os.kill(os.getpid(),signal.SIGTSTP); "
        "a=termios.tcgetattr(0); "
        "sys.exit(0 if (a[3]&termios.ECHO and not a[3]&termios.ICANON) else 42)'"
    )
    first = send(fd, command)
    assert b"[1] Stopped" in first, first
    assert_shell_modes_are_canonical_and_echoing(fd)
    second = send(fd, "fg")
    assert b"[1] Stopped" in second, second
    assert_shell_modes_are_canonical_and_echoing(fd)
    send(fd, "fg")
    assert b"0" in send(fd, "/usr/bin/printf %s $?")
    assert_shell_modes_are_canonical_and_echoing(fd)
    assert exit_shell(pid, fd) == 0


def scenario_terminal_modes_bg_then_fg():
    pid, fd = start()
    read_until(fd)
    helper = os.path.join(HOME, "bg-fg-helper.py")
    with open(helper, "w", encoding="utf-8") as helper_file:
        helper_file.write(
            "import os, signal, sys, termios, time\n"
            "a = termios.tcgetattr(0)\n"
            "a[3] &= ~termios.ECHO\n"
            "termios.tcsetattr(0, termios.TCSANOW, a)\n"
            "os.kill(os.getpid(), signal.SIGTSTP)\n"
            "while os.tcgetpgrp(0) != os.getpgrp():\n"
            "    time.sleep(0.02)\n"
            "sys.exit(0 if not (termios.tcgetattr(0)[3] & termios.ECHO) else 42)\n"
        )
    stopped = send(fd, f"/usr/bin/python3 {helper}")
    assert b"[1] Stopped" in stopped, stopped
    assert b"[1]" in send(fd, "bg")
    assert_shell_modes_are_canonical_and_echoing(fd)
    send(fd, "fg")
    assert b"0" in send(fd, "/usr/bin/printf %s $?")
    assert_shell_modes_are_canonical_and_echoing(fd)
    assert exit_shell(pid, fd) == 0


def scenario_interactive_errors_preserve_terminal():
    pid, fd = start()
    read_until(fd)
    cases = [
        ("definitely-not-a-carli-command", b"command not found", b"127"),
        ('echo "unterminated', b"unclosed double quote", b"2"),
        ("pwd > /definitely/missing/directory/file", b"No such file", b"1"),
    ]
    for command, message, status in cases:
        result = send(fd, command)
        assert message in result, result
        assert status in send(fd, "/usr/bin/printf %s $?")
        assert_shell_modes_are_canonical_and_echoing(fd)
    assert b"still-usable" in send(fd, "/usr/bin/printf still-usable")
    assert exit_shell(pid, fd) == 0


def scenario_fg_output_failure_retains_job():
    pid, fd = start()
    read_until(fd)
    stop_command(fd, "sleep 30")
    failed = send(fd, "fg %1 > /dev/full")
    assert b"could not write output" in failed, failed
    assert b"[1] Stopped sleep 30" in send(fd, "jobs")
    os.write(fd, b"fg %1\r")
    interrupt_foreground(fd)
    assert_shell_modes_are_canonical_and_echoing(fd)
    assert exit_shell(pid, fd) == 0


def scenario_ctrl_d_preserves_last_status():
    pid, fd = start()
    read_until(fd)
    send(fd, "sh -c 'exit 23'")
    os.write(fd, b"\x04")
    _, raw_status = os.waitpid(pid, 0)
    assert os.waitstatus_to_exitcode(raw_status) == 23


def scenario_ctrl_d_hangs_up_stopped_job():
    pid_file = os.path.join(HOME, "ctrl-d-child.pid")
    pid, fd = start()
    read_until(fd)
    stop_command(fd, f"sh -c 'echo $$ > {pid_file}; sleep 30'")
    with open(pid_file, encoding="utf-8") as child_pid_file:
        child_pid = int(child_pid_file.read())
    os.write(fd, b"\x04")
    _, raw_status = os.waitpid(pid, 0)
    assert os.waitstatus_to_exitcode(raw_status) == 148
    wait_process_gone(child_pid, "stopped job")


def scenario_history_load_and_save_errors_are_nonfatal():
    os.mkdir(os.path.join(HOME, ".carli_history"))
    pid, fd = start()
    startup = read_until(fd)
    assert b"could not load history" in startup, startup
    assert b"usable" in send(fd, "/usr/bin/printf usable")
    os.write(fd, b"exit 0\r")
    output = b""
    while True:
        try:
            chunk = os.read(fd, 4096)
            if not chunk:
                break
            output += chunk
        except OSError as error:
            if error.errno == errno.EIO:
                break
            raise
    _, raw_status = os.waitpid(pid, 0)
    assert os.waitstatus_to_exitcode(raw_status) == 0
    assert b"could not save history" in output, output


def scenario_blank_lines_are_not_saved_to_history():
    pid, fd = start()
    read_until(fd)
    send(fd, "   ")
    send(fd, "\t")
    send(fd, "/usr/bin/printf history-marker")
    assert exit_shell(pid, fd) == 0
    with open(os.path.join(HOME, ".carli_history"), encoding="utf-8") as history_file:
        history = history_file.read().splitlines()
    assert "   " not in history
    assert "\t" not in history
    assert "/usr/bin/printf history-marker" in history


def scenario_sighup_at_prompt_saves_history():
    pid, fd = start()
    read_until(fd)
    send(fd, "/usr/bin/printf hangup-history-marker")
    os.kill(pid, signal.SIGHUP)
    _, raw_status = os.waitpid(pid, 0)
    assert os.waitstatus_to_exitcode(raw_status) == 129
    with open(os.path.join(HOME, ".carli_history"), encoding="utf-8") as history_file:
        assert "/usr/bin/printf hangup-history-marker" in history_file.read()


def scenario_sighup_cleans_stopped_job():
    pid_file = os.path.join(HOME, "sighup-stopped.pid")
    pid, fd = start()
    read_until(fd)
    stop_command(fd, f"sh -c 'echo $$ > {pid_file}; sleep 30'")
    with open(pid_file, encoding="utf-8") as child_pid_file:
        child_pid = int(child_pid_file.read())
    os.kill(pid, signal.SIGHUP)
    _, raw_status = os.waitpid(pid, 0)
    assert os.waitstatus_to_exitcode(raw_status) == 129
    wait_process_gone(child_pid, "stopped job")


def scenario_sighup_cleans_background_job():
    pid_file = os.path.join(HOME, "sighup-background.pid")
    pid, fd = start()
    read_until(fd)
    stop_command(fd, f"sh -c 'echo $$ > {pid_file}; sleep 30'")
    with open(pid_file, encoding="utf-8") as child_pid_file:
        child_pid = int(child_pid_file.read())
    send(fd, "bg")
    os.kill(pid, signal.SIGHUP)
    _, raw_status = os.waitpid(pid, 0)
    assert os.waitstatus_to_exitcode(raw_status) == 129
    wait_process_gone(child_pid, "background job")


def scenario_sighup_cleans_foreground_job():
    pid_file = os.path.join(HOME, "sighup-foreground.pid")
    pid, fd = start()
    read_until(fd)
    os.write(fd, f"sh -c 'echo $$ > {pid_file}; sleep 30'\r".encode())
    deadline = time.monotonic() + 3
    while not os.path.exists(pid_file):
        if time.monotonic() >= deadline:
            raise AssertionError("foreground job did not write its pid")
        time.sleep(0.02)
    with open(pid_file, encoding="utf-8") as child_pid_file:
        child_pid = int(child_pid_file.read())
    os.kill(pid, signal.SIGHUP)
    _, raw_status = os.waitpid(pid, 0)
    assert os.waitstatus_to_exitcode(raw_status) == 129
    wait_process_gone(child_pid, "foreground job")


def scenario_real_pty_disconnect_saves_history():
    pid, fd = start()
    read_until(fd)
    send(fd, "/usr/bin/printf disconnect-history-marker")
    os.close(fd)
    _, raw_status = os.waitpid(pid, 0)
    assert os.waitstatus_to_exitcode(raw_status) == 129
    with open(os.path.join(HOME, ".carli_history"), encoding="utf-8") as history_file:
        assert "/usr/bin/printf disconnect-history-marker" in history_file.read()


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
    "complete_termios_snapshot_restored": scenario_complete_termios_snapshot_restored,
    "terminal_modes_repeated_stop_resume": scenario_terminal_modes_repeated_stop_resume,
    "terminal_modes_bg_then_fg": scenario_terminal_modes_bg_then_fg,
    "interactive_errors_preserve_terminal": scenario_interactive_errors_preserve_terminal,
    "fg_output_failure_retains_job": scenario_fg_output_failure_retains_job,
    "ctrl_d_preserves_last_status": scenario_ctrl_d_preserves_last_status,
    "ctrl_d_hangs_up_stopped_job": scenario_ctrl_d_hangs_up_stopped_job,
    "history_load_and_save_errors_are_nonfatal": scenario_history_load_and_save_errors_are_nonfatal,
    "blank_lines_are_not_saved_to_history": scenario_blank_lines_are_not_saved_to_history,
    "sighup_at_prompt_saves_history": scenario_sighup_at_prompt_saves_history,
    "sighup_cleans_stopped_job": scenario_sighup_cleans_stopped_job,
    "sighup_cleans_background_job": scenario_sighup_cleans_background_job,
    "sighup_cleans_foreground_job": scenario_sighup_cleans_foreground_job,
    "real_pty_disconnect_saves_history": scenario_real_pty_disconnect_saves_history,
}

try:
    SCENARIOS[SCENARIO]()
finally:
    shutil.rmtree(HOME, ignore_errors=True)
