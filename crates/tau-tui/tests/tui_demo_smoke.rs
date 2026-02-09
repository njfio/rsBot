use std::process::Command;

#[test]
fn functional_tui_demo_binary_renders_single_frame_without_color() {
    let binary = env!("CARGO_BIN_EXE_tau-tui");
    let output = Command::new(binary)
        .args([
            "--frames",
            "1",
            "--sleep-ms",
            "0",
            "--width",
            "48",
            "--no-color",
        ])
        .output()
        .expect("binary executes");
    assert!(
        output.status.success(),
        "status={} stderr={}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Tau TUI Demo - frame 1/1"));
    assert!(stdout.contains("op:update"));
}

#[test]
fn integration_tui_demo_binary_renders_multiple_frames() {
    let binary = env!("CARGO_BIN_EXE_tau-tui");
    let output = Command::new(binary)
        .args([
            "--frames",
            "2",
            "--sleep-ms",
            "0",
            "--width",
            "56",
            "--no-color",
        ])
        .output()
        .expect("binary executes");
    assert!(
        output.status.success(),
        "status={} stderr={}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Tau TUI Demo - frame 1/2"));
    assert!(stdout.contains("Tau TUI Demo - frame 2/2"));
}

#[test]
fn regression_tui_demo_binary_rejects_invalid_frames_argument() {
    let binary = env!("CARGO_BIN_EXE_tau-tui");
    let output = Command::new(binary)
        .args(["--frames", "0"])
        .output()
        .expect("binary executes");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("--frames must be >= 1"));
}
