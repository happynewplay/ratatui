use crate::agent::{
    ChoiceGroupState, ClaudeCodeDashboardState, FocusTarget, PendingTurn, Session, SessionKind,
    ModelKind, ToolEvent, ToolKind, ToolStatus,
};
use crate::input_commands::{CommandMode, CommandPicker, PickerOutcome, workspace_relative_path};
use crate::ui;
use color_eyre::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, MouseEventKind};
use std::path::PathBuf;
use std::time::Duration;
use std::{env, fs, process::Command as ProcessCommand};
use ratatui::{DefaultTerminal, Frame};
use tui_input::backend::crossterm::EventHandler;
use tui_input::Input;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AppState {
    SessionSelect,
    ModelSelect,
    Chat,
    ClaudeCodeDashboard,
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
    claude_code_state: ClaudeCodeDashboardState,
    follow_transcript: bool,
    transcript_scroll: usize,
    needs_terminal_clear: bool,
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
            claude_code_state: ClaudeCodeDashboardState::new(),
            follow_transcript: true,
            transcript_scroll: 0,
            needs_terminal_clear: false,
        }
    }
}

impl App {
    pub fn run(mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        loop {
            if self.needs_terminal_clear {
                terminal.clear()?;
                self.needs_terminal_clear = false;
            }
            let mut max_transcript_scroll = 0;
            terminal.draw(|frame| {
                max_transcript_scroll = self.render(frame);
            })?;
            self.sync_transcript_scroll(max_transcript_scroll);
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
                        (AppState::ClaudeCodeDashboard, Event::Key(key)) => {
                            if self.handle_claude_code(key) {
                                return Ok(());
                            }
                        }
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
                self.state = if self.session_index == SessionKind::ALL.len() - 1 {
                    AppState::ClaudeCodeDashboard
                } else {
                    AppState::ModelSelect
                };
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
            KeyCode::Esc if self.choice_group.is_some() => {
                self.choice_group = None;
            }
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
                        self.follow_transcript = true;
                        self.transcript_scroll = 0;
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
            KeyCode::PageUp if self.command_picker.is_none() && self.choice_group.is_none() => {
                self.follow_transcript = false;
                self.transcript_scroll = self.transcript_scroll.saturating_add(5);
            }
            KeyCode::PageDown if self.command_picker.is_none() && self.choice_group.is_none() => {
                self.follow_transcript = false;
                self.transcript_scroll = self.transcript_scroll.saturating_sub(5);
            }
            KeyCode::Home if self.command_picker.is_none() && self.choice_group.is_none() => {
                self.follow_transcript = false;
                self.transcript_scroll = usize::MAX;
            }
            KeyCode::End if self.command_picker.is_none() && self.choice_group.is_none() => {
                self.follow_transcript = true;
                self.transcript_scroll = 0;
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
                        self.follow_transcript = true;
                        self.transcript_scroll = 0;
                        self.input.reset();
                        self.needs_terminal_clear = true;
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
                if self.choice_group.is_none() && matches!(self.command_mode, CommandMode::None) {
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
        let completed = turn.tick(session);
        self.follow_transcript = true;
        self.transcript_scroll = 0;
        if completed {
            if self.choice_group.is_none() {
                if let Some(choice_group) = session.take_pending_choice_group() {
                    self.choice_group = Some(crate::agent::ChoiceGroupState::new(choice_group));
                    self.follow_transcript = true;
                    self.transcript_scroll = 0;
                }
            }
            self.active_turn = None;
        }
    }

    fn render(&self, frame: &mut Frame) -> usize {
        match self.state {
            AppState::SessionSelect => {
                ui::render_session_select(frame, self.session_index);
                0
            }
            AppState::ModelSelect => {
                ui::render_model_select(frame, self.session_index, self.model_index);
                0
            }
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
            AppState::ClaudeCodeDashboard => ui::render_claude_code_dashboard(
                frame,
                &self.claude_code_state,
            ),
        }
    }

    fn handle_claude_code(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                self.state = AppState::SessionSelect;
                false
            }
            KeyCode::Tab => {
                self.claude_code_state.focus_next();
                false
            }
            KeyCode::Enter => {
                match self.claude_code_state.focused {
                    ToolKind::Read => self.run_workspace_read(),
                    ToolKind::Write => self.run_workspace_write(),
                    ToolKind::Execute => self.run_workspace_execute(),
                }
                false
            }
            KeyCode::Char('r') => {
                self.run_workspace_read();
                false
            }
            KeyCode::Char('w') => {
                self.run_workspace_write();
                false
            }
            KeyCode::Char('e') => {
                self.run_workspace_execute();
                false
            }
            _ => false,
        }
    }

    fn apply_tool_event(&mut self, event: ToolEvent) {
        let target = event.target.clone();
        match event.kind {
            ToolKind::Read => self.claude_code_state.read = event.clone(),
            ToolKind::Write => self.claude_code_state.write = event.clone(),
            ToolKind::Execute => self.claude_code_state.execute = event.clone(),
        }
        self.claude_code_state.activity_log.push(event);
        self.input = Input::from(target);
    }

    fn run_workspace_read(&mut self) {
        let target = self.input.value().trim();
        let root = current_dir();
        let path = root.join(if target.is_empty() { "src/main.rs" } else { target });
        let mut event = ToolEvent::new(ToolKind::Read, path.display().to_string());
        event.status = ToolStatus::Running;
        event.summary = "reading file".to_string();
        self.apply_tool_event(event.clone());

        let summary = match fs::read_to_string(&path) {
            Ok(content) => {
                let snippet = content.lines().next().unwrap_or_default().to_string();
                Some((ToolStatus::Done, format!("preview: {snippet}"), None))
            }
            Err(err) => Some((ToolStatus::Error, String::new(), Some(err.to_string()))),
        };

        if let Some((status, summary, error)) = summary {
            let mut done = ToolEvent::new(ToolKind::Read, path.display().to_string());
            done.status = status;
            done.summary = summary;
            done.error = error;
            self.apply_tool_event(done);
        }
    }

    fn run_workspace_write(&mut self) {
        let target = self.input.value().trim();
        let root = current_dir();
        let path = root.join(if target.is_empty() { "target/new-input-form-claude-code.txt" } else { target });
        let mut event = ToolEvent::new(ToolKind::Write, path.display().to_string());
        event.status = ToolStatus::Running;
        event.summary = "writing file".to_string();
        self.apply_tool_event(event.clone());

        let write_result = workspace_relative_path(&root, &path)
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::PermissionDenied, "path escapes workspace"))
            .and_then(|relative| {
                let absolute = root.join(relative);
                if let Some(parent) = absolute.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::write(&absolute, b"new-input-form claude code mode\n")?;
                Ok(absolute)
            });

        let mut done = ToolEvent::new(ToolKind::Write, path.display().to_string());
        match write_result {
            Ok(absolute) => {
                done.status = ToolStatus::Done;
                done.summary = format!("saved {}", absolute.display());
            }
            Err(err) => {
                done.status = ToolStatus::Error;
                done.error = Some(err.to_string());
            }
        }
        self.apply_tool_event(done);
    }

    fn run_workspace_execute(&mut self) {
        let command = self.input.value().trim().to_string();
        let command = if command.is_empty() {
            "cargo test -p new-input-form --lib app::tests::session_select_lists_claude_code_last_and_enters_dashboard".to_string()
        } else {
            command
        };
        let command_for_exec = command.clone();
        let mut event = ToolEvent::new(ToolKind::Execute, command.clone());
        event.status = ToolStatus::Running;
        event.summary = "running command".to_string();
        self.apply_tool_event(event.clone());

        let output = ProcessCommand::new(if cfg!(windows) { "cmd" } else { "sh" })
            .args(if cfg!(windows) { vec!["/C", command_for_exec.as_str()] } else { vec!["-lc", command_for_exec.as_str()] })
            .output();

        let mut done = ToolEvent::new(ToolKind::Execute, command_for_exec.as_str());
        match output {
            Ok(output) => {
                if output.status.success() {
                    done.status = ToolStatus::Done;
                    done.summary = String::from_utf8_lossy(&output.stdout).trim().to_string();
                    if done.summary.is_empty() {
                        done.summary = "command completed".to_string();
                    }
                } else {
                    done.status = ToolStatus::Error;
                    done.summary = String::from_utf8_lossy(&output.stderr).trim().to_string();
                    done.error = Some(format!("exit status {:?}", output.status.code()));
                }
            }
            Err(err) => {
                done.status = ToolStatus::Error;
                done.error = Some(err.to_string());
            }
        }
        self.apply_tool_event(done);
    }

    fn sync_transcript_scroll(&mut self, max_transcript_scroll: usize) {
        if self.follow_transcript {
            self.transcript_scroll = 0;
        } else {
            self.transcript_scroll = self.transcript_scroll.min(max_transcript_scroll);
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
    use ratatui::{backend::TestBackend, Terminal};
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};

    #[test]
    fn escape_closes_open_command_picker() {
        let mut app = App::default();
        app.state = AppState::Chat;
        app.command_mode = CommandMode::Files;
        app.command_picker = Some(CommandPicker::files("/workspace"));

        let should_quit = app.handle_chat(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));

        assert!(!should_quit);
        assert_eq!(app.command_mode, CommandMode::None);
        assert!(app.command_picker.is_none());
    }

    #[test]
    fn escape_restores_input_after_command_picker() {
        let mut app = App::default();
        app.state = AppState::Chat;
        app.command_mode = CommandMode::Files;
        app.command_picker = Some(CommandPicker::files("/workspace"));

        assert!(!app.handle_chat(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)));
        assert!(app.command_picker.is_none());

        assert!(!app.handle_chat(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE)));
        assert_eq!(app.input.value(), "x");
    }

    #[test]
    fn command_picker_characters_go_to_filter_not_input() {
        let mut app = App::default();
        app.state = AppState::Chat;
        app.command_mode = CommandMode::Files;
        app.command_picker = Some(CommandPicker::files("/workspace"));

        assert!(!app.handle_chat(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE)));
        assert_eq!(app.input.value(), "");
        assert_eq!(
            app.command_picker
                .as_ref()
                .map(|picker| picker.prompt().to_string()),
            Some("l".to_string())
        );
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
    fn page_up_scrolls_transcript_by_pages() {
        let mut app = App::default();
        app.state = AppState::Chat;

        let handled = app.handle_chat(KeyEvent::new(KeyCode::PageUp, KeyModifiers::NONE));

        assert!(!handled);
        assert!(!app.follow_transcript);
        assert_eq!(app.transcript_scroll, 5);
    }

    #[test]
    fn end_returns_transcript_to_auto_follow() {
        let mut app = App::default();
        app.state = AppState::Chat;
        app.follow_transcript = false;
        app.transcript_scroll = 7;

        let handled = app.handle_chat(KeyEvent::new(KeyCode::End, KeyModifiers::NONE));

        assert!(!handled);
        assert!(app.follow_transcript);
        assert_eq!(app.transcript_scroll, 0);
        assert!(app.needs_terminal_clear);
    }

    #[test]
    fn sending_message_returns_transcript_to_auto_follow() {
        let mut app = App::default();
        app.state = AppState::Chat;
        app.current_session = Some(Session::new(SessionKind::Coder, ModelKind::HermesCode));
        app.follow_transcript = false;
        app.transcript_scroll = 9;
        app.input = Input::from("hello");

        let handled = app.handle_chat(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

        assert!(!handled);
        assert!(app.follow_transcript);
        assert_eq!(app.transcript_scroll, 0);
        assert_eq!(app.input.value(), "");
    }

    #[test]
    fn sending_chinese_message_clears_input_before_next_render() {
        let mut app = App::default();
        app.state = AppState::Chat;
        app.current_session = Some(Session::new(SessionKind::Coder, ModelKind::HermesCode));
        app.input = Input::from("发送中文");

        let handled = app.handle_chat(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

        assert!(!handled);
        assert_eq!(app.input.value(), "");
        assert_eq!(app.input.cursor(), 0);
        assert!(app.follow_transcript);
        assert_eq!(app.transcript_scroll, 0);
    }

    #[test]
    fn sending_chinese_message_renders_clean_prompt_on_next_frame() {
        let mut app = App::default();
        app.state = AppState::Chat;
        app.current_session = Some(Session::new(SessionKind::Coder, ModelKind::HermesCode));
        app.input = Input::from("发送中文");

        assert!(!app.handle_chat(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("create terminal");
        terminal
            .draw(|frame| {
                let _ = app.render(frame);
            })
            .expect("draw after submit");

        let buffer = terminal.backend().buffer();
        let rendered = buffer
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();

        assert!(rendered.contains("Prompt"));
        assert!(rendered.contains("Enter send"));
        assert!(!rendered.contains("发送中文"));
    }

    #[test]
    fn sending_chinese_message_requests_terminal_clear() {
        let mut app = App::default();
        app.state = AppState::Chat;
        app.current_session = Some(Session::new(SessionKind::Coder, ModelKind::HermesCode));
        app.input = Input::from("发送中文");

        assert!(!app.handle_chat(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));
        assert!(app.needs_terminal_clear);
    }

    #[test]
    fn submitting_choice_group_returns_transcript_to_auto_follow() {
        let mut app = App::default();
        app.state = AppState::Chat;
        app.current_session = Some(Session::new(SessionKind::Coder, ModelKind::HermesCode));
        app.follow_transcript = false;
        app.transcript_scroll = 9;
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

        assert!(!app.handle_chat(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)));
        assert!(!app.handle_chat(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)));
        assert!(!app.handle_chat(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));

        assert!(app.follow_transcript);
        assert_eq!(app.transcript_scroll, 0);
        assert!(app.choice_group.is_none());
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
    fn sync_transcript_scroll_clamps_out_of_range_offsets() {
        let mut app = App::default();
        app.state = AppState::Chat;
        app.follow_transcript = false;
        app.transcript_scroll = 999;

        app.sync_transcript_scroll(12);

        assert_eq!(app.transcript_scroll, 12);
    }

    #[test]
    fn mouse_wheel_still_moves_after_transcript_scroll_is_clamped() {
        let mut app = App::default();
        app.state = AppState::Chat;
        app.follow_transcript = false;
        app.transcript_scroll = 999;

        app.sync_transcript_scroll(12);
        app.handle_mouse(MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 0,
            row: 0,
            modifiers: KeyModifiers::NONE,
        });

        assert_eq!(app.transcript_scroll, 11);
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
        assert!(!app.handle_chat(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)));
        assert!(!app.handle_chat(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE)));
        assert!(!app.handle_chat(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)));
        assert!(!app.handle_chat(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE)));
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
    fn choice_group_supports_chinese_multi_question_submit_flow() {
        let mut app = App::default();
        app.state = AppState::Chat;
        app.current_session = Some(Session::new(SessionKind::Coder, ModelKind::HermesCode));
        app.choice_group = Some(ChoiceGroupState::new(ChoiceGroupBlock {
            title: "选择".to_string(),
            questions: vec![
                ChoiceQuestion {
                    id: "模式".to_string(),
                    mode: ChoiceMode::Single,
                    prompt: "请选择模式".to_string(),
                    options: vec![
                        ChoiceOption { id: "快速".to_string(), label: "快速".to_string() },
                        ChoiceOption { id: "安全".to_string(), label: "安全".to_string() },
                    ],
                },
                ChoiceQuestion {
                    id: "标签".to_string(),
                    mode: ChoiceMode::Multi,
                    prompt: "请选择标签".to_string(),
                    options: vec![
                        ChoiceOption { id: "中文".to_string(), label: "中文".to_string() },
                        ChoiceOption { id: "混合".to_string(), label: "混合".to_string() },
                    ],
                },
            ],
            submit_label: "提交".to_string(),
        }));

        assert!(!app.handle_chat(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE)));
        assert!(!app.handle_chat(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)));
        assert!(!app.handle_chat(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)));
        assert!(!app.handle_chat(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE)));
        assert!(!app.handle_chat(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)));
        assert!(!app.handle_chat(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE)));
        assert!(!app.handle_chat(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)));

        let handled = app.handle_chat(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

        assert!(!handled);
        assert!(app.choice_group.is_none());
        let session = app.current_session.as_ref().unwrap();
        let last_message = session.messages.last().unwrap();
        assert_eq!(last_message.role, crate::agent::Role::User);
        assert!(last_message.content.contains("\"question_id\":\"模式\""));
        assert!(last_message.content.contains("\"question_id\":\"标签\""));
        assert!(last_message.content.contains("\"selected_ids\":[\"快速\"]"));
        assert!(last_message.content.contains("\"selected_ids\":[\"中文\",\"混合\"]"));
    }

    #[test]
    fn escape_closes_open_choice_group() {
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

        let handled = app.handle_chat(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));

        assert!(!handled);
        assert!(app.choice_group.is_none());
    }

    #[test]
    fn session_select_lists_claude_code_last_and_enters_dashboard() {
        let mut app = App::default();
        app.state = AppState::SessionSelect;
        app.session_index = SessionKind::ALL.len() - 1;

        app.handle_session_select(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

        assert!(matches!(app.state, AppState::ClaudeCodeDashboard));
    }

    #[test]
    fn execute_tool_event_updates_dashboard_state() {
        let mut app = App::default();
        app.state = AppState::ClaudeCodeDashboard;

        app.apply_tool_event(ToolEvent {
            kind: ToolKind::Execute,
            status: ToolStatus::Running,
            target: "cargo test -p new-input-form".to_string(),
            summary: "running".to_string(),
            error: None,
            elapsed_ms: Some(38),
        });

        assert_eq!(app.claude_code_state.execute.status, ToolStatus::Running);
        assert_eq!(app.claude_code_state.activity_log.len(), 1);
        assert_eq!(app.claude_code_state.activity_log[0].target, "cargo test -p new-input-form");
    }

    #[test]
    fn claude_code_dashboard_returns_to_menu_with_escape() {
        let mut app = App::default();
        app.state = AppState::ClaudeCodeDashboard;

        let should_quit = app.handle_claude_code(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));

        assert!(!should_quit);
        assert!(matches!(app.state, AppState::SessionSelect));
    }

    #[test]
    fn claude_code_read_updates_read_block() {
        let mut app = App::default();
        app.state = AppState::ClaudeCodeDashboard;
        app.input = Input::from("examples/apps/new-input-form/Cargo.toml");

        app.run_workspace_read();

        assert_eq!(app.claude_code_state.read.kind, ToolKind::Read);
        assert!(matches!(
            app.claude_code_state.read.status,
            ToolStatus::Done | ToolStatus::Error
        ));
        assert_eq!(app.claude_code_state.activity_log.len(), 2);
    }

    #[test]
    fn claude_code_write_updates_write_block() {
        let mut app = App::default();
        app.state = AppState::ClaudeCodeDashboard;
        app.input = Input::from("target/new-input-form-claude-code.txt");

        app.run_workspace_write();

        assert_eq!(app.claude_code_state.write.kind, ToolKind::Write);
        assert!(matches!(
            app.claude_code_state.write.status,
            ToolStatus::Done | ToolStatus::Error
        ));
    }

    #[test]
    fn claude_code_execute_updates_execute_block() {
        let mut app = App::default();
        app.state = AppState::ClaudeCodeDashboard;
        app.input = Input::from("cargo test -p new-input-form --lib app::tests::session_select_lists_claude_code_last_and_enters_dashboard");

        app.run_workspace_execute();

        assert_eq!(app.claude_code_state.execute.kind, ToolKind::Execute);
        assert!(matches!(
            app.claude_code_state.execute.status,
            ToolStatus::Done | ToolStatus::Error
        ));
    }

    #[test]
    fn claude_code_activity_log_grows_for_multiple_events() {
        let mut app = App::default();
        app.state = AppState::ClaudeCodeDashboard;
        app.apply_tool_event(ToolEvent {
            kind: ToolKind::Read,
            status: ToolStatus::Done,
            target: "src/main.rs".to_string(),
            summary: "read".to_string(),
            error: None,
            elapsed_ms: None,
        });
        app.apply_tool_event(ToolEvent {
            kind: ToolKind::Write,
            status: ToolStatus::Done,
            target: "target/new-input-form-claude-code.txt".to_string(),
            summary: "saved".to_string(),
            error: None,
            elapsed_ms: None,
        });

        assert_eq!(app.claude_code_state.activity_log.len(), 2);
    }

    #[test]
    fn tab_cycles_claude_code_focus() {
        let mut app = App::default();
        app.state = AppState::ClaudeCodeDashboard;
        assert_eq!(app.claude_code_state.focused, ToolKind::Read);

        app.handle_claude_code(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        assert_eq!(app.claude_code_state.focused, ToolKind::Write);

        app.handle_claude_code(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        assert_eq!(app.claude_code_state.focused, ToolKind::Execute);

        app.handle_claude_code(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        assert_eq!(app.claude_code_state.focused, ToolKind::Read);
    }

    #[test]
    fn enter_runs_the_currently_focused_claude_code_action() {
        let mut app = App::default();
        app.state = AppState::ClaudeCodeDashboard;
        app.input = Input::from("examples/apps/new-input-form/Cargo.toml");

        let handled = app.handle_claude_code(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

        assert!(!handled);
        assert_eq!(app.claude_code_state.read.kind, ToolKind::Read);
        assert!(matches!(
            app.claude_code_state.read.status,
            ToolStatus::Done | ToolStatus::Error
        ));
    }

    #[test]
    fn choice_group_characters_do_not_enter_input() {
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

        let handled = app.handle_chat(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE));

        assert!(!handled);
        assert_eq!(app.input.value(), "");
        assert!(app.choice_group.is_some());
        assert!(matches!(
            app.choice_group.as_ref().unwrap().focus,
            FocusTarget::Question {
                question_index: 0,
                option_index: 0,
            }
        ));
    }

    #[test]
    fn tick_active_turn_auto_opens_choice_group_from_assistant_payload() {
        let mut app = App::default();
        app.state = AppState::Chat;
        app.current_session = Some(Session::new(SessionKind::Coder, ModelKind::HermesCode));
        app.follow_transcript = false;
        app.transcript_scroll = 7;
        app.active_turn = app
            .current_session
            .as_mut()
            .and_then(|session| session.begin_turn("/choice plan the next step"));

        while app.active_turn.is_some() {
            app.tick_active_turn();
        }

        assert!(app.choice_group.is_some());
        assert!(app.follow_transcript);
        assert_eq!(app.transcript_scroll, 0);
        assert!(
            app.current_session
                .as_mut()
                .and_then(|session| session.take_pending_choice_group())
                .is_none()
        );
    }

    #[test]
    fn tick_active_turn_keeps_transcript_following_while_streaming() {
        let mut app = App::default();
        app.state = AppState::Chat;
        app.current_session = Some(Session::new(SessionKind::Coder, ModelKind::HermesCode));
        app.follow_transcript = false;
        app.transcript_scroll = 12;
        app.active_turn = app
            .current_session
            .as_mut()
            .and_then(|session| session.begin_turn("/plan write tests"));

        app.tick_active_turn();

        assert!(app.follow_transcript);
        assert_eq!(app.transcript_scroll, 0);
        assert!(app.active_turn.is_some());
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

    #[test]
    fn tick_active_turn_does_not_duplicate_live_session_in_history() {
        let mut app = App::default();
        app.state = AppState::Chat;
        app.current_session = Some(Session::new(SessionKind::Coder, ModelKind::HermesCode));
        app.active_turn = app
            .current_session
            .as_mut()
            .and_then(|session| session.begin_turn("/plan write tests"));

        while app.active_turn.is_some() {
            app.tick_active_turn();
        }

        assert!(app.history.is_empty());
        assert!(app
            .current_session
            .as_ref()
            .unwrap()
            .messages
            .iter()
            .any(|message| message.role == crate::agent::Role::User
                && message.content == "/plan write tests"));
    }

    #[test]
    fn render_chat_shows_each_live_user_message_once() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("create terminal");
        let mut app = App::default();
        app.state = AppState::Chat;
        app.current_session = Some(Session::new(SessionKind::Coder, ModelKind::HermesCode));
        app.active_turn = app
            .current_session
            .as_mut()
            .and_then(|session| session.begin_turn("hello"));

        while app.active_turn.is_some() {
            app.tick_active_turn();
        }

        terminal
            .draw(|frame| {
                let _ = crate::ui::render_chat(
                    frame,
                    app.current_session.as_ref(),
                    &app.history,
                    &app.input,
                    app.command_mode,
                    app.command_picker.as_ref(),
                    app.choice_group.as_ref(),
                    app.spinner_frame,
                    app.follow_transcript,
                    app.transcript_scroll,
                );
            })
            .expect("draw chat");

        let buffer = terminal.backend().buffer();
        let rendered = buffer
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert_eq!(rendered.matches("hello").count(), 1);
    }
}
