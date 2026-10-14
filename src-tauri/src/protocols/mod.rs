//! Custom URI schemes (spec §7). OWNER: agent E.
//!  homemade-asset://localhost/<percent-encoded absolute path>  -> stream any local file, Content-Type via mime_guess, HTTP Range support.
//!  homemade-app://<app_id>.localhost/<rel path>  (macOS form: homemade-app://<app_id>/<rel path>) -> serve <root>/<ui dir of manifest.ui.entry>/<rel>;
//!     for text/html inject `<script src="/__homemade/bridge.js"></script>` after <head> (or prepend); /__homemade/bridge.js serves resources/homemade-bridge.js.
//!  Both must set Access-Control-Allow-Origin: * and handle OPTIONS.
pub mod asset;
pub mod app_ui;
