use std::process::Command;

#[test]
fn help_flags_print_usage_and_exit_zero() {
    for flag in ["--help", "-h"] {
        let out = Command::new(env!("CARGO_BIN_EXE_sql-mcp"))
            .arg(flag)
            .env_remove("SQL_MCP_CONFIG")
            .env_remove("SQL_MCP_SEMANTIC_PATH")
            .output()
            .expect("spawn sql-mcp");
        assert!(
            out.status.success(),
            "flag={flag} status={:?} stderr={}",
            out.status.code(),
            String::from_utf8_lossy(&out.stderr)
        );
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(stdout.contains("USAGE:"), "flag={flag} stdout={stdout}");
        assert!(stdout.contains("--stdio"), "flag={flag} stdout={stdout}");
        assert!(
            stdout.contains("SQL_MCP_CONFIG"),
            "flag={flag} stdout={stdout}"
        );
    }
}

#[test]
fn unknown_flag_prints_help_and_exits_2() {
    let out = Command::new(env!("CARGO_BIN_EXE_sql-mcp"))
        .arg("--not-a-real-flag")
        .output()
        .expect("spawn sql-mcp");
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("unknown option"), "{stderr}");
    assert!(stderr.contains("--stdio"), "{stderr}");
}

#[test]
fn missing_config_names_path_on_stderr() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("missing.toml");
    let out = Command::new(env!("CARGO_BIN_EXE_sql-mcp"))
        .env("SQL_MCP_CONFIG", &missing)
        .env("SQL_MCP_SEMANTIC_PATH", dir.path().join("semantic.db"))
        .env("RUST_LOG", "off")
        .output()
        .expect("spawn sql-mcp");
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains(missing.to_string_lossy().as_ref()),
        "{stderr}"
    );
    assert!(
        stderr.contains("SQL_MCP_CONFIG") || stderr.contains("cannot read config"),
        "{stderr}"
    );
}
