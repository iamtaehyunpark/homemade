//! macOS quit guard. Cmd+Q, the Dock's Quit, AppleScript `quit` and logout all go through
//! `-[NSApplication terminate:]`, which asks the app delegate `applicationShouldTerminate:`.
//! tao's delegate does not implement it, so the process would die without Tauri ever seeing an
//! `ExitRequested`. We add that method to tao's delegate class at runtime: while anything is
//! active it answers NSTerminateCancel and raises the confirm-exit modal; otherwise NSTerminateNow.
use crate::state::AppState;
use objc2::runtime::{AnyObject, Sel};
use objc2::{class, msg_send, sel};
use std::sync::{Arc, OnceLock};

static STATE: OnceLock<Arc<AppState>> = OnceLock::new();

const NS_TERMINATE_CANCEL: usize = 0;
const NS_TERMINATE_NOW: usize = 1;

extern "C" fn application_should_terminate(_this: &AnyObject, _cmd: Sel, _sender: *mut AnyObject) -> usize {
    let Some(st) = STATE.get() else { return NS_TERMINATE_NOW };
    let n = st.active_count();
    if n == 0 { return NS_TERMINATE_NOW; }
    use tauri::{Emitter, Manager};
    let _ = st.app_handle.emit("app://confirm-exit", n);
    if let Some(w) = st.app_handle.get_webview_window("main") { let _ = w.show(); let _ = w.set_focus(); }
    NS_TERMINATE_CANCEL
}

pub fn install(state: Arc<AppState>) {
    let _ = STATE.set(state);
    unsafe {
        let app: *mut AnyObject = msg_send![class!(NSApplication), sharedApplication];
        if app.is_null() { return; }
        let delegate: *mut AnyObject = msg_send![app, delegate];
        if delegate.is_null() { eprintln!("[homemade] no NSApp delegate; Cmd+Q guard not installed"); return; }
        let cls = (*delegate).class();
        let imp: objc2::runtime::Imp = std::mem::transmute(application_should_terminate as extern "C" fn(&AnyObject, Sel, *mut AnyObject) -> usize);
        let types = c"Q@:@";
        let added = objc2::ffi::class_addMethod(cls as *const _ as *mut _, sel!(applicationShouldTerminate:), imp, types.as_ptr());
        if !added.as_bool() { eprintln!("[homemade] delegate already handles applicationShouldTerminate:; Cmd+Q guard not installed"); }
    }
}
