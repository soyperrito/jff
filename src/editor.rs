use std::path::{Path, PathBuf};
use std::process::Command;

use crate::cli::EditorArg;
use crate::config::Config;
use crate::error::{Error, Result};
use crate::model::Entry;
use crate::store::Store;

struct ResolvedEditor {
    program: PathBuf,
    args: Vec<String>,
}

pub fn open(
    store: &Store,
    config: &Config,
    entry: &Entry,
    choice: Option<EditorArg>,
) -> Result<()> {
    let editor = resolve(config, choice)?;
    let path = entry.abs_path(store.root());

    if !path.is_file() {
        return Err(Error::MissingFile(path));
    }

    let status = Command::new(&editor.program)
        .args(&editor.args)
        .arg(&path)
        .status()?;

    if !status.success() {
        return Err(Error::EditorFailed(status.to_string()));
    }

    Ok(())
}

fn resolve(config: &Config, choice: Option<EditorArg>) -> Result<ResolvedEditor> {
    match choice.unwrap_or(EditorArg::Custom) {
        EditorArg::Custom => match &config.editor.path {
            Some(path) if !path.as_os_str().is_empty() => Ok(ResolvedEditor {
                program: path.clone(),
                args: Vec::new(),
            }),
            _ => program(&config.editor.name, Vec::new()),
        },
        EditorArg::Nano => program("nano", Vec::new()),
        EditorArg::Vim => {
            if find_in_path("nvim").is_some() {
                program("nvim", Vec::new())
            } else {
                program("vim", Vec::new())
            }
        }
        EditorArg::Code => program("code", vec!["--wait".to_string()]),
        EditorArg::Emacs => program("emacs", Vec::new()),
    }
}

fn program(name: &str, args: Vec<String>) -> Result<ResolvedEditor> {
    find_in_path(name)
        .map(|program| ResolvedEditor { program, args })
        .ok_or_else(|| Error::EditorNotFound(name.to_string()))
}

pub fn find_in_path(name: &str) -> Option<PathBuf> {
    let direct = Path::new(name);

    if direct.is_absolute() || name.contains('/') || name.contains('\\') {
        return is_executable(direct).then(|| direct.to_path_buf());
    }

    let path = std::env::var_os("PATH")?;

    std::env::split_paths(&path)
        .map(|directory| directory.join(name))
        .find(|candidate| is_executable(candidate))
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;

    path.metadata()
        .map(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    path.is_file()
}
