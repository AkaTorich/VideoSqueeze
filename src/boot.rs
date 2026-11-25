// Splash screen: the main window builds itself hidden (see shape.rs) and reports steps (boot_step);
// the splash (ui/splash.html) shows them with a progress bar and, when done, calls app_ready — the main
// window gets its outline and focus, the splash closes.
use std::sync::Mutex;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::shape;

/// If the splash fails, the main window shows itself after this time.
const FALLBACK: Duration = Duration::from_secs(10);

#[derive(Default)]
pub struct Boot(Mutex<BootStep>);

#[derive(Serialize, Clone, Default)]
pub struct BootStep {
    pub progress: f64,
    pub label: String,
}

/// Main window: next build step.
#[tauri::command]
pub fn boot_step(app: AppHandle, boot: State<Boot>, progress: f64, label: String) {
    let step = BootStep { progress, label };
    *boot.0.lock().unwrap() = step.clone();
    let _ = app.emit("boot", step);
}

/// Splash: where the build is now (if the splash opened after the first steps).
#[tauri::command]
pub fn boot_status(boot: State<Boot>) -> BootStep {
    boot.0.lock().unwrap().clone()
}

/// Splash: everything is drawn — show the main window, close the splash.
#[tauri::command]
pub fn app_ready(app: AppHandle) {
    reveal(&app);
}

/// At start (both windows hidden; the splash shows itself once it has an outline — shape.rs):
/// fallback reveal of the main window if the splash doesn't make it.
pub fn start(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(FALLBACK);
        let hidden = app.get_webview_window("main").map(|w| !shape::is_revealed(&w)).unwrap_or(false);
        if hidden {
            reveal(&app);
        }
    });
}

fn reveal(app: &AppHandle) {
    if let Some(main) = app.get_webview_window("main") {
        shape::reveal_main(&main);
        let _ = main.show();
        let _ = main.set_focus();
    }
    if let Some(splash) = app.get_webview_window("splash") {
        let _ = splash.close();
    }
}
