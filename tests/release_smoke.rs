use std::process::Command;

fn grafito_binary_path() -> std::path::PathBuf {
    let mut path = std::env::current_exe().expect("current test executable path");

    while path.file_name().is_some_and(|name| name != "target") {
        path.pop();
    }

    path.push("debug");
    path.push(if cfg!(windows) {
        "grafito.exe"
    } else {
        "grafito"
    });
    path
}

#[test]
fn test_release_binary_help() {
    // The release build is covered separately; this smoke test must not run nested Cargo.
    let binary = grafito_binary_path();
    let output = Command::new(&binary)
        .arg("--help")
        .output()
        .unwrap_or_else(|err| panic!("{} --help failed: {err}", binary.display()));

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Usage:") || stdout.contains("Uso:"),
        "expected help output to contain 'Usage:' or 'Uso:', got:\n{}",
        stdout
    );
    assert!(
        output.status.success(),
        "{} --help should exit successfully",
        binary.display()
    );
}
