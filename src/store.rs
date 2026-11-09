use std::fs::{self, File, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};

use serde::Serialize;
use tempfile::NamedTempFile;

use crate::error::{Error, Result};
use crate::links::{self, MoreLink};
use crate::model::{Entry, Kind, Registry};
use crate::refs;
use crate::text;

#[derive(Debug, Clone, Serialize)]
pub struct Related {
    pub link: MoreLink,
    pub target: Option<Entry>,
    /// First `# heading` of the target file, used by the related option list.
    pub heading: Option<String>,
}

#[derive(Clone)]
pub struct Store {
    root: PathBuf,
}
impl Store {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn ensure_layout(&self) -> Result<()> {
        fs::create_dir_all(self.root.join(Kind::Term.dir()))?;
        fs::create_dir_all(self.root.join(Kind::Concept.dir()))?;
        Ok(())
    }

    #[cfg(test)]
    pub fn create(&self, kind: Kind, title: &str) -> Result<Entry> {
        self.insert(kind, title, None, NewContent::Generated)
    }

    /// Import an existing Markdown file, or create a new one when `source`
    /// does not exist.
    ///
    /// The title is taken from YAML front matter (`title:`) or from the first
    /// `# heading`; otherwise it falls back to the file stem. The first letter
    /// of that title selects the `terms/<letter>` or `concepts/<letter>`
    /// folder. Existing sources are copied, never moved.
    pub fn create_from_file(&self, kind: Kind, source: &Path) -> Result<Entry> {
        let stem = source
            .file_stem()
            .and_then(|value| value.to_str())
            .and_then(text::title_from_stem)
            .ok_or_else(|| {
                Error::InvalidTitle(format!(
                    "`{}` does not have a usable file name",
                    source.display()
                ))
            })?;

        if source.exists() {
            if !source.is_file() {
                return Err(Error::InvalidTitle(format!(
                    "`{}` is not a regular file",
                    source.display()
                )));
            }

            let contents = fs::read_to_string(source)?;
            let title = text::title_from_markdown(&contents).unwrap_or(stem);
            self.insert(kind, &title, Some(source), NewContent::Custom(contents))
        } else {
            self.insert(kind, &stem, None, NewContent::Generated)
        }
    }

    fn insert(
        &self,
        kind: Kind,
        title: &str,
        source: Option<&Path>,
        new_content: NewContent,
    ) -> Result<Entry> {
        let title = validate_title(title)?;
        let _lock = self.lock()?;
        let mut registry = self.load()?;
        let normalized = text::normalize(title);

        if let Some(existing) = registry
            .entries
            .iter()
            .find(|entry| entry.kind == kind && text::normalize(&entry.title) == normalized)
        {
            return Err(Error::Duplicate(
                existing.title.clone(),
                format!("{} {}", existing.kind, existing.reference),
            ));
        }

        let letter = refs::first_letter(title);
        let mut stamp = refs::timestamp_now();
        let reference = loop {
            let candidate = refs::make(kind, title, &stamp);

            if !registry
                .entries
                .iter()
                .any(|entry| entry.reference.eq_ignore_ascii_case(&candidate))
            {
                break candidate;
            }

            stamp = refs::bump_stamp(&stamp);
        };

        let dir = self.root.join(kind.dir()).join(letter.to_string());
        fs::create_dir_all(&dir)?;

        let slug = text::slugify(title);
        let mut stem = slug.clone();
        let mut abs_path = dir.join(format!("{stem}.md"));
        let mut relative_path = format!("{}/{}/{}.md", kind.dir(), letter, stem);
        let mut skip_write = false;

        if let Some(source) = source
            && let (Ok(source), Ok(destination)) = (source.canonicalize(), abs_path.canonicalize())
            && source == destination
        {
            skip_write = true;
        }

        if !skip_write {
            let mut attempt = 0u16;

            while abs_path.exists() {
                attempt += 1;
                stem = if attempt == 1 {
                    format!("{slug}_{reference}")
                } else {
                    format!("{slug}_{reference}_{attempt}")
                };
                abs_path = dir.join(format!("{stem}.md"));
                relative_path = format!("{}/{}/{}.md", kind.dir(), letter, stem);
            }
        }

        let created_at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        let contents = match new_content {
            NewContent::Generated => format!(
                "# {title}\n\n<!-- jff:ref={reference} kind={kind} created={created_at} -->\n"
            ),
            NewContent::Custom(contents) => contents,
        };

        if !skip_write {
            fs::write(&abs_path, contents)?;
        }

        let entry = Entry {
            id: registry.next_id,
            reference: reference.clone(),
            kind,
            title: title.to_string(),
            letter,
            path: relative_path,
            created_at,
        };

        registry.next_id = registry.next_id.saturating_add(1);
        registry.entries.push(entry.clone());

        if let Err(err) = self.save(&registry) {
            if !skip_write {
                let _ = fs::remove_file(&abs_path);
            }
            return Err(err);
        }

        Ok(entry)
    }

    pub fn get(&self, reference: &str) -> Result<Entry> {
        let reference = reference.trim();
        let registry = self.load()?;

        registry
            .entries
            .into_iter()
            .find(|entry| entry.reference.eq_ignore_ascii_case(reference))
            .ok_or_else(|| Error::RefNotFound(reference.to_string()))
    }

    pub fn delete(&self, reference: &str) -> Result<Entry> {
        let _lock = self.lock()?;
        let mut registry = self.load()?;
        let reference = reference.trim();

        let index = registry
            .entries
            .iter()
            .position(|entry| entry.reference.eq_ignore_ascii_case(reference))
            .ok_or_else(|| Error::RefNotFound(reference.to_string()))?;

        let entry = registry.entries.remove(index);
        self.save(&registry)?;
        let _ = fs::remove_file(entry.abs_path(&self.root));

        Ok(entry)
    }

    pub fn content(&self, entry: &Entry) -> Result<String> {
        let path = entry.abs_path(&self.root);

        match fs::read_to_string(&path) {
            Ok(contents) => Ok(contents),
            Err(err) if err.kind() == ErrorKind::NotFound => Err(Error::MissingFile(path)),
            Err(err) => Err(err.into()),
        }
    }

    pub fn list(&self, kind: Option<Kind>, letter: Option<char>) -> Result<Vec<Entry>> {
        let registry = self.load()?;
        let letter = letter.map(|value| value.to_ascii_lowercase());

        let mut entries: Vec<Entry> = registry
            .entries
            .into_iter()
            .filter(|entry| kind.is_none_or(|kind| entry.kind == kind))
            .filter(|entry| letter.is_none_or(|letter| entry.letter.to_ascii_lowercase() == letter))
            .collect();

        sort_entries(&mut entries);
        Ok(entries)
    }

    pub fn find_exact(&self, kind: Option<Kind>, title: &str) -> Result<Vec<Entry>> {
        let key = text::normalize(title);
        let registry = self.load()?;

        let mut entries: Vec<Entry> = registry
            .entries
            .into_iter()
            .filter(|entry| kind.is_none_or(|kind| entry.kind == kind))
            .filter(|entry| text::normalize(&entry.title) == key)
            .collect();

        sort_entries(&mut entries);
        Ok(entries)
    }

    pub fn search(&self, kind: Option<Kind>, query: &str) -> Result<Vec<Entry>> {
        let key = text::normalize(query);
        let registry = self.load()?;

        let mut scored: Vec<(u8, Entry)> = registry
            .entries
            .into_iter()
            .filter(|entry| kind.is_none_or(|kind| entry.kind == kind))
            .filter_map(|entry| {
                let title = text::normalize(&entry.title);
                let score = if key.is_empty() {
                    3
                } else if title == key {
                    0
                } else if title.starts_with(&key) {
                    1
                } else if title.contains(&key) {
                    2
                } else {
                    return None;
                };

                Some((score, entry))
            })
            .collect();

        scored.sort_by(|left, right| {
            left.0
                .cmp(&right.0)
                .then_with(|| {
                    left.1
                        .title
                        .to_lowercase()
                        .cmp(&right.1.title.to_lowercase())
                })
                .then_with(|| left.1.reference.cmp(&right.1.reference))
        });

        Ok(scored.into_iter().map(|(_, entry)| entry).collect())
    }

    /// Find a single entry by exact title, falling back to a fuzzy search.
    pub fn resolve_one(&self, query: &str, kind: Option<Kind>) -> Result<Entry> {
        let exact = self.find_exact(kind, query)?;

        if exact.len() == 1 {
            return Ok(exact.into_iter().next().expect("length checked"));
        }

        if exact.len() > 1 {
            return Err(ambiguous(query, &exact));
        }

        let matches = self.search(kind, query)?;

        match matches.len() {
            0 => Err(Error::NotFound(query.to_string())),
            1 => Ok(matches.into_iter().next().expect("length checked")),
            _ => Err(ambiguous(query, &matches)),
        }
    }

    pub fn related(&self, reference: &str) -> Result<Vec<Related>> {
        let entry = self.get(reference)?;
        let contents = self.content(&entry)?;
        let abs_path = entry.abs_path(&self.root);
        let base_dir = abs_path.parent().unwrap_or(&self.root);
        let registry = self.load()?;

        Ok(links::extract(&contents)
            .into_iter()
            .map(|link| {
                let target = resolve_link(&self.root, base_dir, &link, &registry);
                let heading = target.as_ref().and_then(|entry| {
                    self.content(entry)
                        .ok()
                        .and_then(|contents| text::heading_from_markdown(&contents))
                });

                Related {
                    link,
                    target,
                    heading,
                }
            })
            .collect())
    }

    fn registry_path(&self) -> PathBuf {
        self.root.join("map.json")
    }

    fn lock_path(&self) -> PathBuf {
        self.root.join(".lock")
    }

    fn load(&self) -> Result<Registry> {
        let path = self.registry_path();

        match fs::read_to_string(&path) {
            Ok(contents) if contents.trim().is_empty() => Ok(Registry::default()),
            Ok(contents) => {
                let value: serde_json::Value = serde_json::from_str(&contents)?;

                if value.is_array() {
                    let entries: Vec<Entry> = serde_json::from_value(value)?;
                    let next_id = entries.iter().map(|entry| entry.id).max().unwrap_or(0) + 1;

                    return Ok(Registry {
                        version: 1,
                        next_id,
                        entries,
                    });
                }

                let mut registry: Registry = serde_json::from_value(value)?;
                repair_registry(&mut registry);
                Ok(registry)
            }
            Err(err) if err.kind() == ErrorKind::NotFound => Ok(Registry::default()),
            Err(err) => Err(err.into()),
        }
    }

    fn save(&self, registry: &Registry) -> Result<()> {
        self.ensure_layout()?;
        let path = self.registry_path();
        let mut temporary = NamedTempFile::new_in(&self.root)?;
        let contents = serde_json::to_string_pretty(registry)?;

        temporary.write_all(contents.as_bytes())?;
        temporary.write_all(b"\n")?;
        temporary
            .persist(&path)
            .map_err(|error| Error::Io(error.error))?;

        Ok(())
    }

    fn lock(&self) -> Result<LockGuard> {
        self.ensure_layout()?;
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(self.lock_path())?;

        file.lock()?;
        Ok(LockGuard { file })
    }
}

struct LockGuard {
    file: File,
}

impl Drop for LockGuard {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}

enum NewContent {
    Generated,
    Custom(String),
}

fn validate_title(title: &str) -> Result<&str> {
    let title = title.trim();

    if title.is_empty() {
        return Err(Error::EmptyTitle);
    }

    if title.chars().count() > 200 {
        return Err(Error::InvalidTitle(
            "must be at most 200 characters".to_string(),
        ));
    }

    if title.chars().any(char::is_control) {
        return Err(Error::InvalidTitle(
            "must not contain control characters".to_string(),
        ));
    }

    Ok(title)
}

fn sort_entries(entries: &mut [Entry]) {
    entries.sort_by(|left, right| {
        left.title
            .to_lowercase()
            .cmp(&right.title.to_lowercase())
            .then_with(|| left.reference.cmp(&right.reference))
    });
}

fn ambiguous(query: &str, entries: &[Entry]) -> Error {
    let refs = entries
        .iter()
        .take(10)
        .map(|entry| entry.reference.clone())
        .collect::<Vec<_>>()
        .join(", ");

    Error::Ambiguous(query.to_string(), refs)
}

fn resolve_link(
    root: &Path,
    base_dir: &Path,
    link: &MoreLink,
    registry: &Registry,
) -> Option<Entry> {
    let dest = link.dest.trim();

    if let Some(raw_reference) = dest.strip_prefix("jff:") {
        return find_by_ref(registry, raw_reference.trim());
    }

    if refs::is_valid(dest)
        && let Some(entry) = find_by_ref(registry, dest)
    {
        return Some(entry);
    }

    let relative = links::resolve_path(root, base_dir, dest)?;
    let key = path_key(&relative);

    registry
        .entries
        .iter()
        .find(|entry| path_key(Path::new(&entry.path)) == key)
        .cloned()
}

fn find_by_ref(registry: &Registry, reference: &str) -> Option<Entry> {
    registry
        .entries
        .iter()
        .find(|entry| entry.reference.eq_ignore_ascii_case(reference))
        .cloned()
}

fn path_key(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn repair_registry(registry: &mut Registry) {
    if registry.version == 0 {
        registry.version = 1;
    }

    let next_id = registry
        .entries
        .iter()
        .map(|entry| entry.id)
        .max()
        .unwrap_or(0)
        .saturating_add(1);

    if registry.next_id < next_id {
        registry.next_id = next_id;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn store() -> (TempDir, Store) {
        let directory = TempDir::new().expect("temporary directory");
        let store = Store::new(directory.path().join(".jff"));
        (directory, store)
    }

    #[test]
    fn creates_an_entry_and_markdown_file() {
        let (_directory, store) = store();
        let entry = store.create(Kind::Term, "Water Pollution").expect("create");

        assert_eq!(entry.title, "Water Pollution");
        assert_eq!(entry.kind, Kind::Term);
        assert_eq!(entry.letter, 'w');
        assert!(entry.reference.starts_with("wt"));
        assert!(entry.reference.len() == 16);
        assert!(store.root().join("terms/w/water_pollution.md").is_file());

        let registry = store.load().expect("registry");
        assert_eq!(registry.entries.len(), 1);
    }

    #[test]
    fn keeps_refs_unique_in_the_same_second() {
        let (_directory, store) = store();
        let water = store.create(Kind::Term, "Water").expect("water");
        let wine = store.create(Kind::Term, "Wine").expect("wine");

        assert_ne!(water.reference, wine.reference);
    }

    #[test]
    fn rejects_duplicate_titles_per_kind() {
        let (_directory, store) = store();
        store.create(Kind::Term, "Water").expect("first");

        let error = store
            .create(Kind::Term, "  water  ")
            .expect_err("duplicate must fail");

        assert!(matches!(error, Error::Duplicate(..)));
    }

    #[test]
    fn search_ignores_case_and_accents() {
        let (_directory, store) = store();
        store
            .create(Kind::Concept, "Contaminación del agua")
            .expect("create");

        let found = store
            .find_exact(Some(Kind::Concept), "contaminacion del agua")
            .expect("search");

        assert_eq!(found.len(), 1);
    }

    #[test]
    fn deletes_entry_and_file() {
        let (_directory, store) = store();
        let entry = store.create(Kind::Term, "Water").expect("create");
        let path = entry.abs_path(store.root());

        let deleted = store.delete(&entry.reference).expect("delete");

        assert_eq!(deleted.reference, entry.reference);
        assert!(!path.exists());
        assert!(store.load().expect("registry").entries.is_empty());
    }

    #[test]
    fn resolves_more_links_from_markdown() {
        let (_directory, store) = store();
        let water = store.create(Kind::Term, "Water").expect("term");
        let pollution = store
            .create(Kind::Concept, "Water Pollution")
            .expect("concept");

        let term_path = water.abs_path(store.root());
        let mut contents = store.content(&water).expect("content");
        contents.push_str("\n[more:concept](../../concepts/w/water_pollution.md)\n");
        fs::write(&term_path, contents).expect("write link");

        let related = store.related(&water.reference).expect("related");

        assert_eq!(related.len(), 1);
        assert_eq!(
            related[0].target.as_ref().map(|entry| &entry.reference),
            Some(&pollution.reference)
        );
    }

    #[test]
    fn related_uses_the_target_h1_as_label() {
        let (_directory, store) = store();
        let water = store.create(Kind::Term, "Water").expect("term");
        let pollution = store
            .create(Kind::Concept, "Water Pollution")
            .expect("concept");

        let concept_path = pollution.abs_path(store.root());
        fs::write(&concept_path, "# Visible heading\n\nBody.\n").expect("rewrite concept");

        let term_path = water.abs_path(store.root());
        let mut contents = store.content(&water).expect("content");
        contents.push_str("\n[more:concept](../../concepts/w/water_pollution.md)\n");
        fs::write(&term_path, contents).expect("append link");

        let related = store.related(&water.reference).expect("related");

        assert_eq!(related.len(), 1);
        assert_eq!(related[0].heading.as_deref(), Some("Visible heading"));
        assert_eq!(
            related[0].target.as_ref().map(|entry| entry.title.as_str()),
            Some("Water Pollution")
        );
    }

    #[test]
    fn imports_an_existing_markdown_file() {
        let (directory, store) = store();
        let source = directory.path().join("inbox/agua.md");
        let contents = "---\ntitle: \"Water Pollution\"\n---\n\n# Heading in body\n\nBody text.\n";

        fs::create_dir_all(source.parent().unwrap()).expect("source dir");
        fs::write(&source, contents).expect("source file");

        let entry = store.create_from_file(Kind::Term, &source).expect("import");

        assert_eq!(entry.title, "Water Pollution");
        assert_eq!(entry.letter, 'w');
        assert_eq!(entry.path, "terms/w/water_pollution.md");
        assert_eq!(store.content(&entry).expect("content"), contents);
        assert!(source.is_file(), "the source must be copied, not moved");
    }

    #[test]
    fn uses_the_first_h1_when_there_is_no_front_matter() {
        let (directory, store) = store();
        let source = directory.path().join("inbox/agua.md");

        fs::create_dir_all(source.parent().unwrap()).expect("source dir");
        fs::write(&source, "# Water\n\nBody.\n").expect("source file");

        let entry = store.create_from_file(Kind::Term, &source).expect("import");

        assert_eq!(entry.title, "Water");
        assert_eq!(entry.path, "terms/w/water.md");
    }

    #[test]
    fn creates_a_new_markdown_file_when_the_source_is_missing() {
        let (directory, store) = store();
        let source = directory.path().join("inbox/wind.md");

        let entry = store
            .create_from_file(Kind::Concept, &source)
            .expect("create");

        assert_eq!(entry.title, "wind");
        assert_eq!(entry.path, "concepts/w/wind.md");
        assert!(
            store
                .content(&entry)
                .expect("content")
                .starts_with("# wind")
        );
        assert!(!source.exists());
    }

    #[test]
    fn rejects_importing_a_duplicate_title() {
        let (directory, store) = store();
        let source = directory.path().join("water.md");
        fs::write(&source, "# Water\n").expect("source file");

        store
            .create_from_file(Kind::Term, &source)
            .expect("first import");
        let error = store
            .create_from_file(Kind::Term, &source)
            .expect_err("duplicate import");

        assert!(matches!(error, Error::Duplicate(..)));
    }
}
