use std::process::Command;

#[test]
fn test_release_binary_help() {
    // Run the binary with --help and verify it prints usage.
    let output = Command::new("cargo")
        .args([
            "run",
            "--package",
            "grafito-app",
            "--release",
            "--",
            "--help",
        ])
        .output()
        .expect("cargo run failed");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Usage:") || stdout.contains("Uso:"),
        "expected help output to contain 'Usage:' or 'Uso:', got:\n{}",
        stdout
    );
    assert!(
        output.status.success(),
        "cargo run --release -- --help should exit successfully"
    );
}
