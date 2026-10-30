use std::{path::PathBuf, process::Command};
fn binary() -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_hyprannotations"));
    c.env_remove("WAYLAND_DISPLAY");
    c.env_remove("WAYLAND_SOCKET");
    c
}
fn temp(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("hyprannotations-{}-{name}.png", std::process::id()))
}
#[test]
fn preview_works_without_wayland() {
    let preview = temp("preview");
    assert!(
        binary()
            .arg("--preview")
            .arg(&preview)
            .status()
            .unwrap()
            .success()
    );
    let pix = tiny_skia::Pixmap::load_png(&preview).unwrap();
    assert_eq!((pix.width(), pix.height()), (682, 88));
    std::fs::remove_file(preview).unwrap();
}
#[test]
fn default_config_is_valid_toml() {
    let result = binary().arg("--print-config").output().unwrap();
    assert!(result.status.success());
    let config: toml::Value = toml::from_str(std::str::from_utf8(&result.stdout).unwrap()).unwrap();
    assert_eq!(config["default_tool"].as_str(), Some("pen"));
}
#[test]
#[ignore = "requires an interactive Wayland compositor and briefly captures input"]
fn wayland_surface_lifecycle() {
    assert!(
        Command::new(env!("CARGO_BIN_EXE_hyprannotations"))
            .arg("--smoke-test")
            .status()
            .unwrap()
            .success()
    );
}
