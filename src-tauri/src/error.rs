//! Single error type used by every Tauri command. Serializes to a plain string for the frontend.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Msg(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Db(#[from] rusqlite::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Toml(#[from] toml::de::Error),
    #[error(transparent)]
    Any(#[from] anyhow::Error),
}
impl serde::Serialize for Error {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}
impl From<String> for Error { fn from(s: String) -> Self { Error::Msg(s) } }
impl From<&str> for Error { fn from(s: &str) -> Self { Error::Msg(s.to_string()) } }
pub type Result<T> = std::result::Result<T, Error>;
pub fn err<T>(msg: impl Into<String>) -> Result<T> { Err(Error::Msg(msg.into())) }
