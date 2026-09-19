use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT_DIRECTORY: AtomicUsize = AtomicUsize::new(1);

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new() -> Self {
        let number = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("carli-release-{}-{number}", std::process::id()));
        fs::create_dir_all(&path).unwrap();
        Self(fs::canonicalize(path).unwrap())
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn release_script_builds_a_versioned_archive_and_checksum() {
    let directory = TestDirectory::new();
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/package-release.sh");
    let output = Command::new(script)
        .args(["--binary", env!("CARGO_BIN_EXE_carli")])
        .args(["--target", "test-target"])
        .args(["--output", directory.0.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let bundle = format!("carli-v{}-test-target", env!("CARGO_PKG_VERSION"));
    let archive = directory.0.join(format!("{bundle}.tar.gz"));
    let checksums = directory.0.join("SHA256SUMS");
    assert!(archive.is_file());
    assert!(checksums.is_file());
    assert!(
        fs::read_to_string(checksums)
            .unwrap()
            .contains(&format!("{bundle}.tar.gz"))
    );

    let listing = Command::new("tar")
        .args(["-tzf", archive.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(listing.status.success());
    let listing = String::from_utf8(listing.stdout).unwrap();
    for relative in [
        "carli",
        "LICENSE",
        "README.md",
        "CHANGELOG.md",
        "scripts/install.sh",
        "scripts/uninstall.sh",
    ] {
        let expected = format!("{bundle}/{relative}");
        assert!(listing.lines().any(|line| line == expected), "{expected}");
    }
}
