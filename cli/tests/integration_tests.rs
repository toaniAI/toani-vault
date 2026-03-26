use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

#[test]
fn test_cli_help() {
    let mut cmd = Command::cargo_bin("credbridge").unwrap();
    cmd.arg("--help");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("CredBridge CLI"));
}

#[test]
fn test_config_help() {
    let mut cmd = Command::cargo_bin("credbridge").unwrap();
    cmd.args(["config", "--help"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Configuration management"));
}

#[test]
fn test_auth_help() {
    let mut cmd = Command::cargo_bin("credbridge").unwrap();
    cmd.args(["auth", "--help"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Authentication management"));
}

#[test]
fn test_credentials_help() {
    let mut cmd = Command::cargo_bin("credbridge").unwrap();
    cmd.args(["credentials", "--help"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Credential management"));
}

#[test]
fn test_tokens_help() {
    let mut cmd = Command::cargo_bin("credbridge").unwrap();
    cmd.args(["tokens", "--help"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Token"));
}

#[test]
fn test_audit_help() {
    let mut cmd = Command::cargo_bin("credbridge").unwrap();
    cmd.args(["audit", "--help"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("审计"));
}

#[test]
fn test_sandbox_help() {
    let mut cmd = Command::cargo_bin("credbridge").unwrap();
    cmd.args(["sandbox", "--help"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("沙箱"));
}

#[test]
fn test_config_show_without_config() {
    // 使用临时目录作为配置目录
    let temp_dir = TempDir::new().unwrap();
    let home = temp_dir.path();

    let mut cmd = Command::cargo_bin("credbridge").unwrap();
    cmd.args(["config", "show"]);
    cmd.env("HOME", home);

    // 应该成功执行，即使配置文件不存在
    cmd.assert().success();
}

#[test]
fn test_output_json_flag() {
    let mut cmd = Command::cargo_bin("credbridge").unwrap();
    cmd.args(["--output", "json", "config", "show"]);
    cmd.assert().success().stdout(predicate::str::contains("{"));
}
