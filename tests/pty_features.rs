use std::process::Command;

fn run_pty_scenario(script: &str, scenario: Option<&str>) {
    let mut command = Command::new("python3");
    command
        .arg(format!("{}/tests/{script}", env!("CARGO_MANIFEST_DIR")))
        .arg(env!("CARGO_BIN_EXE_carli"));
    if let Some(scenario) = scenario {
        command.arg(scenario);
    }
    let status = command
        .status()
        .expect("python3 is required for PTY integration tests");
    assert!(
        status.success(),
        "PTY integration scenario {scenario:?} failed: {status}"
    );
}

#[test]
fn interactive_terminal_history_signals_and_job_control() {
    run_pty_scenario("pty_features.py", None);
}

#[test]
fn repeated_prompt_interrupts_leave_shell_usable() {
    run_pty_scenario("pty_edge_cases.py", Some("repeated_prompt_interrupts"));
}

#[test]
fn job_selection_errors_are_specific() {
    run_pty_scenario("pty_edge_cases.py", Some("job_selection_errors"));
}

#[test]
fn bg_rejects_an_already_running_job() {
    run_pty_scenario("pty_edge_cases.py", Some("bg_rejects_running_job"));
}

#[test]
fn fg_defaults_to_newest_job_and_returns_its_status() {
    run_pty_scenario(
        "pty_edge_cases.py",
        Some("fg_default_and_normal_exit_status"),
    );
}

#[test]
fn background_terminal_reader_is_stopped_again() {
    run_pty_scenario(
        "pty_edge_cases.py",
        Some("background_terminal_read_stops_again"),
    );
}

#[test]
fn exiting_shell_hangs_up_stopped_jobs() {
    run_pty_scenario("pty_edge_cases.py", Some("shell_hangs_up_stopped_jobs"));
}

#[test]
fn prompt_handles_root_directory_and_missing_user() {
    run_pty_scenario("pty_edge_cases.py", Some("prompt_root_and_unknown_user"));
}

#[test]
fn completed_and_signaled_jobs_are_reported_and_reaped() {
    run_pty_scenario("pty_edge_cases.py", Some("job_completion_notifications"));
}
