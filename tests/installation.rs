use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT_DIRECTORY: AtomicUsize = AtomicUsize::new(1);

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new(name: &str) -> Self {
        let number = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "carli-install-{name}-{}-{number}",
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

fn script(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("scripts")
        .join(name)
}

fn run_install(binary: &Path, destination: &Path, shells: &Path) -> Output {
    Command::new(script("install.sh"))
        .args(["--binary", binary.to_str().unwrap()])
        .args(["--destination", destination.to_str().unwrap()])
        .args(["--shells-file", shells.to_str().unwrap()])
        .output()
        .unwrap()
}

fn run_uninstall(destination: &Path, shells: &Path, passwd: &Path, remove: bool) -> Output {
    let mut command = Command::new(script("uninstall.sh"));
    command
        .args(["--destination", destination.to_str().unwrap()])
        .args(["--shells-file", shells.to_str().unwrap()])
        .args(["--passwd-file", passwd.to_str().unwrap()]);
    if remove {
        command.arg("--remove-binary");
    }
    command.output().unwrap()
}

#[test]
fn installer_copies_smoke_tests_and_registers_exactly_once() {
    let directory = TestDirectory::new("happy");
    let destination = directory.path().join("bin/carli");
    let shells = directory.path().join("shells");
    fs::create_dir(directory.path().join("bin")).unwrap();
    fs::write(
        &shells,
        format!(
            "/bin/sh\n{}\n{}\n",
            destination.display(),
            destination.display()
        ),
    )
    .unwrap();
    let mut shells_permissions = fs::metadata(&shells).unwrap().permissions();
    shells_permissions.set_mode(0o640);
    fs::set_permissions(&shells, shells_permissions).unwrap();

    for _ in 0..2 {
        let output = run_install(
            Path::new(env!("CARGO_BIN_EXE_carli")),
            &destination,
            &shells,
        );
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    assert!(destination.is_file());
    assert_eq!(
        fs::metadata(&destination).unwrap().permissions().mode() & 0o777,
        0o755
    );
    assert!(
        Command::new(&destination)
            .args(["-c", "exit 0"])
            .status()
            .unwrap()
            .success()
    );
    let entries = fs::read_to_string(&shells).unwrap();
    assert_eq!(
        entries
            .lines()
            .filter(|line| *line == destination.to_str().unwrap())
            .count(),
        1
    );
    assert!(entries.lines().any(|line| line == "/bin/sh"));
    assert_eq!(
        fs::metadata(&shells).unwrap().permissions().mode() & 0o777,
        0o640
    );
}

#[test]
fn uninstall_unregisters_before_optionally_removing_binary() {
    let directory = TestDirectory::new("uninstall");
    let destination = directory.path().join("carli");
    let shells = directory.path().join("shells");
    let passwd = directory.path().join("passwd");
    fs::copy(env!("CARGO_BIN_EXE_carli"), &destination).unwrap();
    fs::write(&shells, format!("/bin/sh\n{}\n", destination.display())).unwrap();
    fs::write(&passwd, "root:x:0:0:root:/root:/bin/sh\n").unwrap();

    let output = run_uninstall(&destination, &shells, &passwd, false);
    assert!(output.status.success());
    assert!(destination.exists());
    assert!(
        !fs::read_to_string(&shells)
            .unwrap()
            .contains(destination.to_str().unwrap())
    );

    fs::write(&shells, format!("/bin/sh\n{}\n", destination.display())).unwrap();
    let output = run_uninstall(&destination, &shells, &passwd, true);
    assert!(output.status.success());
    assert!(!destination.exists());
    assert_eq!(fs::read_to_string(shells).unwrap(), "/bin/sh\n");
}

#[test]
fn uninstall_refuses_shell_still_assigned_to_an_account() {
    let directory = TestDirectory::new("assigned");
    let destination = directory.path().join("carli");
    let shells = directory.path().join("shells");
    let passwd = directory.path().join("passwd");
    fs::copy(env!("CARGO_BIN_EXE_carli"), &destination).unwrap();
    fs::write(&shells, format!("/bin/sh\n{}\n", destination.display())).unwrap();
    fs::write(
        &passwd,
        format!("alice:x:1000:1000::/home/alice:{}\n", destination.display()),
    )
    .unwrap();

    let output = run_uninstall(&destination, &shells, &passwd, true);
    assert_eq!(output.status.code(), Some(1));
    assert!(destination.exists());
    assert!(
        fs::read_to_string(shells)
            .unwrap()
            .contains(destination.to_str().unwrap())
    );
}

#[test]
fn installer_rejects_unsafe_paths_without_changes() {
    let directory = TestDirectory::new("unsafe");
    let shells = directory.path().join("shells");
    fs::write(&shells, "/bin/sh\n").unwrap();
    let output = run_install(
        Path::new(env!("CARGO_BIN_EXE_carli")),
        Path::new("relative/carli"),
        &shells,
    );
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(fs::read_to_string(&shells).unwrap(), "/bin/sh\n");

    let link = directory.path().join("shells-link");
    symlink(&shells, &link).unwrap();
    let output = run_install(
        Path::new(env!("CARGO_BIN_EXE_carli")),
        &directory.path().join("carli"),
        &link,
    );
    assert_eq!(output.status.code(), Some(1));
    assert!(!directory.path().join("carli").exists());

    let destination_link = directory.path().join("destination-link");
    symlink(env!("CARGO_BIN_EXE_carli"), &destination_link).unwrap();
    let output = run_install(
        Path::new(env!("CARGO_BIN_EXE_carli")),
        &destination_link,
        &shells,
    );
    assert_eq!(output.status.code(), Some(1));
    assert!(destination_link.is_symlink());
}

#[test]
fn failed_install_preserves_existing_binary_and_registry() {
    let directory = TestDirectory::new("rollback");
    let destination = directory.path().join("carli");
    let shells = directory.path().join("shells");
    fs::write(&destination, "original").unwrap();
    fs::write(&shells, "/bin/sh\n").unwrap();

    let output = run_install(&directory.path().join("missing"), &destination, &shells);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(fs::read_to_string(destination).unwrap(), "original");
    assert_eq!(fs::read_to_string(shells).unwrap(), "/bin/sh\n");
}

#[test]
fn registry_commit_failure_rolls_back_existing_binary() {
    let directory = TestDirectory::new("transaction-rollback");
    let destination = directory.path().join("carli");
    let shells = directory.path().join("shells");
    let tools = directory.path().join("tools");
    let counter = directory.path().join("mv-count");
    fs::create_dir(&tools).unwrap();
    fs::write(&destination, "original-binary").unwrap();
    fs::write(&shells, "/bin/sh\n").unwrap();

    let fake_mv = tools.join("mv");
    fs::write(
        &fake_mv,
        format!(
            "#!/bin/sh\ncount=0\n[ ! -f {0} ] || count=$(cat {0})\ncount=$((count + 1))\nprintf '%s' \"$count\" > {0}\n[ \"$count\" -ne 2 ] || exit 1\nexec /usr/bin/mv \"$@\"\n",
            counter.display()
        ),
    )
    .unwrap();
    let mut permissions = fs::metadata(&fake_mv).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&fake_mv, permissions).unwrap();

    let output = Command::new(script("install.sh"))
        .args(["--binary", env!("CARGO_BIN_EXE_carli")])
        .args(["--destination", destination.to_str().unwrap()])
        .args(["--shells-file", shells.to_str().unwrap()])
        .env("PATH", format!("{}:/usr/bin:/bin", tools.display()))
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(fs::read_to_string(destination).unwrap(), "original-binary");
    assert_eq!(fs::read_to_string(shells).unwrap(), "/bin/sh\n");
}
