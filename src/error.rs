use std::path::PathBuf;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("invalid JSON: {0}")]
    Json(#[from] serde_json::Error),

    #[error("no entry found for ref `{0}`")]
    RefNotFound(String),

    #[error("no entry found matching `{0}`")]
    NotFound(String),

    #[error("`{0}` matches more than one entry: {1}")]
    Ambiguous(String, String),

    #[error("an entry titled `{0}` already exists ({1})")]
    Duplicate(String, String),

    #[error("title cannot be empty")]
    EmptyTitle,

    #[error("invalid title: {0}")]
    InvalidTitle(String),

    #[error("file `{0}` does not exist; the registry and the content tree are out of sync")]
    MissingFile(PathBuf),

    #[error("editor `{0}` was not found in PATH")]
    EditorNotFound(String),

    #[error("editor exited with status {0}")]
    EditorFailed(String),

    #[error("theme error: {0}")]
    Theme(String),

    #[error("home directory could not be determined; set JFF_HOME")]
    NoHome,
}

pub type Result<T> = std::result::Result<T, Error>;
