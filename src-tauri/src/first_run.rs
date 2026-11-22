use std::fs;
use std::path::Path;
use std::process::Command;

use tauri::{AppHandle, Manager};

const EXTENSION_UUID: &str = "pookieclip@gracepetermutiibwa.github.io";
const AUTOSTART_FILE: &str = "io.github.gracepetermutiibwa.pookieclip.desktop";

// Runs once after install: turns on the GNOME extension and starts PookieClip at login.
// Skipped in dev builds so a debug binary never ends up in autostart.
pub fn run_once(app: &AppHandle, data_dir: &Path) {
    let marker = data_dir.join("setup-done");
    if cfg!(debug_assertions) || marker.exists() {
        return;
    }

    // Does nothing outside GNOME, where the command is missing
    let _ = Command::new("gnome-extensions")
        .args(["enable", EXTENSION_UUID])
        .output();

    add_autostart(app);
    let _ = fs::write(marker, "");
}

fn add_autostart(app: &AppHandle) {
    let (Ok(config_dir), Ok(exe)) = (app.path().config_dir(), std::env::current_exe()) else {
        return;
    };
    let autostart_dir = config_dir.join("autostart");
    let entry = format!(
        "[Desktop Entry]\nType=Application\nName=PookieClip\nExec={} --hidden\nIcon=pookieclip\nNoDisplay=true\nX-GNOME-Autostart-enabled=true\n",
        exe.display()
    );
    if fs::create_dir_all(&autostart_dir).is_ok() {
        let _ = fs::write(autostart_dir.join(AUTOSTART_FILE), entry);
    }
}
