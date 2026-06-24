use crate::agent::{PendingTurn, Session, SessionKind, ModelKind};
use crate::input_commands::{CommandMode, CommandPicker, PickerOutcome};
use crate::ui;
use color_eyre::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind};
use std::time::Duration;
use std::{env, path::PathBuf};
use ratatui::{DefaultTerminal, Frame};
use tui_input::backend::crossterm::EventHandler;
use tui_input::Input;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AppState {
    SessionSelect,
    ModelSelect,
    Chat,
}

pub struct App {
    state: AppState,
    session_index: usize,
    model_index: usize,
    spinner_frame: usize,
    input: Input,
    current_session: Option<Session>,
    history: Vec<Session>,
    active_turn: Option<PendingTurn>,
    command_mode: CommandMode,
    command_picker: Option<CommandPicker>,
}

impl Default for App {
    fn default() -> Self {
        Self {
            state: AppState::SessionSelect,
            session_index: 0,
            model_index: 0,
            spinner_frame: 0,
            input: Input::default(),
            current_session: None,
            history: vec![],
            active_turn: None,
            command_mode: CommandMode::None,
            command_picker: None,
        }
    }
}

impl App {
    pub fn run(mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        loop {
            terminal.draw(|frame| self.render(frame))?;
            self.spinner_frame = self.spinner_frame.wrapping_add(1);
            if event::poll(Duration::from_millis(50))? {
                if let Some(event) = Self::read_key_event()? {
                    match self.state {
                        AppState::SessionSelect => self.handle_session_select(event),
                        AppState::ModelSelect => self.handle_model_select(event),
                        AppState::Chat => {
                            if self.handle_chat(event) {
                                return Ok(());
                            }
                        }
                    }
                }
            } else {
                self.tick_active_turn();
                continue;
            }

            if self.active_turn.is_some() {
                self.tick_active_turn();
            }
        }
    }

    fn read_key_event() -> Result<Option<KeyEvent>> {
        let event = event::read()?;
        Ok(match event {
            Event::Key(key) if matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) => {
                Some(key)
            }
            _ => None,
        })
    }

    fn handle_session_select(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.session_index = self.session_index.saturating_sub(1);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.session_index = (self.session_index + 1).min(SessionKind::ALL.len() - 1);
            }
            KeyCode::Enter => {
                self.state = AppState::ModelSelect;
            }
            KeyCode::Char('q') => {}
            _ => {}
        }
    }

    fn handle_model_select(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.model_index = self.model_index.saturating_sub(1);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.model_index = (self.model_index + 1).min(ModelKind::ALL.len() - 1);
            }
            KeyCode::Enter => {
                let session_kind = SessionKind::ALL[self.session_index];
                let model_kind = ModelKind::ALL[self.model_index];
                self.current_session = Some(Session::new(session_kind, model_kind));
                self.input = Input::default();
                self.state = AppState::Chat;
            }
            KeyCode::Char('q') => {}
            _ => {}
        }
    }

    fn handle_chat(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Char('q') => return true,
            KeyCode::Char('c') if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
                if let Some(turn) = self.active_turn.as_mut() {
                    turn.request_interrupt();
                }
            }
            KeyCode::Esc if self.command_picker.is_some() => {
                self.command_mode = CommandMode::None;
                self.command_picker = None;
            }
            KeyCode::Char('@') => {
                self.command_mode = CommandMode::Files;
                self.command_picker = Some(CommandPicker::files(current_dir()));
            }
            KeyCode::Char('/') => {
                self.command_mode = CommandMode::Actions;
                self.command_picker = Some(CommandPicker::actions());
            }
            KeyCode::Enter => {
                if matches!(self.command_mode, CommandMode::Files | CommandMode::Actions) {
                    if let Some(picker) = self.command_picker.as_mut() {
                        let outcome = picker.activate(&mut self.input);
                        if matches!(outcome, PickerOutcome::Close) {
                            self.command_mode = CommandMode::None;
                            self.command_picker = None;
                        }
                    }
                } else {
                    let content = self.input.value().trim().to_owned();
                    if !content.is_empty() {
                        if let Some(session) = self.current_session.as_mut() {
                            self.active_turn = session.begin_turn(&content);
                        }
                        self.input.reset();
                    }
                }
            }
            KeyCode::Up | KeyCode::Char('k') if self.command_picker.is_some() => {
                if let Some(picker) = self.command_picker.as_mut() {
                    picker.move_up();
                }
            }
            KeyCode::Down | KeyCode::Char('j') if self.command_picker.is_some() => {
                if let Some(picker) = self.command_picker.as_mut() {
                    picker.move_down();
                }
            }
            KeyCode::Backspace if self.command_picker.is_some() => {
                if let Some(picker) = self.command_picker.as_mut() {
                    picker.pop_filter_char();
                }
            }
            KeyCode::Char(ch) if self.command_picker.is_some() => {
                if let Some(picker) = self.command_picker.as_mut() {
                    picker.push_filter_char(ch);
                }
            }
            KeyCode::Backspace if self.command_mode != CommandMode::None => {
                self.input.handle_event(&Event::Key(key));
            }
            _ => {
                if matches!(self.command_mode, CommandMode::None) {
                    self.input.handle_event(&Event::Key(key));
                }
            }
        }
        false
    }

    fn tick_active_turn(&mut self) {
        let Some(turn) = self.active_turn.as_mut() else {
            return;
        };
        let Some(session) = self.current_session.as_mut() else {
            self.active_turn = None;
            return;
        };
        if turn.tick(session) {
            self.history.push(session.clone());
            self.active_turn = None;
        }
    }

    fn render(&self, frame: &mut Frame) {
        match self.state {
            AppState::SessionSelect => ui::render_session_select(frame, self.session_index),
            AppState::ModelSelect => ui::render_model_select(frame, self.session_index, self.model_index),
            AppState::Chat => ui::render_chat(
                frame,
                self.current_session.as_ref(),
                &self.history,
                &self.input,
                self.command_mode,
                self.command_picker.as_ref(),
                self.spinner_frame,
            ),
        }
    }
}

fn current_dir() -> PathBuf {
    env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    #[test]
    fn escape_closes_open_command_picker() {
        let mut app = App::default();
        app.state = AppState::Chat;
        app.command_mode = CommandMode::Files;
        app.command_picker = Some(CommandPicker::actions());

        let should_quit = app.handle_chat(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));

        assert!(!should_quit);
        assert_eq!(app.command_mode, CommandMode::None);
        assert!(app.command_picker.is_none());
    }

    #[test]
    fn command_mode_accepts_action_picker_visibility() {
        let mut app = App::default();
        app.state = AppState::Chat;
        app.command_mode = CommandMode::Actions;
        app.command_picker = Some(CommandPicker::actions());

        let should_quit = app.handle_chat(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));

        assert!(!should_quit);
        assert_eq!(app.command_mode, CommandMode::None);
    }

    #[test]
    fn ctrl_c_requests_interrupt_on_active_turn() {
        let mut app = App::default();
        app.state = AppState::Chat;
        app.current_session = Some(Session::new(SessionKind::Coder, ModelKind::HermesCode));
        app.active_turn = app
            .current_session
            .as_mut()
            .and_then(|session| session.begin_turn("/run sleep 2"));

        let handled = app.handle_chat(KeyEvent::new(
            KeyCode::Char('c'),
            KeyModifiers::CONTROL,
        ));

        assert!(!handled);
        assert!(app.active_turn.as_ref().is_some());
        assert!(
            app.active_turn
                .as_ref()
                .map(|turn| turn.is_interrupted())
                .unwrap_or(false),
            "expected ctrl+c to mark the active turn as interrupted"
        );
    }
}
