// Custom window shape: the window is cut along the polygon the page sends (ui/js/shape.js). Outside the
// outline there is no window at all — neither pixels nor clicks. Inside, everything is painted opaque:
// WebView2 transparency on Windows gets lost (hidden start, modal dialogs) and is filled with white, so the
// shape relies on the region only.
//
// Both windows are created hidden and shown only with their outline: the splash as soon as it sends its
// outline, the main window when it is ready (boot::reveal → reveal_main); an outline sent earlier is kept.
// (An empty region on a hidden window won't do: Windows resets it while WebView2 is being created.)
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use tauri::{Manager, State, WebviewWindow};

#[derive(Default)]
pub struct Shape {
    revealed: AtomicBool,
    pending: Mutex<Option<Vec<[f64; 2]>>>,
}

/// `points` — vertices in CSS pixels from the window's top-left corner; fewer than three remove the shape.
#[tauri::command]
pub fn set_window_shape(window: WebviewWindow, shape: State<Shape>, points: Vec<[f64; 2]>) -> Result<(), String> {
    if window.label() == "main" && !shape.revealed.load(Ordering::SeqCst) {
        *shape.pending.lock().unwrap() = Some(points);
        return Ok(());
    }
    apply(&window, &points)?;
    // the splash appears already shaped
    if window.label() != "main" && !window.is_visible().unwrap_or(true) {
        let _ = window.show();
    }
    Ok(())
}

/// Showing the main window: apply the stored outline first.
pub fn reveal_main(window: &WebviewWindow) {
    let shape = window.app_handle().state::<Shape>();
    shape.revealed.store(true, Ordering::SeqCst);
    let points = shape.pending.lock().unwrap().take().unwrap_or_default();
    let _ = apply(window, &points);
}

pub fn is_revealed(window: &WebviewWindow) -> bool {
    window.app_handle().state::<Shape>().revealed.load(Ordering::SeqCst)
}

fn apply(window: &WebviewWindow, points: &[[f64; 2]]) -> Result<(), String> {
    #[cfg(windows)]
    {
        use windows_sys::Win32::Foundation::{HWND, POINT};
        use windows_sys::Win32::Graphics::Gdi::{CreatePolygonRgn, SetWindowRgn, ALTERNATE};

        let hwnd = window.hwnd().map_err(|e| e.to_string())?.0 as HWND;
        let scale = window.scale_factor().map_err(|e| e.to_string())?;
        // the region counts from the window corner, the page from the client corner; a maximized window
        // has its frame 8 px off screen and the two differ
        let (dx, dy) = match (window.inner_position(), window.outer_position()) {
            (Ok(i), Ok(o)) => ((i.x - o.x) as f64, (i.y - o.y) as f64),
            _ => (0.0, 0.0),
        };
        unsafe {
            if points.len() < 3 {
                SetWindowRgn(hwnd, std::ptr::null_mut(), 1);
                return Ok(());
            }
            let pts: Vec<POINT> = points
                .iter()
                .map(|p| POINT { x: (p[0] * scale + dx).round() as i32, y: (p[1] * scale + dy).round() as i32 })
                .collect();
            let rgn = CreatePolygonRgn(pts.as_ptr(), pts.len() as i32, ALTERNATE);
            if rgn.is_null() {
                return Err("window shape not created".into());
            }
            // after a successful call the system owns the region
            SetWindowRgn(hwnd, rgn, 1);
        }
    }
    #[cfg(not(windows))]
    let _ = (window, points);
    Ok(())
}
