use crate::agent::{ChoiceGroupState, FocusTarget, PendingTurn, Session, SessionKind, ModelKind};
use crate::input_commands::{CommandMode, CommandPicker, PickerOutcome};
use crate::ui;
use color_eyre::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, MouseEventKind};
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
    choice_group: Option<ChoiceGroupState>,
    follow_transcript: bool,
    transcript_scroll: usize,
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
            choice_group: None,
            follow_transcript: true,
            transcript_scroll: 0,
        }
    }
}

impl App {
    pub fn run(mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        loop {
            terminal.draw(|frame| self.render(frame))?;
            self.spinner_frame = self.spinner_frame.wrapping_add(1);
            if event::poll(Duration::from_millis(50))? {
                if let Some(event) = Self::read_event()? {
                    match (self.state, event) {
                        (AppState::SessionSelect, Event::Key(key)) => self.handle_session_select(key),
                        (AppState::ModelSelect, Event::Key(key)) => self.handle_model_select(key),
                        (AppState::Chat, Event::Key(key)) => {
                            if self.handle_chat(key) {
                                return Ok(());
                            }
                        }
                        (AppState::Chat, Event::Mouse(mouse)) => self.handle_mouse(mouse),
                        _ => {}
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

    fn read_event() -> Result<Option<Event>> {
        let event = event::read()?;
        Ok(match event {
            Event::Key(key) if matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) => {
                Some(Event::Key(key))
            }
            Event::Mouse(mouse) => Some(Event::Mouse(mouse)),
            _ => None,
        })
    }

    fn handle_mouse(&mut self, mouse: crossterm::event::MouseEvent) {
        if self.choice_group.is_some() || self.command_picker.is_some() {
            return;
        }
        match mouse.kind {
            MouseEventKind::ScrollUp => {
                self.follow_transcript = false;
                self.transcript_scroll = self.transcript_scroll.saturating_add(1);
            }
            MouseEventKind::ScrollDown => {
                self.follow_transcript = false;
                self.transcript_scroll = self.transcript_scroll.saturating_sub(1);
            }
            _ => {}
        }
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
            KeyCode::Up | KeyCode::Char('k') if self.choice_group.is_some() => {
                if let Some(choice_group) = self.choice_group.as_mut() {
                    choice_group.move_focus_up();
                }
            }
            KeyCode::Down | KeyCode::Char('j') if self.choice_group.is_some() => {
                if let Some(choice_group) = self.choice_group.as_mut() {
                    choice_group.move_focus_down();
                }
            }
            KeyCode::Char(' ') if self.choice_group.is_some() => {
                if let Some(choice_group) = self.choice_group.as_mut() {
                    if let FocusTarget::Question {
                        question_index,
                        option_index,
                    } = choice_group.focus
                    {
                        choice_group.toggle_selected(question_index, option_index);
                    }
                }
            }
            KeyCode::Enter if self.choice_group.is_some() => {
                if let Some(choice_group) = self.choice_group.as_ref() {
                    if matches!(choice_group.focus, FocusTarget::Submit) {
                        let answers = choice_group.selected_answers();
                        let payload = crate::agent::serialize_choice_answers(&answers);
                        if let Some(session) = self.current_session.as_mut() {
                            session.messages.push(crate::agent::Message::user(payload));
                        }
                        self.choice_group = None;
                    }
                }
            }
            KeyCode::Up | KeyCode::Char('k')
                if self.command_picker.is_none() && self.choice_group.is_none() =>
            {
                self.follow_transcript = false;
                self.transcript_scroll = self.transcript_scroll.saturating_add(1);
            }
            KeyCode::Down | KeyCode::Char('j')
                if self.command_picker.is_none() && self.choice_group.is_none() =>
            {
                self.follow_transcript = false;
                self.transcript_scroll = self.transcript_scroll.saturating_sub(1);
            }
            KeyCode::Char('c') if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
                if let Some(turn) = self.active_turn.as_mut() {
                    turn.request_interrupt();
                }
            }
            KeyCode::Esc if self.command_picker.is_some() => {
                self.command_mode = CommandMode::None;
                self.command_picker = None;
            }
            KeyCode::Char('t') => {
                self.follow_transcript = !self.follow_transcript;
                if self.follow_transcript {
                    self.transcript_scroll = 0;
                }
            }
            KeyCode::Char('@') => {
                self.command_mode = CommandMode::Files;
                self.command_picker = Some(CommandPicker::files(current_dir()));
            }
            KeyCode::Char('/') => {
                self.command_mode = CommandMode::Actions;
                self.command_picker = Some(CommandPicker::actions());
            }
            KeyCode::Enter if self.choice_group.is_none() => {
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
            if self.choice_group.is_none() {
                if let Some(choice_group) = session.take_pending_choice_group() {
                    self.choice_group = Some(crate::agent::ChoiceGroupState::new(choice_group));
                }
            }
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
                self.choice_group.as_ref(),
                self.spinner_frame,
                self.follow_transcript,
                self.transcript_scroll,
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
    use crate::agent::{ChoiceGroupBlock, ChoiceGroupState, ChoiceMode, ChoiceOption, ChoiceQuestion};
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};

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
    fn scrolling_transcript_disables_auto_follow() {
        let mut app = App::default();
        app.state = AppState::Chat;

        let handled = app.handle_chat(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));

        assert!(!handled);
        assert!(!app.follow_transcript);
        assert_eq!(app.transcript_scroll, 1);
    }

    #[test]
    fn mouse_wheel_scrolls_transcript() {
        let mut app = App::default();
        app.state = AppState::Chat;

        app.handle_mouse(MouseEvent {
            kind: MouseEventKind::ScrollUp,
            column: 0,
            row: 0,
            modifiers: KeyModifiers::NONE,
        });

        assert!(!app.follow_transcript);
        assert_eq!(app.transcript_scroll, 1);
    }

    #[test]
    fn mouse_wheel_is_ignored_while_choice_group_is_open() {
        let mut app = App::default();
        app.state = AppState::Chat;
        app.choice_group = Some(ChoiceGroupState::new(ChoiceGroupBlock {
            title: "Pick".to_string(),
            questions: vec![ChoiceQuestion {
                id: "mode".to_string(),
                mode: ChoiceMode::Single,
                prompt: "Mode?".to_string(),
                options: vec![
                    ChoiceOption { id: "fast".to_string(), label: "Fast".to_string() },
                    ChoiceOption { id: "safe".to_string(), label: "Safe".to_string() },
                ],
            }],
            submit_label: "Submit".to_string(),
        }));

        app.handle_mouse(MouseEvent {
            kind: MouseEventKind::ScrollUp,
            column: 0,
            row: 0,
            modifiers: KeyModifiers::NONE,
        });

        assert!(app.follow_transcript);
        assert_eq!(app.transcript_scroll, 0);
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

    #[test]
    fn choice_group_submit_only_closes_when_focus_is_on_submit() {
        let mut app = App::default();
        app.state = AppState::Chat;
        app.current_session = Some(Session::new(SessionKind::Coder, ModelKind::HermesCode));
        app.choice_group = Some(ChoiceGroupState::new(ChoiceGroupBlock {
            title: "Pick".to_string(),
            questions: vec![
                ChoiceQuestion {
                    id: "mode".to_string(),
                    mode: ChoiceMode::Single,
                    prompt: "Mode?".to_string(),
                    options: vec![
                        ChoiceOption { id: "fast".to_string(), label: "Fast".to_string() },
                        ChoiceOption { id: "safe".to_string(), label: "Safe".to_string() },
                    ],
                },
                ChoiceQuestion {
                    id: "tags".to_string(),
                    mode: ChoiceMode::Multi,
                    prompt: "Tags?".to_string(),
                    options: vec![
                        ChoiceOption { id: "one".to_string(), label: "One".to_string() },
                        ChoiceOption { id: "two".to_string(), label: "Two".to_string() },
                    ],
                },
            ],
            submit_label: "Submit".to_string(),
        }));

        assert!(matches!(
            app.choice_group.as_ref().unwrap().focus,
            FocusTarget::Question {
                question_index: 0,
                option_index: 0,
            }
        ));
        assert!(!app.handle_chat(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE)));
        assert!(!app.handle_chat(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)));
        assert!(!app.handle_chat(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE)));
        assert!(!app.handle_chat(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)));
        assert!(!app.handle_chat(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)));
        assert!(!app.handle_chat(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)));
        let handled = app.handle_chat(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

        assert!(!handled);
        assert!(app.choice_group.is_none());
        assert!(app
            .current_session
            .as_ref()
            .unwrap()
            .messages
            .iter()
            .any(|message| message.content.contains("\"answers\"")));
        assert!(app
            .current_session
            .as_ref()
            .unwrap()
            .messages
            .iter()
            .any(|message| message.content.contains("\"mode\"")));
    }

    #[test]
    fn tick_active_turn_auto_opens_choice_group_from_assistant_payload() {
        let mut app = App::default();
        app.state = AppState::Chat;
        app.current_session = Some(Session::new(SessionKind::Coder, ModelKind::HermesCode));
        app.active_turn = app
            .current_session
            .as_mut()
            .and_then(|session| session.begin_turn("/choice plan the next step"));

        while app.active_turn.is_some() {
            app.tick_active_turn();
        }

        assert!(app.choice_group.is_some());
        assert!(
            app.current_session
                .as_mut()
                .and_then(|session| session.take_pending_choice_group())
                .is_none()
        );
    }

    #[test]
    fn tick_active_turn_ignores_non_assistant_choice_payloads() {
        let mut app = App::default();
        app.state = AppState::Chat;
        app.current_session = Some(Session::new(SessionKind::Coder, ModelKind::HermesCode));
        app.active_turn = app
            .current_session
            .as_mut()
            .and_then(|session| session.begin_turn("plan the next step"));
        if let Some(session) = app.current_session.as_mut() {
            let _ = session.take_pending_choice_group();
        }

        while app.active_turn.is_some() {
            app.tick_active_turn();
        }

        assert!(app.choice_group.is_none());
    }
}
