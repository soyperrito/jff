use std::path::{Component, Path, PathBuf};

use pulldown_cmark::{Event, Parser, Tag, TagEnd};
use serde::Serialize;

use crate::model::Kind;

/// A `[more:term](...)` or `[more:concept](...)` link found in a Markdown file.
#[derive(Debug, Clone, Serialize)]
pub struct MoreLink {
    #[serde(rename = "type")]
    pub kind: Kind,
    pub label: String,
    pub dest: String,
}

pub fn extract(content: &str) -> Vec<MoreLink> {
    let mut links = Vec::new();
    let mut current: Option<(String, String)> = None;

    for event in Parser::new(content) {
        match event {
            Event::Start(Tag::Link { dest_url, .. }) => {
                current = Some((String::new(), dest_url.to_string()));
            }
            Event::Text(text) | Event::Code(text) => {
                if let Some((label, _)) = current.as_mut() {
                    label.push_str(&text);
                }
            }
            Event::End(TagEnd::Link) => {
                if let Some((label, dest)) = current.take()
                    && let Some(kind) = kind_from_label(&label)
                {
                    links.push(MoreLink { kind, label, dest });
                }
            }
            _ => {}
        }
    }

    links
}

fn kind_from_label(label: &str) -> Option<Kind> {
    let label = label.trim().to_ascii_lowercase();

    if label.starts_with("more:term") || label.starts_with("more:t") {
        Some(Kind::Term)
    } else if label.starts_with("more:concept") || label.starts_with("more:c") {
        Some(Kind::Concept)
    } else {
        None
    }
}

/// Resolve a link destination to a path relative to `root`.
///
/// Absolute destinations must live under `root`. Relative destinations are
/// resolved from `base_dir`. `..` components that escape `root`, URLs and
/// empty destinations are rejected.
pub fn resolve_path(root: &Path, base_dir: &Path, dest: &str) -> Option<PathBuf> {
    let dest = dest.split(['#', '?']).next().unwrap_or(dest).trim();

    if dest.is_empty() || dest.contains("://") || dest.starts_with("mailto:") {
        return None;
    }

    let path = Path::new(dest);
    let joined = if path.is_absolute() {
        path.to_path_buf()
    } else {
        base_dir.join(path)
    };

    let relative = joined.strip_prefix(root).ok()?;
    normalize_relative(relative)
}

fn normalize_relative(path: &Path) -> Option<PathBuf> {
    let mut out = PathBuf::new();

    for component in path.components() {
        match component {
            Component::Normal(part) => out.push(part),
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    return None;
                }
            }
            Component::RootDir | Component::Prefix(_) => return None,
        }
    }

    if out.as_os_str().is_empty() {
        None
    } else {
        Some(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_more_links() {
        let content = r#"
# Water

See [more:concept](../../concepts/w/water_polution.md) and [not related](x.md).
Also [more:term](wt20261109123456).
Short [more:t](jff:wt20261109123456) and [more:c](wc20261109123456).
"#;

        let links = extract(content);

        assert_eq!(links.len(), 4);
        assert_eq!(links[0].kind, Kind::Concept);
        assert_eq!(links[0].dest, "../../concepts/w/water_polution.md");
        assert_eq!(links[1].kind, Kind::Term);
        assert_eq!(links[1].dest, "wt20261109123456");
        assert_eq!(links[2].kind, Kind::Term);
        assert_eq!(links[2].dest, "jff:wt20261109123456");
        assert_eq!(links[3].kind, Kind::Concept);
        assert_eq!(links[3].dest, "wc20261109123456");
    }

    #[test]
    fn resolves_relative_paths() {
        let root = Path::new("/home/u/.jff");
        let base = Path::new("/home/u/.jff/terms/w");

        assert_eq!(
            resolve_path(root, base, "../../concepts/w/water.md"),
            Some(PathBuf::from("concepts/w/water.md"))
        );
        assert_eq!(
            resolve_path(root, base, "/home/u/.jff/concepts/w/water.md#top"),
            Some(PathBuf::from("concepts/w/water.md"))
        );
    }

    #[test]
    fn rejects_escaping_and_external_paths() {
        let root = Path::new("/home/u/.jff");
        let base = Path::new("/home/u/.jff/terms/w");

        assert_eq!(resolve_path(root, base, "../../../etc/passwd"), None);
        assert_eq!(resolve_path(root, base, "https://example.com"), None);
        assert_eq!(resolve_path(root, base, "/etc/passwd"), None);
        assert_eq!(resolve_path(root, base, ""), None);
    }
}
