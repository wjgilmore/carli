use std::io::{self, Write};
use std::process::{self, Command};

use carli::{ParseError, parse_line};

fn main() {
    let stdin = io::stdin();
    loop {
        print!("carli$ ");
        if let Err(error) = io::stdout().flush() {
            eprintln!("carli: could not write prompt: {error}");
            process::exit(1);
        }

        let mut line = String::new();
        match stdin.read_line(&mut line) {
            Ok(0) => {
                println!();
                break;
            }
            Ok(_) => {}
            Err(error) => {
                eprintln!("carli: could not read input: {error}");
                continue;
            }
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
            "pwd" => print_working_directory(&words[1..]),
            "which" => which(&words[1..]),
            "exit" => exit_shell(&words[1..]),
            program => run_external(program, &words[1..]),
        }
    }
}

fn is_builtin(name: &str) -> bool {
    matches!(name, "cd" | "pwd" | "which" | "exit")
}

fn report_parse_error(error: ParseError) {
    eprintln!("carli: {error}");
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

fn exit_shell(arguments: &[String]) -> ! {
    if arguments.len() > 1 {
        eprintln!("carli: exit: too many arguments");
        process::exit(2);
    }
    let status = match arguments.first() {
        Some(value) => match value.parse::<u8>() {
            Ok(status) => i32::from(status),
            Err(_) => {
                eprintln!("carli: exit: {value}: numeric argument required");
                2
            }
        },
        None => 0,
    };
    process::exit(status);
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
