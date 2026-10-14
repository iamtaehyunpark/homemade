use tauri::http::{Request, Response};
pub fn handle(_ctx: tauri::UriSchemeContext<tauri::Wry>, req: Request<Vec<u8>>) -> Response<Vec<u8>> {
    let _ = req;
    Response::builder().status(501).body(b"agent E".to_vec()).unwrap()
}
