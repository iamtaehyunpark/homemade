//! `homemade-app://<app_id>.localhost/<rel>` (macOS) / `http://homemade-app.localhost/<app_id>/<rel>` (Linux).
//! Serves the directory containing `manifest.ui.entry`, injecting the bridge script into HTML.
use crate::manifest::schema::HomemadeManifest;
use crate::state::AppState;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use tauri::http::{header, Method, Request, Response};

const BRIDGE_PATH: &str = "__homemade/bridge.js";
const BRIDGE_TAG: &str = "<script src=\"/__homemade/bridge.js\"></script>";

fn base(status: u16) -> tauri::http::response::Builder {
    Response::builder()
        .status(status)
        .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
        .header(header::ACCESS_CONTROL_ALLOW_METHODS, "GET, HEAD, OPTIONS")
        .header(header::CACHE_CONTROL, "no-store")
}

fn err(status: u16, msg: &str) -> Response<Vec<u8>> {
    base(status).header(header::CONTENT_TYPE, "text/plain").body(msg.as_bytes().to_vec()).unwrap()
}

/// Returns (app_id, remaining relative path).
fn split_app(host: &str, path: &str) -> (String, String) {
    let path = path.trim_start_matches('/');
    if !host.is_empty() && host != "localhost" && host != "homemade-app.localhost" && host != "homemade-app" {
        let id = host.split('.').next().unwrap_or("").to_string();
        (id, path.to_string())
    } else {
        match path.split_once('/') {
            Some((id, rest)) => (id.to_string(), rest.to_string()),
            None => (path.to_string(), String::new()),
        }
    }
}

fn inject_bridge(html: &str) -> String {
    let lower = html.to_ascii_lowercase();
    let mut from = 0;
    while let Some(off) = lower[from..].find("<head") {
        let i = from + off;
        from = i + 5;
        let next = lower.as_bytes().get(i + 5).copied().unwrap_or(b'>');
        if !(next == b'>' || next.is_ascii_whitespace()) { continue; }
        if let Some(close) = lower[i..].find('>') {
            let at = i + close + 1;
            return format!("{}{}{}", &html[..at], BRIDGE_TAG, &html[at..]);
        }
    }
    format!("{BRIDGE_TAG}{html}")
}

fn safe_join(dir: &Path, rel: &str) -> Option<PathBuf> {
    let mut out = dir.to_path_buf();
    for c in Path::new(rel).components() {
        match c {
            Component::Normal(s) => out.push(s),
            Component::CurDir => {}
            _ => return None,
        }
    }
    Some(out)
}

pub fn handle(state: Arc<AppState>, resources_dir: PathBuf, req: Request<Vec<u8>>) -> Response<Vec<u8>> {
    if req.method() == Method::OPTIONS {
        return base(204).body(Vec::new()).unwrap();
    }
    let host = req.uri().host().unwrap_or("").to_string();
    let decoded = percent_encoding::percent_decode_str(req.uri().path()).decode_utf8_lossy().to_string();
    let (app_id, rel) = split_app(&host, &decoded);
    let is_head = req.method() == Method::HEAD;

    if rel == BRIDGE_PATH {
        return match std::fs::read(resources_dir.join("homemade-bridge.js")) {
            Ok(b) => base(200)
                .header(header::CONTENT_TYPE, "application/javascript")
                .body(if is_head { Vec::new() } else { b })
                .unwrap(),
            Err(_) => err(404, "bridge not found"),
        };
    }
    if app_id.is_empty() {
        return err(400, "missing app id");
    }

    let root = {
        let conn = match state.db.lock() {
            Ok(c) => c,
            Err(_) => return err(500, "db poisoned"),
        };
        match crate::db::queries::get_app(&conn, &app_id) {
            Ok(Some(a)) => PathBuf::from(a.root_dir),
            Ok(None) => return err(404, "unknown app"),
            Err(_) => return err(500, "db error"),
        }
    };
    let entry = std::fs::read_to_string(root.join("homemade.toml"))
        .ok()
        .and_then(|s| HomemadeManifest::parse(&s).ok())
        .and_then(|m| m.ui.entry)
        .unwrap_or_else(|| "ui/index.html".to_string());
    let entry_path = match safe_join(&root, &entry) {
        Some(p) => p,
        None => return err(403, "bad ui.entry"),
    };
    let ui_dir = entry_path.parent().map(Path::to_path_buf).unwrap_or_else(|| root.clone());

    let file = if rel.is_empty() || rel == "/" {
        entry_path
    } else {
        match safe_join(&ui_dir, &rel) {
            Some(p) => p,
            None => return err(403, "forbidden"),
        }
    };
    let file = if file.is_dir() { file.join("index.html") } else { file };
    let bytes = match std::fs::read(&file) {
        Ok(b) => b,
        Err(_) => return err(404, "not found"),
    };
    let mime = mime_guess::from_path(&file).first_or_octet_stream();
    let mime_s = mime.essence_str().to_string();
    let body = if mime_s == "text/html" {
        inject_bridge(&String::from_utf8_lossy(&bytes)).into_bytes()
    } else {
        bytes
    };
    let ct = if mime.type_() == "text" || mime_s == "application/javascript" { format!("{mime_s}; charset=utf-8") } else { mime_s };
    base(200)
        .header(header::CONTENT_TYPE, ct)
        .header(header::CONTENT_LENGTH, body.len().to_string())
        .body(if is_head { Vec::new() } else { body })
        .unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn split() {
        assert_eq!(split_app("myapp.localhost", "/a/b.js"), ("myapp".into(), "a/b.js".into()));
        assert_eq!(split_app("homemade-app.localhost", "/myapp/x.html"), ("myapp".into(), "x.html".into()));
        assert_eq!(split_app("localhost", "/myapp"), ("myapp".into(), "".into()));
    }
    #[test]
    fn inject() {
        assert!(inject_bridge("<HTML><HEAD lang=x><title>").starts_with("<HTML><HEAD lang=x><script"));
        assert!(inject_bridge("<p>").starts_with("<script"));
    }
}
