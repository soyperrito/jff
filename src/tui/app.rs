use ratatui::widgets::ListState;

use crate::config::Config;
use crate::model::Entry;
use crate::store::{Related, Store};

use super::diagram::DiagramRenderer;
use super::image::ImageManager;
use super::markdown;
use super::theme::Theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Content,
    Related,
}

impl Focus {
    pub fn label(self) -> &'static str {
        match self {
            Self::Content => "content",
            Self::Related => "related",
        }
    }
}

pub struct App {
    pub store: Store,
    pub config: Config,
    pub theme: Theme,
    pub current: Entry,
    pub content: String,
    pub rendered: markdown::Document,
    pub images: Option<ImageManager>,
    pub diagrams: DiagramRenderer,
    pub related: Vec<Related>,
    pub history: Vec<Entry>,
    pub focus: Focus,
    pub scroll: u16,
    pub max_scroll: u16,
    pub total_lines: u16,
    pub selected: usize,
    pub related_state: ListState,
    pub status: Option<String>,
}

impl App {
    pub fn new(store: Store, config: Config, entry: Entry, images: Option<ImageManager>) -> Self {
        let theme = Theme::from_config(&config);
        let mut app = Self {
            store,
            config,
            theme,
            current: entry.clone(),
            content: String::new(),
            rendered: markdown::Document::default(),
            images,
            diagrams: DiagramRenderer::new(),
            related: Vec::new(),
            history: Vec::new(),
            focus: Focus::Content,
            scroll: 0,
            max_scroll: 0,
            total_lines: 0,
            selected: 0,
            related_state: ListState::default(),
            status: None,
        };

        app.load(entry);
        app
    }

    pub fn load(&mut self, entry: Entry) {
        self.current = entry;

        match self.store.content(&self.current) {
            Ok(content) => self.content = content,
            Err(error) => {
                self.content = format!(
                    "# {}\n\nUnable to read this file.\n\n{error}\n",
                    self.current.title
                );
                self.status = Some(format!("read error: {error}"));
            }
        }

        match self.store.related(&self.current.reference) {
            Ok(related) => self.related = related,
            Err(error) => {
                self.related = Vec::new();
                self.status = Some(format!("related: {error}"));
            }
        }

        self.rendered = markdown::render(&self.content, &self.theme);
        self.scroll = 0;
        self.max_scroll = 0;
        self.total_lines = 0;
        self.selected = 0;
        self.related_state.select(if self.related.is_empty() {
            None
        } else {
            Some(0)
        });
    }

    pub fn reload(&mut self) {
        self.status = None;

        if let Some(images) = self.images.as_mut() {
            images.clear();
        }

        let current = self.current.clone();
        self.load(current);

        if self.status.is_none() {
            self.status = Some("reloaded".to_string());
        }
    }

    /// Go back to the previously viewed entry. Returns false when there is no
    /// history left.
    pub fn back(&mut self) -> bool {
        let Some(entry) = self.history.pop() else {
            return false;
        };

        self.status = None;
        self.load(entry);
        self.focus = Focus::Content;

        if self.status.is_none() {
            self.status = Some("back".to_string());
        }

        true
    }

    pub fn open_selected(&mut self) {
        let Some(item) = self.related.get(self.selected) else {
            return;
        };

        let Some(target) = item.target.clone() else {
            self.status = Some("this related link cannot be resolved".to_string());
            return;
        };

        let label = item.heading.clone().unwrap_or_else(|| target.title.clone());

        self.history.push(self.current.clone());
        self.status = None;
        self.load(target);
        self.focus = Focus::Content;

        if self.status.is_none() {
            self.status = Some(format!("opened {label}"));
        }
    }

    pub fn toggle_focus(&mut self) {
        self.focus = match self.focus {
            Focus::Content => Focus::Related,
            Focus::Related => Focus::Content,
        };
    }

    pub fn scroll_up(&mut self, amount: u16) {
        self.scroll = self.scroll.saturating_sub(amount);
    }

    pub fn scroll_down(&mut self, amount: u16) {
        self.scroll = self.scroll.saturating_add(amount).min(self.max_scroll);
    }

    pub fn select_prev(&mut self) {
        if self.related.is_empty() {
            return;
        }

        self.selected = self.selected.saturating_sub(1);
        self.related_state.select(Some(self.selected));
    }

    pub fn select_next(&mut self) {
        if self.related.is_empty() {
            return;
        }

        self.selected = (self.selected + 1).min(self.related.len() - 1);
        self.related_state.select(Some(self.selected));
    }

    pub fn select_first(&mut self) {
        if self.related.is_empty() {
            return;
        }

        self.selected = 0;
        self.related_state.select(Some(0));
    }

    pub fn select_last(&mut self) {
        if self.related.is_empty() {
            return;
        }

        self.selected = self.related.len() - 1;
        self.related_state.select(Some(self.selected));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Kind;
    use std::fs;
    use tempfile::TempDir;

    fn app_with_link() -> (TempDir, App, Entry, Entry) {
        let directory = TempDir::new().expect("temp dir");
        let store = Store::new(directory.path().join(".jff"));
        let water = store.create(Kind::Term, "Water").expect("term");
        let concept = store
            .create(Kind::Concept, "Water Pollution")
            .expect("concept");

        let term_path = water.abs_path(store.root());
        let mut contents = store.content(&water).expect("content");
        contents.push_str("\n[more:concept](../../concepts/w/water_pollution.md)\n");
        fs::write(&term_path, contents).expect("link");

        let app = App::new(store, Config::default(), water.clone(), None);
        (directory, app, water, concept)
    }

    #[test]
    fn opens_a_related_entry_and_goes_back() {
        let (_directory, mut app, water, concept) = app_with_link();

        assert_eq!(app.related.len(), 1);
        assert_eq!(app.focus, Focus::Content);

        app.toggle_focus();
        assert_eq!(app.focus, Focus::Related);

        app.open_selected();
        assert_eq!(app.current.reference, concept.reference);
        assert_eq!(app.history.len(), 1);
        assert_eq!(app.focus, Focus::Content);

        assert!(app.back());
        assert_eq!(app.current.reference, water.reference);
        assert!(!app.back());
    }

    #[test]
    fn selection_stays_within_bounds() {
        let (_directory, mut app, _water, _concept) = app_with_link();

        app.select_prev();
        assert_eq!(app.selected, 0);

        app.select_next();
        assert_eq!(app.selected, 0);

        app.select_last();
        assert_eq!(app.selected, 0);
    }

    #[test]
    fn reload_reads_the_file_again() {
        let (_directory, mut app, _water, _concept) = app_with_link();
        let path = app.current.abs_path(app.store.root());

        fs::write(&path, "# Water\n\nUpdated body.\n").expect("rewrite");
        app.reload();

        assert!(app.content.contains("Updated body."));
    }
}
