use std::fs;
use std::os::unix::fs::{MetadataExt, PermissionsExt, symlink};
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

fn make_executable(path: &Path, contents: &str) {
    fs::write(path, contents).unwrap();
    let mut permissions = fs::metadata(path).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).unwrap();
}

fn system_mv() -> &'static str {
    ["/usr/bin/mv", "/bin/mv"]
        .into_iter()
        .find(|path| Path::new(path).is_file())
        .expect("tests require the system mv utility")
}

fn system_path_with(tools: &Path) -> String {
    format!("{}:/usr/bin:/bin:/usr/sbin:/sbin", tools.display())
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
            "#!/bin/sh\ncount=0\n[ ! -f {0} ] || count=$(cat {0})\ncount=$((count + 1))\nprintf '%s' \"$count\" > {0}\n[ \"$count\" -ne 2 ] || exit 1\nexec {1} \"$@\"\n",
            counter.display(),
            system_mv()
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
        .env("PATH", system_path_with(&tools))
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(fs::read_to_string(destination).unwrap(), "original-binary");
    assert_eq!(fs::read_to_string(shells).unwrap(), "/bin/sh\n");
}

#[test]
fn script_help_succeeds_and_invalid_cli_is_usage_error() {
    for name in ["install.sh", "uninstall.sh"] {
        let help = Command::new(script(name)).arg("--help").output().unwrap();
        assert!(help.status.success(), "script: {name}");
        assert!(String::from_utf8_lossy(&help.stderr).contains("usage:"));

        for arguments in [vec!["--unknown"], vec!["--destination"]] {
            let output = Command::new(script(name)).args(arguments).output().unwrap();
            assert_eq!(output.status.code(), Some(2), "script: {name}");
            assert!(String::from_utf8_lossy(&output.stderr).contains("usage:"));
        }
    }
}

#[test]
fn installer_rejects_non_executable_and_failed_smoke_test_binaries() {
    let directory = TestDirectory::new("bad-binaries");
    let destination = directory.path().join("carli");
    let shells = directory.path().join("shells");
    let non_executable = directory.path().join("non-executable");
    let failing = directory.path().join("failing");
    fs::write(&shells, "/bin/sh\n").unwrap();
    fs::write(&non_executable, "not executable").unwrap();

    let output = run_install(&non_executable, &destination, &shells);
    assert_eq!(output.status.code(), Some(1));
    assert!(!destination.exists());
    assert_eq!(fs::read_to_string(&shells).unwrap(), "/bin/sh\n");

    make_executable(&failing, "#!/bin/sh\nexit 19\n");
    let output = run_install(&failing, &destination, &shells);
    assert_eq!(output.status.code(), Some(19));
    assert!(!destination.exists());
    assert_eq!(fs::read_to_string(shells).unwrap(), "/bin/sh\n");
}

#[test]
fn installer_rejects_missing_and_non_regular_targets() {
    let directory = TestDirectory::new("missing-targets");
    let shells = directory.path().join("shells");
    fs::write(&shells, "/bin/sh\n").unwrap();

    let missing_parent = directory.path().join("missing/carli");
    let output = run_install(
        Path::new(env!("CARGO_BIN_EXE_carli")),
        &missing_parent,
        &shells,
    );
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(fs::read_to_string(&shells).unwrap(), "/bin/sh\n");

    let destination_directory = directory.path().join("destination-directory");
    fs::create_dir(&destination_directory).unwrap();
    let output = run_install(
        Path::new(env!("CARGO_BIN_EXE_carli")),
        &destination_directory,
        &shells,
    );
    assert_eq!(output.status.code(), Some(1));

    let output = run_install(
        Path::new(env!("CARGO_BIN_EXE_carli")),
        &directory.path().join("carli"),
        &directory.path().join("missing-shells"),
    );
    assert_eq!(output.status.code(), Some(1));
    assert!(!directory.path().join("carli").exists());
}

#[test]
fn installer_canonicalizes_paths_with_spaces_and_repairs_missing_newline() {
    let directory = TestDirectory::new("canonical-spaces");
    let parent = directory.path().join("directory with spaces");
    let bin = parent.join("bin");
    fs::create_dir_all(&bin).unwrap();
    let destination = bin.join("..").join("bin/carli");
    let canonical_destination = bin.join("carli");
    let shells = parent.join("shell registry");
    fs::write(&shells, "/bin/sh").unwrap();

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
    assert!(canonical_destination.exists());
    assert_eq!(
        fs::read_to_string(shells).unwrap(),
        format!("/bin/sh\n{}\n", canonical_destination.display())
    );
}

#[test]
fn installer_rejects_newlines_in_paths_without_changes() {
    let directory = TestDirectory::new("newline");
    let shells = directory.path().join("shells");
    fs::write(&shells, "/bin/sh\n").unwrap();
    let destination = directory.path().join("carli\ninjected");
    let output = run_install(
        Path::new(env!("CARGO_BIN_EXE_carli")),
        &destination,
        &shells,
    );
    assert_eq!(output.status.code(), Some(2));
    assert!(!destination.exists());
    assert_eq!(fs::read_to_string(shells).unwrap(), "/bin/sh\n");
}

#[test]
fn uninstall_is_idempotent_removes_duplicates_and_preserves_mode() {
    let directory = TestDirectory::new("uninstall-idempotent");
    let destination = directory.path().join("carli");
    let shells = directory.path().join("shells");
    let passwd = directory.path().join("passwd");
    fs::copy(env!("CARGO_BIN_EXE_carli"), &destination).unwrap();
    fs::write(
        &shells,
        format!(
            "/bin/sh\n{}\n{}",
            destination.display(),
            destination.display()
        ),
    )
    .unwrap();
    let mut permissions = fs::metadata(&shells).unwrap().permissions();
    permissions.set_mode(0o600);
    fs::set_permissions(&shells, permissions).unwrap();
    fs::write(&passwd, "root:x:0:0:root:/root:/bin/sh\n").unwrap();

    for _ in 0..2 {
        let output = run_uninstall(&destination, &shells, &passwd, false);
        assert!(output.status.success());
    }
    assert_eq!(fs::read_to_string(&shells).unwrap(), "/bin/sh\n");
    assert_eq!(
        fs::metadata(&shells).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert!(destination.exists());
}

#[test]
fn uninstall_rejects_unsafe_files_without_mutating_registry() {
    let directory = TestDirectory::new("uninstall-unsafe");
    let real_destination = directory.path().join("real-carli");
    let destination = directory.path().join("carli-link");
    let shells = directory.path().join("shells");
    let passwd = directory.path().join("passwd");
    fs::copy(env!("CARGO_BIN_EXE_carli"), &real_destination).unwrap();
    symlink(&real_destination, &destination).unwrap();
    let registry = format!("/bin/sh\n{}\n", destination.display());
    fs::write(&shells, &registry).unwrap();
    fs::write(&passwd, "root:x:0:0:root:/root:/bin/sh\n").unwrap();

    let output = run_uninstall(&destination, &shells, &passwd, true);
    assert_eq!(output.status.code(), Some(1));
    assert!(destination.is_symlink());
    assert_eq!(fs::read_to_string(&shells).unwrap(), registry);

    let shells_link = directory.path().join("shells-link");
    symlink(&shells, &shells_link).unwrap();
    let output = run_uninstall(&real_destination, &shells_link, &passwd, false);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(fs::read_to_string(shells).unwrap(), registry);
}

#[test]
fn failed_registry_commit_removes_new_binary_when_no_previous_copy_exists() {
    let directory = TestDirectory::new("new-binary-rollback");
    let destination = directory.path().join("carli");
    let shells = directory.path().join("shells");
    let tools = directory.path().join("tools");
    let counter = directory.path().join("mv-count");
    fs::create_dir(&tools).unwrap();
    fs::write(&shells, "/bin/sh\n").unwrap();
    let fake_mv = tools.join("mv");
    make_executable(
        &fake_mv,
        &format!(
            "#!/bin/sh\ncount=0\n[ ! -f {0} ] || count=$(cat {0})\ncount=$((count + 1))\nprintf '%s' \"$count\" > {0}\n[ \"$count\" -ne 2 ] || exit 1\nexec {1} \"$@\"\n",
            counter.display(),
            system_mv()
        ),
    );

    let output = Command::new(script("install.sh"))
        .args(["--binary", env!("CARGO_BIN_EXE_carli")])
        .args(["--destination", destination.to_str().unwrap()])
        .args(["--shells-file", shells.to_str().unwrap()])
        .env("PATH", system_path_with(&tools))
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(!destination.exists());
    assert_eq!(fs::read_to_string(shells).unwrap(), "/bin/sh\n");
}

#[test]
fn uninstall_allows_similar_but_not_exact_account_shell() {
    let directory = TestDirectory::new("similar-account-shell");
    let destination = directory.path().join("carli");
    let shells = directory.path().join("shells");
    let passwd = directory.path().join("passwd");
    fs::copy(env!("CARGO_BIN_EXE_carli"), &destination).unwrap();
    fs::write(&shells, format!("/bin/sh\n{}\n", destination.display())).unwrap();
    fs::write(
        &passwd,
        format!(
            "alice:x:1000:1000::/home/alice:{}-other\n",
            destination.display()
        ),
    )
    .unwrap();

    let output = run_uninstall(&destination, &shells, &passwd, false);
    assert!(output.status.success());
    assert_eq!(fs::read_to_string(shells).unwrap(), "/bin/sh\n");
}

#[test]
fn installer_supports_bsd_stat_metadata_output() {
    let directory = TestDirectory::new("bsd-stat");
    let destination = directory.path().join("carli");
    let shells = directory.path().join("shells");
    let tools = directory.path().join("tools");
    fs::create_dir(&tools).unwrap();
    fs::write(&shells, "/bin/sh\n").unwrap();
    let mut permissions = fs::metadata(&shells).unwrap().permissions();
    permissions.set_mode(0o640);
    fs::set_permissions(&shells, permissions).unwrap();

    make_executable(
        &tools.join("stat"),
        &format!(
            "#!/bin/sh\n[ \"$1\" = -f ] || exit 64\ncase \"$2\" in\n  %Lp) echo 640 ;;\n  %u:%g) echo {}:{} ;;\n  *) exit 64 ;;\nesac\n",
            fs::metadata(&shells).unwrap().uid(),
            fs::metadata(&shells).unwrap().gid()
        ),
    );

    let output = Command::new(script("install.sh"))
        .args(["--binary", env!("CARGO_BIN_EXE_carli")])
        .args(["--destination", destination.to_str().unwrap()])
        .args(["--shells-file", shells.to_str().unwrap()])
        .env("PATH", system_path_with(&tools))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        fs::metadata(shells).unwrap().permissions().mode() & 0o777,
        0o640
    );
}

#[test]
fn macos_uninstall_refuses_shell_assigned_through_directory_services() {
    let directory = TestDirectory::new("macos-assigned");
    let destination = directory.path().join("carli");
    let shells = directory.path().join("shells");
    let tools = directory.path().join("tools");
    fs::create_dir(&tools).unwrap();
    fs::copy(env!("CARGO_BIN_EXE_carli"), &destination).unwrap();
    let registry = format!("/bin/sh\n{}\n", destination.display());
    fs::write(&shells, &registry).unwrap();
    make_executable(&tools.join("uname"), "#!/bin/sh\necho Darwin\n");
    make_executable(
        &tools.join("dscl"),
        &format!(
            "#!/bin/sh\nprintf 'alice  %s\\n' '{}'\n",
            destination.display()
        ),
    );

    let output = Command::new(script("uninstall.sh"))
        .args(["--destination", destination.to_str().unwrap()])
        .args(["--shells-file", shells.to_str().unwrap()])
        .args(["--remove-binary"])
        .env("PATH", system_path_with(&tools))
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(destination.exists());
    assert_eq!(fs::read_to_string(shells).unwrap(), registry);
}

#[test]
fn macos_uninstall_fails_closed_when_directory_services_fails() {
    let directory = TestDirectory::new("macos-dscl-failure");
    let destination = directory.path().join("carli");
    let shells = directory.path().join("shells");
    let tools = directory.path().join("tools");
    fs::create_dir(&tools).unwrap();
    fs::copy(env!("CARGO_BIN_EXE_carli"), &destination).unwrap();
    let registry = format!("/bin/sh\n{}\n", destination.display());
    fs::write(&shells, &registry).unwrap();
    make_executable(&tools.join("uname"), "#!/bin/sh\necho Darwin\n");
    make_executable(&tools.join("dscl"), "#!/bin/sh\nexit 70\n");

    let output = Command::new(script("uninstall.sh"))
        .args(["--destination", destination.to_str().unwrap()])
        .args(["--shells-file", shells.to_str().unwrap()])
        .args(["--remove-binary"])
        .env("PATH", system_path_with(&tools))
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("could not determine"));
    assert!(destination.exists());
    assert_eq!(fs::read_to_string(shells).unwrap(), registry);
}
