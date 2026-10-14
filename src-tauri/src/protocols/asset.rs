//! `homemade-asset://localhost/<percent-encoded abs path>` (Linux: `http://homemade-asset.localhost/<path>`).
//! Streams local files with HTTP Range support.
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;
use tauri::http::{header, Method, Request, Response};

const MAX_CHUNK: u64 = 64 * 1024 * 1024;

fn base(status: u16) -> tauri::http::response::Builder {
    Response::builder()
        .status(status)
        .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
        .header(header::ACCESS_CONTROL_ALLOW_METHODS, "GET, HEAD, OPTIONS")
        .header(header::ACCESS_CONTROL_ALLOW_HEADERS, "Range")
        .header(header::ACCESS_CONTROL_EXPOSE_HEADERS, "Content-Range, Content-Length, Accept-Ranges")
        .header(header::ACCEPT_RANGES, "bytes")
}

fn err(status: u16, msg: &str) -> Response<Vec<u8>> {
    base(status).header(header::CONTENT_TYPE, "text/plain").body(msg.as_bytes().to_vec()).unwrap()
}

/// Parse a single `bytes=` range against a file of `len` bytes. Returns inclusive (start, end) or Err(()) for 416.
fn parse_range(h: &str, len: u64) -> Option<Result<(u64, u64), ()>> {
    let spec = h.trim().strip_prefix("bytes=")?;
    let first = spec.split(',').next()?.trim();
    let (a, b) = first.split_once('-')?;
    let (a, b) = (a.trim(), b.trim());
    if len == 0 {
        return Some(Err(()));
    }
    let r = if a.is_empty() {
        let suffix: u64 = b.parse().ok()?;
        if suffix == 0 { return Some(Err(())); }
        (len.saturating_sub(suffix), len - 1)
    } else {
        let start: u64 = a.parse().ok()?;
        let end: u64 = if b.is_empty() { len - 1 } else { b.parse::<u64>().ok()?.min(len - 1) };
        if start >= len || start > end { return Some(Err(())); }
        (start, end)
    };
    Some(Ok(r))
}

pub fn handle(_ctx: tauri::UriSchemeContext<tauri::Wry>, req: Request<Vec<u8>>) -> Response<Vec<u8>> {
    if req.method() == Method::OPTIONS {
        return base(204).body(Vec::new()).unwrap();
    }
    let raw = req.uri().path();
    let decoded = percent_encoding::percent_decode_str(raw).decode_utf8_lossy().to_string();
    // The URI path always starts with '/'; an encoded absolute path decodes to "//abs/..." or "/abs/...".
    let mut p = decoded.as_str();
    while p.starts_with("//") { p = &p[1..]; }
    #[cfg(windows)]
    let p = p.trim_start_matches('/');
    let path = PathBuf::from(p);
    if !path.is_absolute() {
        return err(400, "path must be absolute");
    }
    let meta = match std::fs::metadata(&path) {
        Ok(m) => m,
        Err(_) => return err(404, "not found"),
    };
    if meta.is_dir() {
        return err(403, "is a directory");
    }
    let len = meta.len();
    let mime = mime_guess::from_path(&path).first_or_octet_stream().to_string();
    let is_head = req.method() == Method::HEAD;

    let range = req.headers().get(header::RANGE).and_then(|v| v.to_str().ok()).and_then(|h| parse_range(h, len));
    let (status, start, end) = match range {
        Some(Err(())) => {
            return base(416).header(header::CONTENT_RANGE, format!("bytes */{len}")).body(Vec::new()).unwrap();
        }
        Some(Ok((s, e))) => (206u16, s, e.min(s + MAX_CHUNK - 1)),
        None => {
            if len == 0 { (200, 0, 0) } else if len > MAX_CHUNK { (206, 0, MAX_CHUNK - 1) } else { (200, 0, len - 1) }
        }
    };
    let count = if len == 0 { 0 } else { end - start + 1 };

    let mut b = base(status).header(header::CONTENT_TYPE, &mime).header(header::CONTENT_LENGTH, count.to_string());
    if status == 206 {
        b = b.header(header::CONTENT_RANGE, format!("bytes {start}-{end}/{len}"));
    }
    if is_head {
        return b.body(Vec::new()).unwrap();
    }
    let mut buf = vec![0u8; count as usize];
    let res = (|| -> std::io::Result<()> {
        let mut f = File::open(&path)?;
        f.seek(SeekFrom::Start(start))?;
        f.read_exact(&mut buf)?;
        Ok(())
    })();
    match res {
        Ok(()) => b.body(buf).unwrap(),
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => err(403, "permission denied"),
        Err(e) => err(500, &e.to_string()),
    }
}
