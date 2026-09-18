use nix::errno::Errno;
use nix::libc;
use nix::sys::signal::{self, SaFlags, SigAction, SigHandler, SigSet, Signal, killpg};
use nix::sys::termios::{SetArg, Termios, tcgetattr, tcsetattr};
use nix::sys::wait::{WaitPidFlag, WaitStatus, waitpid};
use nix::unistd::{Pid, getpgrp, getpid, setpgid, tcgetpgrp, tcsetpgrp};
use rustyline::DefaultEditor;
use rustyline::error::ReadlineError;
use std::fs::{File, OpenOptions};
use std::io;
use std::io::{BufRead, IsTerminal, Write};
use std::os::unix::process::CommandExt;
use std::os::unix::process::ExitStatusExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};

use carli::{OutputRedirection, ParseError, parse_command_line_with_status};

static HANGUP_REQUESTED: AtomicBool = AtomicBool::new(false);

extern "C" fn request_hangup(_: i32) {
    HANGUP_REQUESTED.store(true, Ordering::Relaxed);
    // rustyline temporarily owns SIGINT while blocked in terminal input.
    // Raising it wakes readline so the main loop can observe the hangup flag.
    // libc::raise is async-signal-safe on POSIX systems.
    unsafe {
        libc::raise(libc::SIGINT);
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum JobState {
    Running,
    Stopped,
}

struct Job {
    id: usize,
    pid: Pid,
    process_group: Pid,
    command: String,
    state: JobState,
    terminal_modes: Option<Termios>,
}

struct JobControl {
    interactive: bool,
    shell_process_group: Pid,
    shell_terminal_modes: Option<Termios>,
    jobs: Vec<Job>,
    next_job_id: usize,
}

enum CommandOutcome {
    Status(i32),
    Exit(u8),
}

struct Invocation {
    command: Option<String>,
    login_shell: bool,
}

fn history_path() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .map(|home| home.join(".carli_history"))
}

fn main() -> std::process::ExitCode {
    let invocation = match parse_invocation() {
        Ok(invocation) => invocation,
        Err(message) => {
            eprintln!("carli: {message}");
            eprintln!("usage: carli [-c COMMAND]");
            return std::process::ExitCode::from(2);
        }
    };
    let interactive = invocation.command.is_none() && io::stdin().is_terminal();
    let (shell_process_group, shell_terminal_modes) = if interactive {
        match setup_interactive_shell() {
            Ok(state) => (state.0, Some(state.1)),
            Err(error) => {
                eprintln!("carli: could not initialize job control: {error}");
                return std::process::ExitCode::FAILURE;
            }
        }
    } else {
        (getpgrp(), None)
    };
    let mut job_control = JobControl {
        interactive,
        shell_process_group,
        shell_terminal_modes,
        jobs: Vec::new(),
        next_job_id: 1,
    };

    let startup_outcome = if interactive || invocation.login_shell {
        ensure_default_path();
        load_startup_files(&mut job_control)
    } else {
        CommandOutcome::Status(0)
    };
    let initial_status = match startup_outcome {
        CommandOutcome::Status(status) => status,
        CommandOutcome::Exit(status) => {
            hang_up_jobs(&job_control.jobs);
            return std::process::ExitCode::from(status);
        }
    };

    let exit_status = match invocation.command {
        Some(command) => outcome_status(execute_line(&command, initial_status, &mut job_control)),
        None if interactive => run_interactive(&mut job_control, initial_status),
        None => run_batch(&mut job_control, initial_status),
    };

    hang_up_jobs(&job_control.jobs);
    std::process::ExitCode::from(exit_status)
}

fn parse_invocation() -> Result<Invocation, String> {
    let mut arguments = std::env::args_os();
    let program = arguments.next().unwrap_or_default();
    let login_shell = PathBuf::from(program)
        .file_name()
        .is_some_and(|name| name.to_string_lossy().starts_with('-'));
    let arguments: Vec<_> = arguments.collect();

    let command = match arguments.as_slice() {
        [] => None,
        [flag, command] if flag == "-c" => Some(
            command
                .clone()
                .into_string()
                .map_err(|_| "command is not valid UTF-8".to_string())?,
        ),
        [flag] if flag == "-c" => return Err("option `-c` requires a command".to_string()),
        [argument, ..] => {
            return Err(format!(
                "unsupported argument `{}`",
                argument.to_string_lossy()
            ));
        }
    };

    Ok(Invocation {
        command,
        login_shell,
    })
}

fn ensure_default_path() {
    if std::env::var_os("PATH").is_none() {
        // SAFETY: carli is single-threaded and performs startup initialization
        // before spawning commands.
        unsafe {
            std::env::set_var("PATH", "/usr/local/bin:/usr/bin:/bin");
        }
    }
}

fn startup_paths() -> Vec<PathBuf> {
    let system_path = std::env::var_os("CARLI_SYSTEM_CONFIG")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/etc/carli/config"));
    let mut paths = vec![system_path];
    let user_path = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .map(|directory| directory.join("carli/config"))
        .or_else(|| {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .map(|home| home.join(".config/carli/config"))
        });
    if let Some(path) = user_path {
        paths.push(path);
    }
    paths
}

fn load_startup_files(job_control: &mut JobControl) -> CommandOutcome {
    let mut last_status = 0;

    for path in startup_paths() {
        let file = match File::open(&path) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => {
                eprintln!(
                    "carli: {}: could not read startup file: {error}",
                    path.display()
                );
                last_status = 1;
                continue;
            }
        };

        for (index, line) in io::BufReader::new(file).lines().enumerate() {
            let line_number = index + 1;
            let line = match line {
                Ok(line) => line,
                Err(error) => {
                    eprintln!(
                        "carli: {}:{line_number}: could not read startup file: {error}",
                        path.display()
                    );
                    last_status = 1;
                    break;
                }
            };

            match execute_line_from(&line, last_status, job_control, Some((&path, line_number))) {
                CommandOutcome::Status(status) => last_status = status,
                CommandOutcome::Exit(status) => return CommandOutcome::Exit(status),
            }
        }
    }

    CommandOutcome::Status(last_status)
}

fn run_interactive(job_control: &mut JobControl, initial_status: i32) -> u8 {
    let mut editor = DefaultEditor::new().expect("carli: could not initialize line editor");
    let history_path = history_path();

    if let Some(path) = &history_path
        && path.exists()
        && let Err(error) = editor.load_history(path)
    {
        eprintln!("carli: could not load history: {error}");
    }

    let mut last_status = initial_status;
    let exit_status = loop {
        if HANGUP_REQUESTED.load(Ordering::Relaxed) {
            break 129;
        }
        reap_jobs(&mut job_control.jobs);
        let prompt = build_prompt();

        let line = match editor.readline(&prompt) {
            Ok(_line) if HANGUP_REQUESTED.load(Ordering::Relaxed) => break 129,
            Ok(line) => line,

            Err(ReadlineError::Interrupted) => {
                if HANGUP_REQUESTED.load(Ordering::Relaxed) {
                    break 129;
                }
                // Ctrl-C cancels the current input.
                last_status = 130;
                continue;
            }

            Err(ReadlineError::Eof) => {
                // Ctrl-D exits carli.
                println!();
                break status_to_u8(last_status);
            }

            Err(error) => {
                if HANGUP_REQUESTED.load(Ordering::Relaxed) {
                    break 129;
                }
                eprintln!("carli: could not read input: {error}");
                break 1;
            }
        };

        if !line.trim().is_empty()
            && let Err(error) = editor.add_history_entry(line.as_str())
        {
            eprintln!("carli: could not add history entry: {error}");
        }

        match execute_line(&line, last_status, job_control) {
            CommandOutcome::Status(status) => last_status = status,
            CommandOutcome::Exit(status) => break status,
        }
    };

    if let Some(path) = &history_path
        && let Err(error) = editor.save_history(path)
    {
        eprintln!("carli: could not save history: {error}");
    }

    exit_status
}

fn run_batch(job_control: &mut JobControl, initial_status: i32) -> u8 {
    let stdin = io::stdin();
    let mut last_status = initial_status;

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(line) => line,
            Err(error) => {
                eprintln!("carli: could not read input: {error}");
                return 1;
            }
        };
        match execute_line(&line, last_status, job_control) {
            CommandOutcome::Status(status) => last_status = status,
            CommandOutcome::Exit(status) => return status,
        }
    }

    status_to_u8(last_status)
}

fn is_builtin(name: &str) -> bool {
    matches!(
        name,
        "bg" | "cd" | "exit" | "export" | "fg" | "jobs" | "pwd" | "which"
    )
}

fn report_parse_error(error: ParseError) {
    eprintln!("carli: {error}");
}

fn execute_line(line: &str, last_status: i32, job_control: &mut JobControl) -> CommandOutcome {
    execute_line_from(line, last_status, job_control, None)
}

fn execute_line_from(
    line: &str,
    last_status: i32,
    job_control: &mut JobControl,
    source: Option<(&std::path::Path, usize)>,
) -> CommandOutcome {
    let parsed = match parse_command_line_with_status(line, last_status) {
        Ok(parsed) => parsed,
        Err(error) => {
            match source {
                Some((path, line_number)) => {
                    eprintln!("carli: {}:{line_number}: {error}", path.display())
                }
                None => report_parse_error(error),
            }
            return CommandOutcome::Status(2);
        }
    };
    if parsed.words.is_empty() {
        return CommandOutcome::Status(last_status);
    }

    let input = match parsed.input.as_deref().map(File::open).transpose() {
        Ok(input) => input,
        Err(error) => {
            let path = parsed.input.as_deref().unwrap_or_default();
            eprintln!("carli: {path}: {error}");
            return CommandOutcome::Status(1);
        }
    };
    let mut output = match parsed.output.as_ref().map(open_output).transpose() {
        Ok(output) => output,
        Err(error) => {
            let path = output_path(parsed.output.as_ref());
            eprintln!("carli: {path}: {error}");
            return CommandOutcome::Status(1);
        }
    };

    let status = match parsed.words[0].as_str() {
        "cd" => change_directory(&parsed.words[1..]),
        "exit" => {
            return match exit_status(&parsed.words[1..], last_status) {
                Ok(status) => CommandOutcome::Exit(status),
                Err(status) => CommandOutcome::Status(status),
            };
        }
        "export" => export_variable(&parsed.words[1..]),
        "pwd" => print_working_directory(&parsed.words[1..], output.as_mut()),
        "which" => which(&parsed.words[1..], output.as_mut()),
        "jobs" => list_jobs(&parsed.words[1..], &job_control.jobs, output.as_mut()),
        "fg" => foreground_job(&parsed.words[1..], job_control, output.as_mut()),
        "bg" => background_job(&parsed.words[1..], job_control, output.as_mut()),
        program => run_external(program, &parsed.words[1..], input, output, job_control),
    };
    CommandOutcome::Status(status)
}

fn outcome_status(outcome: CommandOutcome) -> u8 {
    match outcome {
        CommandOutcome::Status(status) => status_to_u8(status),
        CommandOutcome::Exit(status) => status,
    }
}

fn status_to_u8(status: i32) -> u8 {
    status.rem_euclid(256) as u8
}

fn export_variable(arguments: &[String]) -> i32 {
    if arguments.len() != 1 {
        eprintln!("carli: usage: export NAME=VALUE");
        return 1;
    }

    let assignment = &arguments[0];

    let Some((name, value)) = assignment.split_once('=') else {
        eprintln!("carli: export: expected NAME=VALUE");
        return 1;
    };

    if !is_valid_variable_name(name) {
        eprintln!("carli: export: `{name}` is not a valid variable name");
        return 1;
    }

    // TODO: carli is currently single-threaded, so no other
    // thread can concurrently read or modify the process
    // environment.
    unsafe {
        std::env::set_var(name, value);
    }

    0
}

fn is_valid_variable_name(name: &str) -> bool {
    let mut characters = name.chars();

    let Some(first) = characters.next() else {
        return false;
    };

    if first != '_' && !first.is_ascii_alphabetic() {
        return false;
    }

    characters.all(|character| character == '_' || character.is_ascii_alphanumeric())
}

fn which(arguments: &[String], output: Option<&mut File>) -> i32 {
    if arguments.is_empty() {
        eprintln!("carli: which: not enough arguments");
        return 1;
    }

    if arguments.len() > 1 {
        eprintln!("carli: which: too many arguments");
        return 1;
    }

    let command_name = &arguments[0];

    if is_builtin(command_name) {
        return write_output(output, &format!("{command_name}: carli built-in"));
    }

    let Some(path) = std::env::var_os("PATH") else {
        eprintln!("carli: which: PATH is not set");
        return 1;
    };

    for directory in std::env::split_paths(&path) {
        let candidate = directory.join(command_name);
        if candidate.is_file() {
            return write_output(output, &candidate.display().to_string());
        }
    }

    eprintln!("carli: which: {command_name} not found");
    1
}

fn change_directory(arguments: &[String]) -> i32 {
    if arguments.len() > 1 {
        eprintln!("carli: cd: too many arguments");
        eprintln!("carli: try cd <name_of_directory>");
        return 1;
    }
    let destination = arguments
        .first()
        .cloned()
        .or_else(|| std::env::var("HOME").ok());
    let Some(destination) = destination else {
        eprintln!("carli: cd: HOME is not set");
        return 1;
    };
    if let Err(error) = std::env::set_current_dir(&destination) {
        eprintln!("carli: cd: {destination}: {error}");
        return 1;
    }

    0
}

fn print_working_directory(arguments: &[String], output: Option<&mut File>) -> i32 {
    if !arguments.is_empty() {
        eprintln!("carli: pwd: too many arguments");
        return 1;
    }
    match std::env::current_dir() {
        Ok(path) => write_output(output, &path.display().to_string()),
        Err(error) => {
            eprintln!("carli: pwd: {error}");
            1
        }
    }
}

fn write_output(output: Option<&mut File>, line: &str) -> i32 {
    let result = match output {
        Some(file) => writeln!(file, "{line}"),
        None => writeln!(io::stdout(), "{line}"),
    };

    match result {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("carli: could not write output: {error}");
            1
        }
    }
}

fn list_jobs(arguments: &[String], jobs: &[Job], output: Option<&mut File>) -> i32 {
    if !arguments.is_empty() {
        eprintln!("carli: jobs: too many arguments");
        return 1;
    }

    let mut stdout = io::stdout();
    let writer: &mut dyn Write = match output {
        Some(file) => file,
        None => &mut stdout,
    };

    for job in jobs {
        let state = match job.state {
            JobState::Running => "Running",
            JobState::Stopped => "Stopped",
        };
        if let Err(error) = writeln!(writer, "[{}] {state:<7} {}", job.id, job.command) {
            eprintln!("carli: jobs: could not write output: {error}");
            return 1;
        }
    }

    0
}

fn foreground_job(
    arguments: &[String],
    job_control: &mut JobControl,
    output: Option<&mut File>,
) -> i32 {
    if !job_control.interactive {
        eprintln!("carli: fg: job control is unavailable without a terminal");
        return 1;
    }

    if let Err(error) = refresh_shell_terminal_modes(job_control) {
        eprintln!("carli: fg: could not save terminal modes: {error}");
        return 1;
    }

    let index = match find_job(arguments, &job_control.jobs, "fg") {
        Ok(index) => index,
        Err(status) => return status,
    };
    let mut job = job_control.jobs.remove(index);

    if write_output(output, &job.command) != 0 {
        job_control.jobs.push(job);
        return 1;
    }

    if let Err(error) = tcsetpgrp(io::stdin(), job.process_group) {
        eprintln!("carli: fg: could not give job the terminal: {error}");
        job_control.jobs.push(job);
        return 1;
    }

    if let Some(modes) = job.terminal_modes.as_ref()
        && let Err(error) = tcsetattr(io::stdin(), SetArg::TCSADRAIN, modes)
    {
        eprintln!("carli: fg: could not restore job terminal modes: {error}");
        let _ = reclaim_terminal(job_control);
        job_control.jobs.push(job);
        return 1;
    }

    if job.state == JobState::Stopped {
        if let Err(error) = killpg(job.process_group, Signal::SIGCONT) {
            eprintln!("carli: fg: could not continue job: {error}");
            let _ = reclaim_terminal(job_control);
            job_control.jobs.push(job);
            return 1;
        }
        job.state = JobState::Running;
    }

    let status = wait_for_foreground_job(job, &mut job_control.jobs, &mut job_control.next_job_id);
    if let Err(error) = reclaim_terminal(job_control) {
        eprintln!("carli: fg: could not restore the terminal: {error}");
        return 1;
    }
    status
}

fn background_job(
    arguments: &[String],
    job_control: &mut JobControl,
    output: Option<&mut File>,
) -> i32 {
    if !job_control.interactive {
        eprintln!("carli: bg: job control is unavailable without a terminal");
        return 1;
    }

    let index = match find_job(arguments, &job_control.jobs, "bg") {
        Ok(index) => index,
        Err(status) => return status,
    };
    let job = &mut job_control.jobs[index];

    if job.state == JobState::Running {
        eprintln!("carli: bg: job {} is already running", job.id);
        return 1;
    }
    if let Err(error) = killpg(job.process_group, Signal::SIGCONT) {
        eprintln!("carli: bg: could not continue job {}: {error}", job.id);
        return 1;
    }
    job.state = JobState::Running;
    write_output(output, &format!("[{}] {}", job.id, job.command))
}

fn find_job(arguments: &[String], jobs: &[Job], builtin: &str) -> Result<usize, i32> {
    if arguments.len() > 1 {
        eprintln!("carli: {builtin}: too many arguments");
        return Err(1);
    }
    if jobs.is_empty() {
        eprintln!("carli: {builtin}: no current job");
        return Err(1);
    }

    let requested_id = match arguments.first() {
        Some(value) => match value.strip_prefix('%').unwrap_or(value).parse::<usize>() {
            Ok(id) => id,
            Err(_) => {
                eprintln!("carli: {builtin}: {value}: invalid job identifier");
                return Err(1);
            }
        },
        None => jobs.iter().map(|job| job.id).max().unwrap_or_default(),
    };

    jobs.iter()
        .position(|job| job.id == requested_id)
        .ok_or_else(|| {
            eprintln!("carli: {builtin}: %{requested_id}: no such job");
            1
        })
}

fn open_output(redirection: &OutputRedirection) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.create(true).write(true);

    match redirection {
        OutputRedirection::Truncate(_) => options.truncate(true),
        OutputRedirection::Append(_) => options.append(true),
    };

    options.open(output_path(Some(redirection)))
}

fn output_path(redirection: Option<&OutputRedirection>) -> &str {
    match redirection {
        Some(OutputRedirection::Truncate(path) | OutputRedirection::Append(path)) => path,
        None => "",
    }
}

fn exit_status(arguments: &[String], last_status: i32) -> Result<u8, i32> {
    if arguments.len() > 1 {
        eprintln!("carli: exit: too many arguments");
        return Err(1);
    }

    match arguments.first() {
        Some(value) => match value.parse::<u8>() {
            Ok(status) => Ok(status),
            Err(_) => {
                eprintln!("carli: exit: {value}: numeric argument required");
                Ok(2)
            }
        },
        None => Ok(status_to_u8(last_status)),
    }
}

fn build_prompt() -> String {
    let template = std::env::var("CARLI_PROMPT").unwrap_or_else(|_| "carli $ ".to_string());

    let cwd = std::env::current_dir()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|_| "?".to_string());

    let directory = std::env::current_dir()
        .ok()
        .and_then(|path| path.file_name()?.to_str().map(str::to_owned))
        .unwrap_or_else(|| "/".to_string());

    let user = std::env::var("USER").unwrap_or_else(|_| "unknown".to_string());

    template
        .replace("{cwd}", &cwd)
        .replace("{dir}", &directory)
        .replace("{user}", &user)
        .replace("{shell}", "carli")
}

fn run_external(
    program: &str,
    arguments: &[String],
    input: Option<File>,
    output: Option<File>,
    job_control: &mut JobControl,
) -> i32 {
    if job_control.interactive
        && let Err(error) = refresh_shell_terminal_modes(job_control)
    {
        eprintln!("carli: {program}: could not save terminal modes: {error}");
        return 1;
    }

    let mut command = Command::new(program);
    command.args(arguments);
    if let Some(input) = input {
        command.stdin(Stdio::from(input));
    }
    if let Some(output) = output {
        command.stdout(Stdio::from(output));
    }

    if job_control.interactive {
        // SAFETY: Only async-signal-safe operations are performed between fork
        // and exec. The child creates its process group and restores the signal
        // dispositions expected by an ordinary foreground program.
        unsafe {
            command.pre_exec(|| {
                setpgid(Pid::from_raw(0), Pid::from_raw(0)).map_err(errno_to_io)?;
                for signal in child_signals() {
                    signal::signal(signal, SigHandler::SigDfl).map_err(errno_to_io)?;
                }
                Ok(())
            });
        }
    }

    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            eprintln!("carli: {program}: command not found");
            return 127;
        }
        Err(error) => {
            eprintln!("carli: {program}: {error}");
            return 126;
        }
    };

    if !job_control.interactive {
        return match child.wait() {
            Ok(status) => exit_status_from_process(status),
            Err(error) => {
                eprintln!("carli: {program}: could not wait for command: {error}");
                1
            }
        };
    }

    let child_pid = Pid::from_raw(child.id() as i32);
    if let Err(error) = setpgid(child_pid, child_pid)
        && error != Errno::EACCES
    {
        eprintln!("carli: {program}: could not create process group: {error}");
        let _ = killpg(child_pid, Signal::SIGKILL);
        let _ = child.wait();
        return 1;
    }

    if let Err(error) = tcsetpgrp(io::stdin(), child_pid) {
        eprintln!("carli: {program}: could not give command the terminal: {error}");
        let _ = killpg(child_pid, Signal::SIGKILL);
        let _ = child.wait();
        return 1;
    }

    let command_text = std::iter::once(program)
        .chain(arguments.iter().map(String::as_str))
        .collect::<Vec<_>>()
        .join(" ");
    let result = wait_for_foreground_job(
        Job {
            id: 0,
            pid: child_pid,
            process_group: child_pid,
            command: command_text,
            state: JobState::Running,
            terminal_modes: None,
        },
        &mut job_control.jobs,
        &mut job_control.next_job_id,
    );

    if let Err(error) = reclaim_terminal(job_control) {
        eprintln!("carli: {program}: could not restore the terminal: {error}");
        return 1;
    }

    result
}

fn setup_interactive_shell() -> io::Result<(Pid, Termios)> {
    loop {
        let process_group = getpgrp();
        let foreground_group = tcgetpgrp(io::stdin()).map_err(errno_to_io)?;
        if process_group == foreground_group {
            break;
        }

        // SAFETY: A shell started in the background must stop until its parent
        // moves it to the foreground. Resetting SIGTTIN first avoids spinning
        // if that signal was inherited as ignored.
        unsafe {
            signal::signal(Signal::SIGTTIN, SigHandler::SigDfl).map_err(errno_to_io)?;
        }
        killpg(process_group, Signal::SIGTTIN).map_err(errno_to_io)?;
    }

    for signal in foreground_signals() {
        // SAFETY: Ignoring these signals is the standard disposition for an
        // interactive shell, and rustyline temporarily installs its own
        // SIGINT handler while reading input.
        unsafe {
            signal::signal(signal, SigHandler::SigIgn).map_err(errno_to_io)?;
        }
    }

    let hangup_action = SigAction::new(
        SigHandler::Handler(request_hangup),
        SaFlags::empty(),
        SigSet::empty(),
    );
    // SAFETY: The handler only uses a lock-free atomic and async-signal-safe
    // raise. Omitting SA_RESTART ensures blocking terminal reads and waits
    // return to code that can perform orderly history and job cleanup.
    unsafe {
        signal::sigaction(Signal::SIGHUP, &hangup_action).map_err(errno_to_io)?;
    }

    let pid = getpid();
    match setpgid(pid, pid) {
        Ok(()) | Err(Errno::EPERM) | Err(Errno::EACCES) => {}
        Err(error) => return Err(errno_to_io(error)),
    }

    let process_group = getpgrp();
    tcsetpgrp(io::stdin(), process_group).map_err(errno_to_io)?;
    let terminal_modes = tcgetattr(io::stdin()).map_err(errno_to_io)?;
    Ok((process_group, terminal_modes))
}

fn refresh_shell_terminal_modes(job_control: &mut JobControl) -> io::Result<()> {
    job_control.shell_terminal_modes = Some(tcgetattr(io::stdin()).map_err(errno_to_io)?);
    Ok(())
}

fn reclaim_terminal(job_control: &JobControl) -> io::Result<()> {
    tcsetpgrp(io::stdin(), job_control.shell_process_group).map_err(errno_to_io)?;
    if let Some(modes) = job_control.shell_terminal_modes.as_ref() {
        tcsetattr(io::stdin(), SetArg::TCSADRAIN, modes).map_err(errno_to_io)?;
    }
    Ok(())
}

fn foreground_signals() -> [Signal; 5] {
    [
        Signal::SIGINT,
        Signal::SIGQUIT,
        Signal::SIGTSTP,
        Signal::SIGTTIN,
        Signal::SIGTTOU,
    ]
}

fn child_signals() -> [Signal; 6] {
    [
        Signal::SIGINT,
        Signal::SIGQUIT,
        Signal::SIGTSTP,
        Signal::SIGTTIN,
        Signal::SIGTTOU,
        Signal::SIGHUP,
    ]
}

fn wait_for_foreground_job(mut job: Job, jobs: &mut Vec<Job>, next_job_id: &mut usize) -> i32 {
    loop {
        match waitpid(job.pid, Some(WaitPidFlag::WUNTRACED)) {
            Ok(WaitStatus::Exited(_, status)) => return status,
            Ok(WaitStatus::Signaled(_, signal, _)) => return 128 + signal as i32,
            Ok(WaitStatus::Stopped(_, signal)) => {
                if job.id == 0 {
                    job.id = *next_job_id;
                    *next_job_id += 1;
                }
                job.state = JobState::Stopped;
                match tcgetattr(io::stdin()) {
                    Ok(modes) => job.terminal_modes = Some(modes),
                    Err(error) => {
                        eprintln!(
                            "carli: could not save terminal modes for {}: {error}",
                            job.command
                        )
                    }
                }
                eprintln!("[{}] Stopped {}", job.id, job.command);
                jobs.push(job);
                return 128 + signal as i32;
            }
            Ok(
                WaitStatus::Continued(_)
                | WaitStatus::PtraceEvent(_, _, _)
                | WaitStatus::PtraceSyscall(_)
                | WaitStatus::StillAlive,
            ) => {}
            Err(Errno::EINTR) => {
                if HANGUP_REQUESTED.load(Ordering::Relaxed) {
                    hang_up_job(&job);
                }
            }
            Err(error) => {
                eprintln!("carli: could not wait for {}: {error}", job.command);
                return 1;
            }
        }
    }
}

fn reap_jobs(jobs: &mut Vec<Job>) {
    jobs.retain_mut(|job| {
        match waitpid(
            job.pid,
            Some(WaitPidFlag::WNOHANG | WaitPidFlag::WUNTRACED | WaitPidFlag::WCONTINUED),
        ) {
            Ok(WaitStatus::Exited(_, status)) => {
                eprintln!("[{}] Done ({status}) {}", job.id, job.command);
                false
            }
            Ok(WaitStatus::Signaled(_, signal, _)) => {
                eprintln!("[{}] Terminated ({signal}) {}", job.id, job.command);
                false
            }
            Ok(WaitStatus::Stopped(_, _)) => {
                job.state = JobState::Stopped;
                true
            }
            Ok(WaitStatus::Continued(_)) => {
                job.state = JobState::Running;
                true
            }
            Err(Errno::ECHILD) => false,
            Err(error) => {
                eprintln!("carli: could not check stopped job {}: {error}", job.pid);
                true
            }
            _ => true,
        }
    });
}

fn hang_up_jobs(jobs: &[Job]) {
    for job in jobs {
        hang_up_job(job);
    }
}

fn hang_up_job(job: &Job) {
    let _ = killpg(job.process_group, Signal::SIGHUP);
    let _ = killpg(job.process_group, Signal::SIGCONT);
}

fn exit_status_from_process(status: std::process::ExitStatus) -> i32 {
    status
        .code()
        .or_else(|| status.signal().map(|signal| 128 + signal))
        .unwrap_or(1)
}

fn errno_to_io(error: Errno) -> io::Error {
    io::Error::from_raw_os_error(error as i32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_registry_contains_exactly_the_implemented_builtins() {
        for name in ["bg", "cd", "exit", "export", "fg", "jobs", "pwd", "which"] {
            assert!(is_builtin(name), "missing builtin: {name}");
        }
        for name in ["", "echo", "history", "BG", " cd"] {
            assert!(!is_builtin(name), "unexpected builtin: {name}");
        }
    }

    #[test]
    fn variable_name_validation_covers_boundaries() {
        for name in ["A", "z", "_", "_NAME_2", "Name123"] {
            assert!(is_valid_variable_name(name), "valid name: {name}");
        }
        for name in ["", "2NAME", "BAD-NAME", "HAS SPACE", "NÁME", "NAME="] {
            assert!(!is_valid_variable_name(name), "invalid name: {name}");
        }
    }

    #[test]
    fn internal_statuses_wrap_to_unix_exit_byte() {
        for (status, expected) in [
            (0, 0),
            (1, 1),
            (255, 255),
            (256, 0),
            (257, 1),
            (-1, 255),
            (-256, 0),
        ] {
            assert_eq!(status_to_u8(status), expected, "status: {status}");
        }
    }

    #[test]
    fn exit_status_uses_explicit_or_previous_status() {
        assert_eq!(exit_status(&[], 37), Ok(37));
        assert_eq!(exit_status(&[], 256), Ok(0));
        assert_eq!(exit_status(&["0".to_string()], 37), Ok(0));
        assert_eq!(exit_status(&["255".to_string()], 0), Ok(255));
        assert_eq!(exit_status(&["invalid".to_string()], 0), Ok(2));
        assert_eq!(exit_status(&["1".to_string(), "2".to_string()], 0), Err(1));
    }

    #[test]
    fn output_path_handles_both_redirection_modes() {
        assert_eq!(output_path(None), "");
        assert_eq!(
            output_path(Some(&OutputRedirection::Truncate("one".to_string()))),
            "one"
        );
        assert_eq!(
            output_path(Some(&OutputRedirection::Append("two".to_string()))),
            "two"
        );
    }

    #[test]
    fn foreground_signal_set_is_complete() {
        assert_eq!(
            foreground_signals(),
            [
                Signal::SIGINT,
                Signal::SIGQUIT,
                Signal::SIGTSTP,
                Signal::SIGTTIN,
                Signal::SIGTTOU,
            ]
        );
    }

    #[test]
    fn children_restore_hangup_alongside_terminal_signals() {
        assert_eq!(
            child_signals(),
            [
                Signal::SIGINT,
                Signal::SIGQUIT,
                Signal::SIGTSTP,
                Signal::SIGTTIN,
                Signal::SIGTTOU,
                Signal::SIGHUP,
            ]
        );
    }

    #[test]
    fn process_exit_status_maps_normal_and_signaled_children() {
        let normal = Command::new("sh").args(["-c", "exit 29"]).status().unwrap();
        assert_eq!(exit_status_from_process(normal), 29);

        let signaled = Command::new("sh")
            .args(["-c", "kill -TERM $$"])
            .status()
            .unwrap();
        assert_eq!(exit_status_from_process(signaled), 143);
    }
}
