//! Window construction. Both the main window and solo app windows get the element picker (`picker.js`) injected into
//! *all* frames, so select mode reaches into a program's web UI (a cross-origin iframe) like a browser devtools cursor.
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

const PICKER: &str = include_str!("picker.js");

pub fn main_window(app: &AppHandle) -> tauri::Result<()> {
    WebviewWindowBuilder::new(app, "main", WebviewUrl::default())
        .title("Homemade")
        .inner_size(1400.0, 900.0)
        .min_inner_size(900.0, 600.0)
        .background_color(tauri::window::Color(0xf6, 0xf7, 0xf9, 0xff))
        .initialization_script_for_all_frames(PICKER)
        .build()?;
    Ok(())
}

/// Solo window for one app (`label` = `app-<id>`, `url` = `/?app=<id>&standalone=1`). Focuses the window if it exists.
#[tauri::command]
pub fn open_app_window(app: AppHandle, label: String, url: String, title: String) -> Result<(), String> {
    if let Some(w) = app.get_webview_window(&label) { return w.set_focus().map_err(|e| e.to_string()); }
    WebviewWindowBuilder::new(&app, &label, WebviewUrl::App(url.into()))
        .title(title)
        .inner_size(960.0, 720.0)
        .initialization_script_for_all_frames(PICKER)
        .build().map(|_| ()).map_err(|e| e.to_string())
}
