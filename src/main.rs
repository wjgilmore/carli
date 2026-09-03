use rustyline::DefaultEditor;
use rustyline::error::ReadlineError;
use std::io;
use std::path::PathBuf;
use std::process::Command;

use carli::{ParseError, parse_line};

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

    let exit_status = loop {
        let prompt = build_prompt();

        let line = match editor.readline(&prompt) {
            Ok(line) => line,

            Err(ReadlineError::Interrupted) => {
                // Ctrl-C cancels the current input.
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

        let words = match parse_line(&line) {
            Ok(words) => words,
            Err(error) => {
                report_parse_error(error);
                continue;
            }
        };
        if words.is_empty() {
            continue;
        }

        match words[0].as_str() {
            "cd" => change_directory(&words[1..]),
            "exit" => {
                if let Some(status) = exit_status(&words[1..]) {
                    break status;
                }
            }
            "export" => export_variable(&words[1..]),
            "pwd" => print_working_directory(&words[1..]),
            "which" => which(&words[1..]),
            program => run_external(program, &words[1..]),
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

fn export_variable(arguments: &[String]) {
    if arguments.len() != 1 {
        eprintln!("carli: usage: export NAME=VALUE");
        return;
    }

    let assignment = &arguments[0];

    let Some((name, value)) = assignment.split_once('=') else {
        eprintln!("carli: export: expected NAME=VALUE");
        return;
    };

    if !is_valid_variable_name(name) {
        eprintln!("carli: export: `{name}` is not a valid variable name");
        return;
    }

    // TODO: carli is currently single-threaded, so no other
    // thread can concurrently read or modify the process
    // environment.
    unsafe {
        std::env::set_var(name, value);
    }
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

fn which(arguments: &[String]) {
    let command_name = &arguments[0];

    if is_builtin(command_name) {
        println!("{command_name}: carli built-in");
        return;
    }

    if arguments.is_empty() {
        eprintln!("carli: which: not enough arguments");
        return;
    }

    if arguments.len() > 1 {
        eprintln!("carli: which: too many arguments");
        return;
    }

    let Some(path) = std::env::var_os("PATH") else {
        eprintln!("carli: which: PATH is not set");
        return;
    };

    let command_name = &arguments[0];

    for directory in std::env::split_paths(&path) {
        let candidate = directory.join(command_name);
        if candidate.is_file() {
            println!("{}", candidate.display());
            return;
        }
    }

    eprintln!("carli: which: {command_name} not found");
}

fn change_directory(arguments: &[String]) {
    if arguments.len() > 1 {
        eprintln!("carli: cd: too many arguments");
        eprintln!("carli: try cd <name_of_directory>");
        return;
    }
    let destination = arguments
        .first()
        .cloned()
        .or_else(|| std::env::var("HOME").ok());
    let Some(destination) = destination else {
        eprintln!("carli: cd: HOME is not set");
        return;
    };
    if let Err(error) = std::env::set_current_dir(&destination) {
        eprintln!("carli: cd: {destination}: {error}");
    }
}

fn print_working_directory(arguments: &[String]) {
    if !arguments.is_empty() {
        eprintln!("carli: pwd: too many arguments");
        return;
    }
    match std::env::current_dir() {
        Ok(path) => println!("{}", path.display()),
        Err(error) => eprintln!("carli: pwd: {error}"),
    }
}

fn exit_status(arguments: &[String]) -> Option<u8> {
    if arguments.len() > 1 {
        eprintln!("carli: exit: too many arguments");
        return None;
    }

    match arguments.first() {
        Some(value) => match value.parse::<u8>() {
            Ok(status) => Some(status),
            Err(_) => {
                eprintln!("carli: exit: {value}: numeric argument required");
                Some(2)
            }
        },
        None => Some(0),
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

fn run_external(program: &str, arguments: &[String]) {
    match Command::new(program).args(arguments).status() {
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            eprintln!("carli: {program}: command not found")
        }
        Err(error) => eprintln!("carli: {program}: {error}"),
    }
}
