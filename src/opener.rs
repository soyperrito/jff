use std::path::Path;
use std::process::{Command, Stdio};

use crate::config::Config;
use crate::editor::find_in_path;
use crate::error::{Error, Result};

/// Open an image with the configured external viewer, or with the platform
/// default (`xdg-open`, `open`, or `start`).
pub fn open_image(path: &Path, config: &Config) -> Result<()> {
    if let Some(opener) = config
        .images
        .opener
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        let mut command = Command::new(opener);
        command.arg(path);
        return spawn(command, opener);
    }

    #[cfg(target_os = "linux")]
    let (program, args): (&str, Vec<&str>) = ("xdg-open", Vec::new());

    #[cfg(target_os = "macos")]
    let (program, args): (&str, Vec<&str>) = ("open", Vec::new());

    #[cfg(target_os = "windows")]
    let (program, args): (&str, Vec<&str>) = ("cmd", vec!["/C", "start", ""]);

    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    let (program, args): (&str, Vec<&str>) = ("xdg-open", Vec::new());

    let executable = find_in_path(program)
        .ok_or_else(|| Error::Opener(format!("`{program}` was not found in PATH")))?;
    let mut command = Command::new(executable);
    command.args(args).arg(path);

    spawn(command, program)
}

fn spawn(mut command: Command, name: &str) -> Result<()> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|error| Error::Opener(format!("{name}: {error}")))
}
