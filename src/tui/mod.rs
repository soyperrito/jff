pub mod app;
pub mod diagram;
pub mod image;
pub mod markdown;
pub mod theme;
pub mod ui;

use std::io::Stdout;

use anyhow::Result;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::crossterm::cursor::Hide;
use ratatui::crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind,
    KeyModifiers, MouseEvent, MouseEventKind,
};
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};

use crate::config::Config;
use crate::editor;
use crate::model::Entry;
use crate::store::Store;

use app::{App, Focus};
use image::ImageManager;

pub fn run(store: Store, config: Config, entry: Entry) -> Result<()> {
    let images = if config.images.enable {
        Some(ImageManager::new(&config.images))
    } else {
        None
    };
    let mut app = App::new(store, config, entry, images);
    let mut session = Session::new()?;

    loop {
        session.terminal.draw(|frame| ui::draw(frame, &mut app))?;

        match event::read()? {
            Event::Key(key) if key.kind == KeyEventKind::Press => match handle_key(&mut app, key) {
                Outcome::Quit => break,
                Outcome::Edit => {
                    session.suspend()?;
                    let result = editor::open(&app.store, &app.config, &app.current, None);
                    session.resume()?;

                    match result {
                        Ok(()) => {
                            app.reload();

                            if let Err(error) = crate::git::autopush(
                                app.store.root(),
                                &app.config,
                                &format!("edit {}", app.current.reference),
                            ) {
                                app.status = Some(format!("git: {error}"));
                            }
                        }
                        Err(error) => app.status = Some(format!("editor: {error}")),
                    }
                }
                Outcome::Continue => {}
            },
            Event::Mouse(mouse) => handle_mouse(&mut app, mouse),
            _ => {}
        }
    }

    Ok(())
}

enum Outcome {
    Continue,
    Quit,
    Edit,
}

fn handle_key(app: &mut App, key: KeyEvent) -> Outcome {
    app.status = None;

    let control = key.modifiers.contains(KeyModifiers::CONTROL);

    match key.code {
        KeyCode::Char('c') if control => return Outcome::Quit,
        KeyCode::Char('q') => return Outcome::Quit,
        KeyCode::Char('e') => return Outcome::Edit,
        KeyCode::Char('r') => {
            app.reload();
            return Outcome::Continue;
        }
        KeyCode::Char('?') => {
            app.status = Some(
                "Tab: focus · j/k or arrows: move · Enter: open · e: edit · r: reload · Esc: back · q: quit"
                    .to_string(),
            );
            return Outcome::Continue;
        }
        KeyCode::Char('d') if control => {
            app.scroll_down(10);
            return Outcome::Continue;
        }
        KeyCode::Char('u') if control => {
            app.scroll_up(10);
            return Outcome::Continue;
        }
        KeyCode::Tab | KeyCode::BackTab => {
            app.toggle_focus();
            app.status = Some(format!("focus: {}", app.focus.label()));
            return Outcome::Continue;
        }
        KeyCode::Esc | KeyCode::Backspace => {
            if !app.back() {
                return Outcome::Quit;
            }

            return Outcome::Continue;
        }
        _ => {}
    }

    match app.focus {
        Focus::Content => match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                app.scroll_up(1);
                scroll_status(app);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                app.scroll_down(1);
                scroll_status(app);
            }
            KeyCode::PageUp => {
                app.scroll_up(10);
                scroll_status(app);
            }
            KeyCode::PageDown | KeyCode::Char(' ') => {
                app.scroll_down(10);
                scroll_status(app);
            }
            KeyCode::Home | KeyCode::Char('g') => {
                app.scroll = 0;
                scroll_status(app);
            }
            KeyCode::End | KeyCode::Char('G') => {
                app.scroll = app.max_scroll;
                scroll_status(app);
            }
            KeyCode::Enter => app.focus = Focus::Related,
            _ => {}
        },
        Focus::Related => match key.code {
            KeyCode::Up | KeyCode::Char('k') => app.select_prev(),
            KeyCode::Down | KeyCode::Char('j') => app.select_next(),
            KeyCode::Home | KeyCode::Char('g') => app.select_first(),
            KeyCode::End | KeyCode::Char('G') => app.select_last(),
            KeyCode::Enter => app.open_selected(),
            KeyCode::Left | KeyCode::Char('h') if !app.back() => {
                app.focus = Focus::Content;
            }
            _ => {}
        },
    }

    Outcome::Continue
}

fn handle_mouse(app: &mut App, mouse: MouseEvent) {
    app.status = None;

    match mouse.kind {
        MouseEventKind::ScrollUp => match app.focus {
            Focus::Content => {
                app.scroll_up(3);
                scroll_status(app);
            }
            Focus::Related => app.select_prev(),
        },
        MouseEventKind::ScrollDown => match app.focus {
            Focus::Content => {
                app.scroll_down(3);
                scroll_status(app);
            }
            Focus::Related => app.select_next(),
        },
        _ => {}
    }
}

fn scroll_status(app: &mut App) {
    app.status = Some(if app.total_lines == 0 {
        "line 0/0".to_string()
    } else {
        format!(
            "line {}/{}",
            app.scroll.saturating_add(1).min(app.total_lines),
            app.total_lines
        )
    });
}

struct Session {
    terminal: Terminal<CrosstermBackend<Stdout>>,
    active: bool,
}

impl Session {
    fn new() -> Result<Self> {
        enable_raw_mode()?;

        let mut stdout = std::io::stdout();

        if let Err(error) = execute!(stdout, EnterAlternateScreen, Hide, EnableMouseCapture) {
            let _ = execute!(stdout, LeaveAlternateScreen, DisableMouseCapture);
            let _ = disable_raw_mode();
            return Err(error.into());
        }

        let backend = CrosstermBackend::new(stdout);
        let terminal = match Terminal::new(backend) {
            Ok(terminal) => terminal,
            Err(error) => {
                let _ = execute!(std::io::stdout(), LeaveAlternateScreen, DisableMouseCapture);
                let _ = disable_raw_mode();
                return Err(error.into());
            }
        };

        let mut session = Self {
            terminal,
            active: true,
        };
        session.terminal.clear()?;
        Ok(session)
    }

    fn suspend(&mut self) -> Result<()> {
        if !self.active {
            return Ok(());
        }

        disable_raw_mode()?;
        execute!(
            self.terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture
        )?;
        self.terminal.show_cursor()?;
        self.active = false;

        Ok(())
    }

    fn resume(&mut self) -> Result<()> {
        if self.active {
            return Ok(());
        }

        enable_raw_mode()?;
        execute!(
            self.terminal.backend_mut(),
            EnterAlternateScreen,
            Hide,
            EnableMouseCapture
        )?;
        self.active = true;
        self.terminal.clear()?;

        Ok(())
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = disable_raw_mode();

        if self.active {
            let _ = execute!(
                self.terminal.backend_mut(),
                LeaveAlternateScreen,
                DisableMouseCapture
            );
            let _ = self.terminal.show_cursor();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Kind;
    use crate::store::Store;
    use ratatui::backend::TestBackend;
    use std::fs;
    use tempfile::TempDir;

    fn app_with_long_note() -> (TempDir, App) {
        let directory = TempDir::new().expect("temp dir");
        let source = directory.path().join("long.md");
        let mut contents = String::from("# Long note\n\n");

        for index in 1..=80 {
            contents.push_str(&format!("Line {index:03} of the long note.\n\n"));
        }

        fs::write(&source, contents).expect("write note");
        let store = Store::new(directory.path().join(".jff"));
        let entry = store
            .create_from_file(Kind::Term, &source)
            .expect("create note");
        let app = App::new(store, Config::default(), entry, None);

        (directory, app)
    }

    fn draw(app: &mut App) -> Terminal<TestBackend> {
        let mut terminal = Terminal::new(TestBackend::new(80, 20)).expect("terminal");
        terminal.draw(|frame| ui::draw(frame, app)).expect("draw");
        terminal
    }

    #[test]
    fn keyboard_scrolls_the_content() {
        let (_directory, mut app) = app_with_long_note();
        draw(&mut app);

        assert!(app.max_scroll > 0);

        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE),
        );
        assert_eq!(app.scroll, 1);

        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE),
        );
        assert_eq!(app.scroll, 11);

        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Char('G'), KeyModifiers::NONE),
        );
        assert_eq!(app.scroll, app.max_scroll);

        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE),
        );
        assert_eq!(app.scroll, 0);
    }

    #[test]
    fn content_draws_a_scrollbar() {
        let (_directory, mut app) = app_with_long_note();
        let terminal = draw(&mut app);
        let text: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect();

        assert!(text.contains('█'), "scrollbar thumb should be visible");
    }

    #[test]
    fn mouse_wheel_scrolls_the_content() {
        let (_directory, mut app) = app_with_long_note();
        draw(&mut app);

        handle_mouse(
            &mut app,
            MouseEvent {
                kind: MouseEventKind::ScrollDown,
                column: 10,
                row: 5,
                modifiers: KeyModifiers::NONE,
            },
        );
        assert_eq!(app.scroll, 3);

        handle_mouse(
            &mut app,
            MouseEvent {
                kind: MouseEventKind::ScrollUp,
                column: 10,
                row: 5,
                modifiers: KeyModifiers::NONE,
            },
        );
        assert_eq!(app.scroll, 0);
    }

    #[test]
    fn tab_switches_to_the_related_pane() {
        let (_directory, mut app) = app_with_long_note();
        draw(&mut app);

        handle_key(&mut app, KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));

        assert_eq!(app.focus, Focus::Related);
    }
}
