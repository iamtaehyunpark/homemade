//! Window construction. Both the main window and solo app windows get the element picker (`picker.js`) injected into
//! *all* frames, so select mode reaches into a program's web UI (a cross-origin iframe) like a browser devtools cursor.
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};
use tauri::menu::{Menu, MenuItem, MenuItemKind};

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

/// Default menu, except ⌘W: the native "Close Window" item would close the whole window before the page sees the key.
/// Ours emits `menu://close` to the focused window, which closes the focused shell tab / program panel first.
pub fn install_menu(app: &AppHandle) -> tauri::Result<()> {
    let menu = Menu::default(app)?;
    for item in menu.items()? {
        let MenuItemKind::Submenu(sub) = item else { continue };
        for it in sub.items()? {
            let MenuItemKind::Predefined(p) = &it else { continue };
            if p.text()?.starts_with("Close") {
                sub.remove(&it)?;
                sub.append(&MenuItem::with_id(app, "close", "Close", true, Some("CmdOrCtrl+W"))?)?;
            }
        }
    }
    app.set_menu(menu)?;
    app.on_menu_event(|app, ev| {
        if ev.id() == "close" {
            if let Some(w) = app.webview_windows().into_values().find(|w| w.is_focused().unwrap_or(false)) { let _ = app.emit_to(w.label(), "menu://close", ()); }
        }
    });
    Ok(())
}
