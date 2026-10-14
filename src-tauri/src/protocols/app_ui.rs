use crate::state::AppState;
use std::sync::Arc;
use tauri::http::{Request, Response};
pub fn handle(state: Arc<AppState>, resources_dir: std::path::PathBuf, req: Request<Vec<u8>>) -> Response<Vec<u8>> {
    let _ = (state, resources_dir, req);
    Response::builder().status(501).body(b"agent E".to_vec()).unwrap()
}
