use std::fmt;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Term,
    Concept,
}

impl Kind {
    pub fn as_char(self) -> char {
        match self {
            Self::Term => 't',
            Self::Concept => 'c',
        }
    }

    pub fn dir(self) -> &'static str {
        match self {
            Self::Term => "terms",
            Self::Concept => "concepts",
        }
    }
}

impl fmt::Display for Kind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Term => "term",
            Self::Concept => "concept",
        })
    }
}

impl std::str::FromStr for Kind {
    type Err = String;

    fn from_str(value: &str) -> std::result::Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "t" | "term" | "terms" => Ok(Self::Term),
            "c" | "concept" | "concepts" => Ok(Self::Concept),
            other => Err(format!("unknown kind `{other}`; expected term or concept")),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    pub id: u64,
    #[serde(rename = "ref")]
    pub reference: String,
    pub kind: Kind,
    pub title: String,
    pub letter: char,
    /// Path relative to the jff home directory, using forward slashes.
    pub path: String,
    pub created_at: String,
}

impl Entry {
    pub fn abs_path(&self, root: &Path) -> PathBuf {
        root.join(&self.path)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Registry {
    pub version: u32,
    pub next_id: u64,
    pub entries: Vec<Entry>,
}

impl Default for Registry {
    fn default() -> Self {
        Self {
            version: 1,
            next_id: 1,
            entries: Vec::new(),
        }
    }
}
