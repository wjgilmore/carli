use std::process::Command;

#[test]
fn interactive_terminal_history_signals_and_job_control() {
    let status = Command::new("python3")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/pty_features.py"
        ))
        .arg(env!("CARGO_BIN_EXE_carli"))
        .status()
        .expect("python3 is required for PTY integration tests");

    assert!(status.success(), "PTY integration script failed: {status}");
}
