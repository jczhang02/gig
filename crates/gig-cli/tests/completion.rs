use std::path::Path;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn unique_temp_dir(name: &str) -> std::path::PathBuf {
    let mut path = std::env::temp_dir();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    path.push(format!("gig-{name}-{}-{nanos}", std::process::id()));
    path
}

#[test]
fn zsh_completion_prints_script_without_bootstrapping_state() {
    let config_home = unique_temp_dir("config");
    let data_home = unique_temp_dir("data");
    let state_home = unique_temp_dir("state");

    let output = Command::new(env!("CARGO_BIN_EXE_gig"))
        .args(["completion", "zsh"])
        .env("XDG_CONFIG_HOME", &config_home)
        .env("XDG_DATA_HOME", &data_home)
        .env("XDG_STATE_HOME", &state_home)
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "completion command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("#compdef gig"));
    assert!(stdout.contains("new"));
    assert!(stdout.contains("ls"));
    assert!(stdout.contains("completion"));
    assert!(stdout.contains("--title"));

    assert!(!Path::new(&config_home).exists());
    assert!(!Path::new(&data_home).exists());
    assert!(!Path::new(&state_home).exists());
}
