use unicode_normalization::UnicodeNormalization;

/// Lowercase, remove diacritics and collapse any run of separators into one
/// space. Used for case- and accent-insensitive comparisons.
pub fn normalize(input: &str) -> String {
    let decomposed: String = input
        .nfd()
        .filter(|ch| !unicode_normalization::char::is_combining_mark(*ch))
        .collect();

    let mut out = String::with_capacity(decomposed.len());
    let mut pending_separator = false;

    for ch in decomposed.chars().flat_map(char::to_lowercase) {
        if ch.is_alphanumeric() {
            if pending_separator && !out.is_empty() {
                out.push(' ');
            }
            out.push(ch);
            pending_separator = false;
        } else {
            pending_separator = true;
        }
    }

    out
}

/// Convert a title into a safe, lowercase ASCII file stem.
pub fn slugify(input: &str) -> String {
    let normalized = normalize(input);
    let mut out = String::with_capacity(normalized.len());
    let mut pending_separator = false;

    for ch in normalized.chars() {
        if ch.is_ascii_alphanumeric() {
            if pending_separator && !out.is_empty() {
                out.push('_');
            }
            out.push(ch.to_ascii_lowercase());
            pending_separator = false;
        } else {
            pending_separator = true;
        }
    }

    let truncated: String = out.chars().take(64).collect();
    let cleaned = truncated.trim_matches('_').to_string();

    if cleaned.is_empty() {
        "untitled".to_string()
    } else {
        cleaned
    }
}

/// Turn a file stem into a human-readable fallback title.
pub fn title_from_stem(stem: &str) -> Option<String> {
    let title = stem.replace(['_', '-'], " ");
    let title = title.split_whitespace().collect::<Vec<_>>().join(" ");

    if title.is_empty() { None } else { Some(title) }
}

/// Extract a title from YAML front matter (`title:`) or from the first
/// level-one Markdown heading (`# Title`).
pub fn title_from_markdown(content: &str) -> Option<String> {
    let content = content.trim_start_matches('\u{feff}');
    let mut lines = content.lines();

    if let Some(first) = lines.next()
        && first.trim() == "---"
    {
        for line in lines.by_ref() {
            let line = line.trim();

            if line == "---" || line == "..." {
                break;
            }

            if let Some(value) = line.strip_prefix("title:") {
                let value = value.trim().trim_matches(['"', '\'']).trim();

                if !value.is_empty() {
                    return Some(value.to_string());
                }
            }
        }
    }

    heading_from_markdown(content)
}

/// Return only the first level-one Markdown heading (`# Title`).
pub fn heading_from_markdown(content: &str) -> Option<String> {
    content.lines().find_map(|line| {
        let title = line.trim_start().strip_prefix("# ")?.trim();

        if title.is_empty() {
            None
        } else {
            Some(title.to_string())
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_removes_accents_and_collapses_separators() {
        assert_eq!(normalize("Café  Pollution!"), "cafe pollution");
        assert_eq!(normalize("water-pollution"), "water pollution");
        assert_eq!(normalize("  ¡Hola,   Mundo!  "), "hola mundo");
    }

    #[test]
    fn slugify_makes_safe_file_stems() {
        assert_eq!(slugify("Water Pollution"), "water_pollution");
        assert_eq!(slugify("  ¡Hola, Mundo! "), "hola_mundo");
        assert_eq!(slugify("C++ / Rust"), "c_rust");
        assert_eq!(slugify("---"), "untitled");
        assert_eq!(slugify(""), "untitled");
    }

    #[test]
    fn slugify_is_bounded() {
        let slug = slugify(&"word ".repeat(100));
        assert!(slug.chars().count() <= 64);
    }

    #[test]
    fn extracts_titles_from_markdown() {
        assert_eq!(
            title_from_markdown("# Water\n\nSome text.\n"),
            Some("Water".to_string())
        );
        assert_eq!(
            title_from_markdown("---\ntitle: \"Water Pollution\"\n---\n# Other\n"),
            Some("Water Pollution".to_string())
        );
        assert_eq!(title_from_markdown("## Not a title\n\nNo h1 here.\n"), None);
    }

    #[test]
    fn extracts_only_h1_headings_for_related_labels() {
        assert_eq!(
            heading_from_markdown("# Water\n\n## Subheading\n"),
            Some("Water".to_string())
        );
        assert_eq!(
            heading_from_markdown("---\ntitle: Metadata\n---\n# Visible heading\n"),
            Some("Visible heading".to_string())
        );
        assert_eq!(heading_from_markdown("## Only level two\n"), None);
    }

    #[test]
    fn derives_titles_from_file_stems() {
        assert_eq!(
            title_from_stem("water_pollution"),
            Some("water pollution".into())
        );
        assert_eq!(
            title_from_stem("Water-Pollution"),
            Some("Water Pollution".into())
        );
        assert_eq!(title_from_stem("---"), None);
    }
}
