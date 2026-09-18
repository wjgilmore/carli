use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT_DIRECTORY: AtomicUsize = AtomicUsize::new(1);

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new(name: &str) -> Self {
        let number = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "carli-matrix-{name}-{}-{number}",
            std::process::id()
        ));
        fs::create_dir_all(&path).unwrap();
        Self(fs::canonicalize(path).unwrap())
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

fn carli() -> Command {
    Command::new(env!("CARGO_BIN_EXE_carli"))
}

fn external_program(name: &str) -> PathBuf {
    std::env::split_paths(&std::env::var_os("PATH").expect("tests require PATH"))
        .map(|directory| directory.join(name))
        .find(|path| path.is_file())
        .unwrap_or_else(|| panic!("tests require {name} on PATH"))
}

fn run_batch(input: &str, home: &Path) -> Output {
    let mut child = carli()
        .env("HOME", home)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    std::io::Write::write_all(child.stdin.as_mut().unwrap(), input.as_bytes()).unwrap();
    child.wait_with_output().unwrap()
}

fn make_executable(path: &Path, body: &str) {
    fs::write(path, body).unwrap();
    let mut permissions = fs::metadata(path).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).unwrap();
}

#[test]
fn empty_c_command_succeeds() {
    let output = carli().args(["-c", ""]).output().unwrap();
    assert!(output.status.success());
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}

#[test]
fn unsupported_invocation_argument_is_usage_error() {
    let output = carli().arg("--unknown").output().unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("unsupported argument `--unknown`"));
}

#[test]
fn arguments_after_c_command_are_rejected() {
    let output = carli().args(["-c", "pwd", "extra"]).output().unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("unsupported argument `-c`"));
}

#[test]
fn command_mode_does_not_treat_semicolon_as_a_separator() {
    let output = carli()
        .args(["-c", "printf '%s' 'one;two'"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(output.stdout, b"one;two");
}

#[test]
fn batch_eof_returns_success_after_only_comments_and_blank_lines() {
    let directory = TestDirectory::new("blank-comments");
    let output = run_batch("\n# comment\n   \n", directory.path());
    assert!(output.status.success());
    assert!(output.stdout.is_empty());
}

#[test]
fn batch_continues_after_parse_and_command_errors() {
    let directory = TestDirectory::new("continue-errors");
    let output = run_batch(
        "echo \"unterminated\nmissing-carli-command\nprintf survived\n",
        directory.path(),
    );
    assert!(output.status.success());
    assert_eq!(output.stdout, b"survived");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("unclosed double quote"));
    assert!(stderr.contains("command not found"));
}

#[test]
fn batch_status_tracks_each_success_and_failure() {
    let directory = TestDirectory::new("status-sequence");
    let output = run_batch(
        "sh -c \"exit 19\"\nprintf '%s ' $?\ntrue\nprintf '%s' $?\n",
        directory.path(),
    );
    assert!(output.status.success());
    assert_eq!(output.stdout, b"19 0");
}

#[test]
fn export_supports_empty_values_and_expansion() {
    let directory = TestDirectory::new("empty-export");
    let output = run_batch(
        "export CARLI_EMPTY=\nprintf '<%s>' \"$CARLI_EMPTY\"\n",
        directory.path(),
    );
    assert!(output.status.success());
    assert_eq!(output.stdout, b"<>");
}

#[test]
fn export_value_preserves_spaces_quotes_and_equals() {
    let directory = TestDirectory::new("complex-export");
    let output = run_batch(
        "export CARLI_COMPLEX=\"hello world=again\"\nprintenv CARLI_COMPLEX\n",
        directory.path(),
    );
    assert!(output.status.success());
    assert_eq!(output.stdout, b"hello world=again\n");
}

#[test]
fn which_returns_first_path_match() {
    let directory = TestDirectory::new("first-path");
    let first = directory.path().join("first");
    let second = directory.path().join("second");
    fs::create_dir_all(&first).unwrap();
    fs::create_dir_all(&second).unwrap();
    make_executable(&first.join("same-tool"), "#!/bin/sh\nexit 0\n");
    make_executable(&second.join("same-tool"), "#!/bin/sh\nexit 0\n");
    let path = std::env::join_paths([&first, &second]).unwrap();

    let output = carli()
        .args(["-c", "which same-tool"])
        .env("PATH", path)
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        first.join("same-tool").display().to_string()
    );
}

#[test]
fn which_ignores_non_files_in_path() {
    let directory = TestDirectory::new("path-directory");
    fs::create_dir(directory.path().join("directory-tool")).unwrap();
    let output = carli()
        .args(["-c", "which directory-tool"])
        .env("PATH", directory.path())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("not found"));
}

#[test]
fn every_builtin_is_reported_by_which() {
    for builtin in ["bg", "cd", "exit", "export", "fg", "jobs", "pwd", "which"] {
        let output = carli()
            .args(["-c", &format!("which {builtin}")])
            .output()
            .unwrap();
        assert!(output.status.success(), "builtin: {builtin}");
        assert_eq!(
            String::from_utf8_lossy(&output.stdout).trim(),
            format!("{builtin}: carli built-in")
        );
    }
}

#[test]
fn explicit_relative_external_path_executes() {
    let directory = TestDirectory::new("relative-command");
    let tool = directory.path().join("relative-tool");
    make_executable(&tool, "#!/bin/sh\nprintf relative\n");
    let output = carli()
        .current_dir(directory.path())
        .args(["-c", "./relative-tool"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(output.stdout, b"relative");
}

#[test]
fn external_arguments_preserve_empty_and_spaced_words() {
    let output = carli()
        .args(["-c", "printf '<%s><%s>' '' 'two words'"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(output.stdout, b"<><two words>");
}

#[test]
fn input_redirection_prevents_command_execution_when_open_fails() {
    let directory = TestDirectory::new("input-before-exec");
    let marker = directory.path().join("marker");
    let missing = directory.path().join("missing");
    let command = format!(
        "sh -c \"touch {}\" < {}",
        marker.display(),
        missing.display()
    );
    let output = carli().args(["-c", &command]).output().unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(!marker.exists());
}

#[test]
fn output_redirection_is_opened_before_builtin_validation() {
    let directory = TestDirectory::new("output-before-builtin");
    let output_path = directory.path().join("created");
    let command = format!("pwd extra > {}", output_path.display());
    let output = carli().args(["-c", &command]).output().unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(output_path.exists());
    assert!(fs::read(output_path).unwrap().is_empty());
}

#[test]
fn append_redirection_creates_a_missing_file() {
    let directory = TestDirectory::new("append-create");
    let output_path = directory.path().join("new-file");
    let command = format!("printf created >> {}", output_path.display());
    assert!(carli().args(["-c", &command]).status().unwrap().success());
    assert_eq!(fs::read(output_path).unwrap(), b"created");
}

#[test]
fn redirection_paths_expand_variables_and_previous_status() {
    let directory = TestDirectory::new("expanded-paths");
    let output = run_batch(
        &format!(
            "export CARLI_OUT={}/result\nsh -c \"exit 7\"\nprintf expanded > $CARLI_OUT$?\n",
            directory.path().display()
        ),
        directory.path(),
    );
    assert!(output.status.success());
    assert_eq!(
        fs::read(directory.path().join("result7")).unwrap(),
        b"expanded"
    );
}

#[test]
fn quoted_and_escaped_redirection_operators_reach_external_command() {
    let output = carli()
        .args(["-c", "printf '%s%s%s' '>' \"<\" \\>"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(output.stdout, b"><>");
}

#[test]
fn fg_and_bg_are_unavailable_in_command_mode() {
    for builtin in ["fg", "bg"] {
        let output = carli().args(["-c", builtin]).output().unwrap();
        assert_eq!(output.status.code(), Some(1), "builtin: {builtin}");
        assert!(
            String::from_utf8_lossy(&output.stderr)
                .contains("job control is unavailable without a terminal")
        );
    }
}

#[test]
fn jobs_without_a_terminal_is_an_empty_successful_listing() {
    let output = carli().args(["-c", "jobs"]).output().unwrap();
    assert!(output.status.success());
    assert!(output.stdout.is_empty());
}

#[test]
fn login_shell_preserves_an_explicitly_empty_path() {
    let directory = TestDirectory::new("empty-path");
    let command = format!("{} PATH", external_program("printenv").display());
    let output = carli()
        .arg0("-carli")
        .args(["-c", &command])
        .env("HOME", directory.path())
        .env("PATH", "")
        .env("CARLI_SYSTEM_CONFIG", directory.path().join("missing"))
        .env_remove("XDG_CONFIG_HOME")
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(output.stdout, b"\n");
}

#[test]
fn empty_xdg_config_home_uses_home_fallback() {
    let directory = TestDirectory::new("empty-xdg");
    let config_directory = directory.path().join(".config/carli");
    fs::create_dir_all(&config_directory).unwrap();
    fs::write(
        config_directory.join("config"),
        "export EMPTY_XDG_FALLBACK=loaded\n",
    )
    .unwrap();
    let output = carli()
        .arg0("-carli")
        .args(["-c", "printenv EMPTY_XDG_FALLBACK"])
        .env("HOME", directory.path())
        .env("XDG_CONFIG_HOME", "")
        .env("CARLI_SYSTEM_CONFIG", directory.path().join("missing"))
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(output.stdout, b"loaded\n");
}

#[test]
fn startup_files_can_change_directory_run_commands_and_redirect() {
    let directory = TestDirectory::new("startup-actions");
    let destination = directory.path().join("destination");
    let redirected = directory.path().join("startup-output");
    let config = directory.path().join("system-config");
    fs::create_dir(&destination).unwrap();
    fs::write(
        &config,
        format!(
            "cd {}\nprintf startup > {}\n",
            destination.display(),
            redirected.display()
        ),
    )
    .unwrap();
    let output = carli()
        .arg0("-carli")
        .args(["-c", "pwd"])
        .env("HOME", directory.path())
        .env("CARLI_SYSTEM_CONFIG", &config)
        .env_remove("XDG_CONFIG_HOME")
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        fs::canonicalize(&destination)
            .unwrap()
            .display()
            .to_string()
    );
    assert_eq!(fs::read(redirected).unwrap(), b"startup");
}

#[test]
fn user_startup_observes_system_startup_status() {
    let directory = TestDirectory::new("startup-status-order");
    let xdg = directory.path().join("xdg/carli");
    let system = directory.path().join("system-config");
    fs::create_dir_all(&xdg).unwrap();
    fs::write(&system, "sh -c \"exit 37\"\n").unwrap();
    fs::write(xdg.join("config"), "export STATUS_FROM_SYSTEM=$?\n").unwrap();
    let output = carli()
        .arg0("-carli")
        .args(["-c", "printenv STATUS_FROM_SYSTEM"])
        .env("HOME", directory.path())
        .env("XDG_CONFIG_HOME", directory.path().join("xdg"))
        .env("CARLI_SYSTEM_CONFIG", &system)
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(output.stdout, b"37\n");
}

#[test]
fn missing_startup_files_are_silent() {
    let directory = TestDirectory::new("missing-startup");
    let output = carli()
        .arg0("-carli")
        .args(["-c", "printf ok"])
        .env("HOME", directory.path())
        .env(
            "CARLI_SYSTEM_CONFIG",
            directory.path().join("missing-system"),
        )
        .env_remove("XDG_CONFIG_HOME")
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(output.stdout, b"ok");
    assert!(output.stderr.is_empty());
}

#[test]
fn startup_exit_without_argument_uses_previous_status() {
    let directory = TestDirectory::new("startup-exit-status");
    let config = directory.path().join("system-config");
    fs::write(&config, "sh -c \"exit 44\"\nexit\n").unwrap();
    let output = carli()
        .arg0("-carli")
        .args(["-c", "printf should-not-run"])
        .env("HOME", directory.path())
        .env("CARLI_SYSTEM_CONFIG", config)
        .env_remove("XDG_CONFIG_HOME")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(44));
    assert!(output.stdout.is_empty());
}

#[test]
fn exit_accepts_zero_and_leading_zeroes() {
    for (value, expected) in [("0", 0), ("00", 0), ("007", 7), ("+1", 1)] {
        let output = carli()
            .args(["-c", &format!("exit {value}")])
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(expected), "value: {value}");
    }
}

#[test]
fn exit_rejects_negative_and_non_integer_values() {
    for value in ["-1", "1.0"] {
        let output = carli()
            .args(["-c", &format!("exit {value}")])
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2), "value: {value}");
        assert!(String::from_utf8_lossy(&output.stderr).contains("numeric argument required"));
    }
}

#[test]
fn cd_accepts_relative_and_quoted_paths() {
    let directory = TestDirectory::new("relative-cd");
    let child = directory.path().join("directory with spaces");
    fs::create_dir(&child).unwrap();
    let output = run_batch(
        &format!(
            "cd {}\ncd 'directory with spaces'\npwd\n",
            directory.path().display()
        ),
        directory.path(),
    );
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        fs::canonicalize(&child).unwrap().display().to_string()
    );
}

#[test]
#[cfg(target_os = "linux")]
fn builtin_write_failures_return_status_one() {
    for command in ["pwd > /dev/full", "which pwd > /dev/full"] {
        let output = carli().args(["-c", command]).output().unwrap();
        assert_eq!(output.status.code(), Some(1), "command: {command}");
        assert!(String::from_utf8_lossy(&output.stderr).contains("could not write output"));
    }
}

#[test]
fn output_redirection_rejects_a_directory_target() {
    let directory = TestDirectory::new("directory-output");
    let command = format!("pwd > {}", directory.path().display());
    let output = carli().args(["-c", &command]).output().unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("Is a directory"));
}

#[test]
fn startup_read_error_sets_status_and_user_file_still_runs() {
    let directory = TestDirectory::new("startup-read-error");
    let system_directory = directory.path().join("system-directory");
    let xdg = directory.path().join("xdg/carli");
    fs::create_dir(&system_directory).unwrap();
    fs::create_dir_all(&xdg).unwrap();
    fs::write(xdg.join("config"), "export SYSTEM_READ_STATUS=$?\n").unwrap();
    let output = carli()
        .arg0("-carli")
        .args(["-c", "printenv SYSTEM_READ_STATUS"])
        .env("HOME", directory.path())
        .env("XDG_CONFIG_HOME", directory.path().join("xdg"))
        .env("CARLI_SYSTEM_CONFIG", &system_directory)
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(output.stdout, b"1\n");
    assert!(String::from_utf8_lossy(&output.stderr).contains("could not read startup file"));
}
