use rustyline::DefaultEditor;
use rustyline::error::ReadlineError;
use std::io;
use std::os::unix::process::ExitStatusExt;
use std::path::PathBuf;
use std::process::Command;

use carli::{ParseError, parse_line_with_status};

fn history_path() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .map(|home| home.join(".carli_history"))
}

fn main() -> std::process::ExitCode {
    let mut editor = DefaultEditor::new().expect("carli: could not initialize line editor");

    let history_path = history_path();

    if let Some(path) = &history_path
        && path.exists()
        && let Err(error) = editor.load_history(path)
    {
        eprintln!("carli: could not load history: {error}");
    }

    let mut last_status = 0;

    let exit_status = loop {
        let prompt = build_prompt();

        let line = match editor.readline(&prompt) {
            Ok(line) => line,

            Err(ReadlineError::Interrupted) => {
                // Ctrl-C cancels the current input.
                last_status = 130;
                continue;
            }

            Err(ReadlineError::Eof) => {
                // Ctrl-D exits carli.
                println!();
                break 0;
            }

            Err(error) => {
                eprintln!("carli: could not read input: {error}");
                break 1;
            }
        };

        if !line.trim().is_empty()
            && let Err(error) = editor.add_history_entry(line.as_str())
        {
            eprintln!("carli: could not add history entry: {error}");
        }

        let words = match parse_line_with_status(&line, last_status) {
            Ok(words) => words,
            Err(error) => {
                report_parse_error(error);
                last_status = 2;
                continue;
            }
        };
        if words.is_empty() {
            continue;
        }

        match words[0].as_str() {
            "cd" => last_status = change_directory(&words[1..]),
            "exit" => match exit_status(&words[1..]) {
                Ok(status) => break status,
                Err(status) => {
                    last_status = status;
                }
            },
            "export" => last_status = export_variable(&words[1..]),
            "pwd" => last_status = print_working_directory(&words[1..]),
            "which" => last_status = which(&words[1..]),
            program => last_status = run_external(program, &words[1..]),
        }
    };

    if let Some(path) = &history_path
        && let Err(error) = editor.save_history(path)
    {
        eprintln!("carli: could not save history: {error}");
    }

    std::process::ExitCode::from(exit_status)
}

fn is_builtin(name: &str) -> bool {
    matches!(name, "cd" | "export" | "pwd" | "which" | "exit")
}

fn report_parse_error(error: ParseError) {
    eprintln!("carli: {error}");
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

fn which(arguments: &[String]) -> i32 {
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
        println!("{command_name}: carli built-in");
        return 0;
    }

    let Some(path) = std::env::var_os("PATH") else {
        eprintln!("carli: which: PATH is not set");
        return 1;
    };

    for directory in std::env::split_paths(&path) {
        let candidate = directory.join(command_name);
        if candidate.is_file() {
            println!("{}", candidate.display());
            return 0;
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

fn print_working_directory(arguments: &[String]) -> i32 {
    if !arguments.is_empty() {
        eprintln!("carli: pwd: too many arguments");
        return 1;
    }
    match std::env::current_dir() {
        Ok(path) => {
            println!("{}", path.display());
            0
        }
        Err(error) => {
            eprintln!("carli: pwd: {error}");
            1
        }
    }
}

fn exit_status(arguments: &[String]) -> Result<u8, i32> {
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
        None => Ok(0),
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

fn run_external(program: &str, arguments: &[String]) -> i32 {
    match Command::new(program).args(arguments).status() {
        Ok(status) => status
            .code()
            .or_else(|| status.signal().map(|signal| 128 + signal))
            .unwrap_or(1),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            eprintln!("carli: {program}: command not found");
            127
        }
        Err(error) => {
            eprintln!("carli: {program}: {error}");
            126
        }
    }
}
