use std::process::Command;

/// This test requires:
///   1. A running HubProxy or compatible source registry at the default source.
///   2. A running target registry (e.g. Docker Distribution) at localhost:5000.
///
/// Run with: cargo test --test sync_integration_test -- --ignored
#[tokio::test]
#[ignore]
async fn test_sync_nginx_to_local_registry() {
    let output = Command::new("cargo")
        .args([
            "run",
            "--",
            "sync",
            "--dest-registry",
            "localhost:5000",
            "--insecure-dest",
            "nginx:1.25",
        ])
        .output()
        .expect("failed to execute bamboo");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "bamboo sync failed:\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );
    assert!(
        stdout.contains("同步成功完成"),
        "预期成功日志出现在 stdout，实际 stdout:\n{}",
        stdout
    );
}
