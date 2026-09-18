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
            "carli-extra-{name}-{}-{number}",
            std::process::id()
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
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

fn executable(path: &Path, contents: &str) {
    fs::write(path, contents).unwrap();
    let mut permissions = fs::metadata(path).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).unwrap();
}

#[test]
fn cd_supports_home_and_reports_all_error_forms() {
    let directory = TestDirectory::new("cd");
    let output = run_batch("cd\npwd\n", directory.path());
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        directory.path().display().to_string()
    );

    let output = run_batch("cd /tmp extra\n", directory.path());
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("too many arguments"));

    let output = run_batch("cd /definitely/not/a/directory\n", directory.path());
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("No such file or directory"));

    let output = carli()
        .args(["-c", "cd"])
        .env_remove("HOME")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("HOME is not set"));
}

#[test]
fn pwd_validates_arguments_and_propagates_success() {
    let output = carli().args(["-c", "pwd"]).output().unwrap();
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        std::env::current_dir().unwrap().display().to_string()
    );

    let output = carli().args(["-c", "pwd extra"]).output().unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("too many arguments"));
}

#[test]
fn export_validates_names_and_reaches_child_processes() {
    let directory = TestDirectory::new("export");
    let output = run_batch(
        "export CARLI_EXPORTED=value\n/usr/bin/printenv CARLI_EXPORTED\n",
        directory.path(),
    );
    assert!(output.status.success());
    assert_eq!(output.stdout, b"value\n");

    for invalid in ["export", "export 2BAD=value", "export A=1 B=2"] {
        let output = carli().args(["-c", invalid]).output().unwrap();
        assert_eq!(output.status.code(), Some(1), "command: {invalid}");
        assert!(!output.stderr.is_empty(), "command: {invalid}");
    }
}

#[test]
fn which_handles_builtins_path_lookup_and_errors() {
    let directory = TestDirectory::new("which");
    let tool = directory.path().join("custom-tool");
    executable(&tool, "#!/bin/sh\nexit 0\n");

    let output = carli()
        .args(["-c", "which custom-tool"])
        .env("PATH", directory.path())
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        tool.display().to_string()
    );

    let output = carli().args(["-c", "which cd"]).output().unwrap();
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("carli built-in"));

    for invalid in ["which", "which one two", "which definitely-missing"] {
        let output = carli().args(["-c", invalid]).output().unwrap();
        assert_eq!(output.status.code(), Some(1), "command: {invalid}");
    }

    let output = carli()
        .args(["-c", "which external"])
        .env_remove("PATH")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("PATH is not set"));
}

#[test]
fn external_commands_use_path_wait_and_return_their_status() {
    let directory = TestDirectory::new("external");
    let tool = directory.path().join("status-tool");
    executable(&tool, "#!/bin/sh\nprintf custom-output\nexit 23\n");

    let output = carli()
        .args(["-c", "status-tool"])
        .env("PATH", directory.path())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(23));
    assert_eq!(output.stdout, b"custom-output");
}

#[test]
fn exit_covers_explicit_inherited_invalid_and_too_many_statuses() {
    assert_eq!(
        carli().args(["-c", "exit 42"]).status().unwrap().code(),
        Some(42)
    );
    assert_eq!(
        carli()
            .args(["-c", "exit invalid"])
            .output()
            .unwrap()
            .status
            .code(),
        Some(2)
    );

    let directory = TestDirectory::new("exit");
    let output = run_batch("sh -c \"exit 7\"\nexit\n", directory.path());
    assert_eq!(output.status.code(), Some(7));

    let output = run_batch("exit 1 2\n", directory.path());
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("too many arguments"));
}

#[test]
fn blank_input_eof_and_errors_have_stable_statuses() {
    let directory = TestDirectory::new("empty");
    assert!(run_batch("", directory.path()).status.success());
    assert!(run_batch("\n\n", directory.path()).status.success());

    let output = run_batch("sh -c \"exit 6\"\n\n", directory.path());
    assert_eq!(output.status.code(), Some(6));

    let output = run_batch("echo \"unterminated\n", directory.path());
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn redirection_reports_missing_files_targets_duplicates_and_open_failures() {
    let directory = TestDirectory::new("redirection-errors");
    let missing = format!("cat < {}/missing", directory.path().display());
    let output = carli().args(["-c", &missing]).output().unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("No such file or directory"));

    for invalid in ["echo hello >", "cat < one < two", "echo hi > one >> two"] {
        let output = carli().args(["-c", invalid]).output().unwrap();
        assert_eq!(output.status.code(), Some(2), "command: {invalid}");
    }

    let invalid_output = format!("pwd > {}/missing/file", directory.path().display());
    let output = carli().args(["-c", &invalid_output]).output().unwrap();
    assert_eq!(output.status.code(), Some(1));
}

#[test]
fn home_startup_fallback_status_and_exit_are_honored() {
    let directory = TestDirectory::new("home-startup");
    let config_directory = directory.path().join(".config/carli");
    fs::create_dir_all(&config_directory).unwrap();
    let config = config_directory.join("config");

    fs::write(&config, "export HOME_CONFIG=loaded\n").unwrap();
    let output = carli()
        .arg0("-carli")
        .args(["-c", "/usr/bin/printenv HOME_CONFIG"])
        .env("HOME", directory.path())
        .env_remove("XDG_CONFIG_HOME")
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(output.stdout, b"loaded\n");

    fs::write(&config, "export BROKEN=\"unterminated\n").unwrap();
    let output = carli()
        .arg0("-carli")
        .args(["-c", "/usr/bin/printf $?"])
        .env("HOME", directory.path())
        .env_remove("XDG_CONFIG_HOME")
        .output()
        .unwrap();
    assert_eq!(output.stdout, b"2");

    fs::write(&config, "exit 31\n").unwrap();
    let output = carli()
        .arg0("-carli")
        .args(["-c", "/usr/bin/printf should-not-run"])
        .env("HOME", directory.path())
        .env_remove("XDG_CONFIG_HOME")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(31));
    assert!(output.stdout.is_empty());
}
