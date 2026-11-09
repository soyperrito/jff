use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::editor::find_in_path;

#[derive(Clone, Copy)]
enum ToolKind {
    Mermaid,
    Dot,
    Typst,
}

pub struct DiagramRenderer {
    mmdc: Option<PathBuf>,
    dot: Option<PathBuf>,
    typst: Option<PathBuf>,
}

impl Default for DiagramRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl DiagramRenderer {
    pub fn new() -> Self {
        Self {
            mmdc: find_in_path("mmdc"),
            dot: find_in_path("dot"),
            typst: find_in_path("typst"),
        }
    }

    pub fn supports(&self, language: &str) -> bool {
        self.command_for(language).is_some()
    }

    pub fn render(&self, root: &Path, language: &str, source: &str) -> Result<PathBuf, String> {
        let (tool, kind) = self
            .command_for(language)
            .ok_or_else(|| format!("no renderer installed for `{language}` diagrams"))?;
        let cache = root.join(".cache").join("diagrams");
        fs::create_dir_all(&cache).map_err(|error| error.to_string())?;

        let mut hasher = DefaultHasher::new();
        language.hash(&mut hasher);
        source.hash(&mut hasher);
        let stem = format!("{:016x}", hasher.finish());

        let (extension, output) = match kind {
            ToolKind::Mermaid => ("mmd", cache.join(format!("{stem}.png"))),
            ToolKind::Dot => ("dot", cache.join(format!("{stem}.png"))),
            ToolKind::Typst => ("typ", cache.join(format!("{stem}.png"))),
        };

        if output.is_file() {
            return Ok(output);
        }

        let input = cache.join(format!("{stem}.{extension}"));
        fs::write(&input, source).map_err(|error| error.to_string())?;

        let mut command = if find_in_path("timeout").is_some() {
            let mut command = Command::new("timeout");
            command.arg("20s").arg(tool);
            command
        } else {
            Command::new(tool)
        };

        match kind {
            ToolKind::Mermaid => {
                command
                    .arg("-i")
                    .arg(&input)
                    .arg("-o")
                    .arg(&output)
                    .arg("-b")
                    .arg("transparent");
            }
            ToolKind::Dot => {
                command.arg("-Tpng").arg(&input).arg("-o").arg(&output);
            }
            ToolKind::Typst => {
                command.arg("compile").arg(&input).arg(&output);
            }
        }

        let result = command
            .current_dir(&cache)
            .output()
            .map_err(|error| format!("failed to run {}: {error}", tool.display()))?;

        if !result.status.success() {
            let stderr = String::from_utf8_lossy(&result.stderr);
            return Err(format!("{} failed: {}", tool.display(), stderr.trim()));
        }

        if !output.is_file() {
            return Err(format!(
                "{} did not create {}",
                tool.display(),
                output.display()
            ));
        }

        let _ = fs::remove_file(&input);
        Ok(output)
    }

    fn command_for(&self, language: &str) -> Option<(&Path, ToolKind)> {
        match language.to_ascii_lowercase().as_str() {
            "mermaid" | "mmd" => self.mmdc.as_deref().map(|path| (path, ToolKind::Mermaid)),
            "dot" | "graphviz" => self.dot.as_deref().map(|path| (path, ToolKind::Dot)),
            "typst" => self.typst.as_deref().map(|path| (path, ToolKind::Typst)),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_languages_are_not_supported() {
        let renderer = DiagramRenderer::new();

        assert!(!renderer.supports("unknown"));
        assert!(!renderer.supports(""));
    }
}
