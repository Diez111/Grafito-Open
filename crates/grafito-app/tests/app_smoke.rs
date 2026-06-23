#[test]
fn test_app_help_cli() {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_grafito"))
        .arg("--help")
        .output()
        .expect("grafito --help failed");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Usage:"),
        "expected help output to contain 'Usage:', got:\n{}",
        stdout
    );
    assert!(
        output.status.success(),
        "grafito --help should exit successfully"
    );
}
