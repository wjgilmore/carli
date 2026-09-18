use std::fs;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT_DIRECTORY: AtomicUsize = AtomicUsize::new(1);

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new(name: &str) -> Self {
        let number = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("carli-{name}-{}-{number}", std::process::id()));
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

fn run_with_input(command: &mut Command, input: &str) -> Output {
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    std::io::Write::write_all(child.stdin.as_mut().unwrap(), input.as_bytes()).unwrap();
    child.wait_with_output().unwrap()
}

#[test]
fn command_mode_propagates_output_and_statuses() {
    let output = carli()
        .args(["-c", "/usr/bin/printf hello"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(output.stdout, b"hello");

    let output = carli().args(["-c", "sh -c \"exit 7\""]).output().unwrap();
    assert_eq!(output.status.code(), Some(7));

    let output = carli()
        .args(["-c", "carli-command-that-does-not-exist"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(127));
    assert!(String::from_utf8_lossy(&output.stderr).contains("command not found"));

    let output = carli()
        .args(["-c", "echo \"unterminated"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("unclosed double quote"));

    let output = carli().arg("-c").output().unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("usage: carli [-c COMMAND]"));
}

#[test]
fn redirection_reads_truncates_and_appends_real_files() {
    let directory = TestDirectory::new("redirection");
    let input = directory.path().join("input.txt");
    let output = directory.path().join("output.txt");
    fs::write(&input, "beta\nalpha\n").unwrap();

    let command = format!("sort < {} > {}", input.display(), output.display());
    assert!(carli().args(["-c", &command]).status().unwrap().success());
    assert_eq!(fs::read_to_string(&output).unwrap(), "alpha\nbeta\n");

    let command = format!("/usr/bin/printf gamma >> {}", output.display());
    assert!(carli().args(["-c", &command]).status().unwrap().success());
    assert_eq!(fs::read_to_string(&output).unwrap(), "alpha\nbeta\ngamma");

    let command = format!("pwd > {}", output.display());
    assert!(carli().args(["-c", &command]).status().unwrap().success());
    assert_eq!(
        fs::read_to_string(&output).unwrap().trim(),
        std::env::current_dir().unwrap().display().to_string()
    );
}

#[test]
fn batch_mode_preserves_state_and_returns_the_last_status() {
    let directory = TestDirectory::new("batch");
    let mut command = carli();
    command.env("HOME", directory.path());
    let output = run_with_input(
        &mut command,
        "export CARLI_BATCH_VALUE=preserved\n/usr/bin/printenv CARLI_BATCH_VALUE\ncd /tmp\npwd\nsh -c \"exit 9\"\n",
    );

    assert_eq!(output.status.code(), Some(9));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("preserved"));
    assert!(stdout.contains("/tmp"));
    assert!(!directory.path().join(".carli_history").exists());
}

#[test]
fn previous_status_and_builtin_failures_work_end_to_end() {
    let mut command = carli();
    let output = run_with_input(
        &mut command,
        "cd /definitely/not/a/real/path\n/usr/bin/printf status=$?\n",
    );

    assert!(output.status.success());
    assert_eq!(output.stdout, b"status=1");
    assert!(String::from_utf8_lossy(&output.stderr).contains("No such file or directory"));
}

#[test]
fn startup_configuration_uses_xdg_and_isolates_automation() {
    let directory = TestDirectory::new("startup");
    let home = directory.path().join("home");
    let xdg = directory.path().join("xdg");
    fs::create_dir_all(home.join(".config/carli")).unwrap();
    fs::create_dir_all(xdg.join("carli")).unwrap();
    fs::write(
        home.join(".config/carli/config"),
        "export STARTUP_VALUE=from-home\n",
    )
    .unwrap();
    fs::write(
        xdg.join("carli/config"),
        "# preferred config\nexport STARTUP_VALUE=from-xdg\nexport BROKEN=\"unterminated\nexport AFTER_ERROR=loaded\n",
    )
    .unwrap();

    let mut normal = carli();
    normal
        .args(["-c", "/usr/bin/printenv STARTUP_VALUE"])
        .env("HOME", &home)
        .env("XDG_CONFIG_HOME", &xdg);
    let output = normal.output().unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());

    let mut login = carli();
    login
        .arg0("-carli")
        .args(["-c", "/usr/bin/printenv STARTUP_VALUE"])
        .env("HOME", &home)
        .env("XDG_CONFIG_HOME", &xdg);
    let output = login.output().unwrap();
    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "from-xdg");
    assert!(String::from_utf8_lossy(&output.stderr).contains("config:3: unclosed double quote"));

    let mut continued = carli();
    continued
        .arg0("-carli")
        .args(["-c", "/usr/bin/printenv AFTER_ERROR"])
        .env("HOME", &home)
        .env("XDG_CONFIG_HOME", &xdg);
    assert_eq!(
        String::from_utf8_lossy(&continued.output().unwrap().stdout).trim(),
        "loaded"
    );
}

#[test]
fn login_shell_supplies_a_default_path_when_path_is_unset() {
    let directory = TestDirectory::new("path");
    let mut command = carli();
    command
        .arg0("-carli")
        .args(["-c", "which echo"])
        .env("HOME", directory.path())
        .env_remove("XDG_CONFIG_HOME")
        .env_remove("PATH");

    let output = command.output().unwrap();
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        "/usr/bin/echo"
    );
}
