#[test]
fn test_app_help_cli() {
    let output = std::process::Command::new("cargo")
        .args(["run", "--package", "grafito-app", "--", "--help"])
        .output()
        .expect("cargo run failed");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Usage:"),
        "expected help output to contain 'Usage:', got:\n{}",
        stdout
    );
    assert!(
        output.status.success(),
        "cargo run -- --help should exit successfully"
    );
}
