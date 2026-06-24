use crate::agent::{Message, ModelKind, Session, SessionKind};
use crate::input_commands::{CommandKind, CommandMode, CommandPicker, PickerAction};
use ratatui::layout::{Constraint, Layout, Margin};
use ratatui::style::{Color, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, List, ListItem, Paragraph};
use ratatui::Frame;
use tui_input::Input;

pub fn render_session_select(frame: &mut Frame, selected: usize) {
    let layout = Layout::vertical([Constraint::Length(4), Constraint::Min(1)]);
    let [title_area, list_area] = frame.area().layout(&layout);

    let title = Line::from_iter([
        Span::from("Agent launcher").bold(),
        Span::from("  choose the operating mode"),
    ]);
    frame.render_widget(title.centered(), title_area);

    let items: Vec<ListItem> = SessionKind::ALL
        .iter()
        .enumerate()
        .map(|(index, kind)| {
            let header = if index == selected {
                Line::from(kind.label()).style(Style::new().fg(Color::Black).bg(Color::Cyan))
            } else {
                Line::from(kind.label()).style(Style::new().fg(Color::White))
            };
            let summary = Line::from(kind.summary()).style(Style::new().dark_gray());
            ListItem::new(vec![header, summary])
        })
        .collect();

    let list = List::new(items).block(
        Block::bordered()
            .title(Line::from("Session types").style(Style::new().bold())),
    );
    frame.render_widget(list, list_area);
}

pub fn render_model_select(frame: &mut Frame, session_index: usize, selected: usize) {
    let layout = Layout::vertical([Constraint::Length(4), Constraint::Min(1)]);
    let [title_area, list_area] = frame.area().layout(&layout);

    let session = SessionKind::ALL[session_index];
    let title = Line::from_iter([
        Span::from("Model selection").bold(),
        Span::from("  pick the runtime model"),
    ]);
    frame.render_widget(title.centered(), title_area);

    let items: Vec<ListItem> = ModelKind::ALL
        .iter()
        .enumerate()
        .map(|(index, kind)| {
            let header = if index == selected {
                Line::from(kind.label()).style(Style::new().fg(Color::Black).bg(Color::Yellow))
            } else {
                Line::from(kind.label()).style(Style::new().fg(Color::White))
            };
            let summary = Line::from(kind.summary()).style(Style::new().dark_gray());
            ListItem::new(vec![header, summary])
        })
        .collect();

    let list = List::new(items).block(
        Block::bordered().title(Line::from(format!("{} mode", session.label())).bold()),
    );
    frame.render_widget(list, list_area);
}

pub fn render_chat(
    frame: &mut Frame,
    session: Option<&Session>,
    history: &[Session],
    input: &Input,
    command_mode: CommandMode,
    command_picker: Option<&CommandPicker>,
    spinner_frame: usize,
    follow_transcript: bool,
) {
    let picker_height = command_picker
        .map(|picker| picker_popup_height(picker) + 2)
        .unwrap_or(0);
    let bottom_height = if picker_height > 0 { 3 + picker_height } else { 3 };
    let layout = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(1),
        Constraint::Length(bottom_height),
    ]);
    let [header_area, transcript_area, bottom_area] = frame.area().layout(&layout);
    let (input_area, picker_area) = if picker_height > 0 {
        let bottom_layout = Layout::vertical([
            Constraint::Length(3),
            Constraint::Length(picker_height),
        ]);
        let [input_area, picker_area] = bottom_area.layout(&bottom_layout);
        (input_area, Some(picker_area))
    } else {
        (bottom_area, None)
    };
    let body = Layout::horizontal([Constraint::Length(28), Constraint::Min(1)]);
    let [sidebar_area, transcript_body] = transcript_area.layout(&body);
    let transcript_lines = transcript_lines(history, session);
    let transcript_height = transcript_body.height as usize;
    let transcript_start = transcript_lines.len().saturating_sub(transcript_height.max(1));

    let header = match session {
        Some(session) => Line::from_iter([
            Span::from(session.kind.label()).bold(),
            Span::from(" / "),
            Span::from(session.model.label()).bold(),
            Span::from("  q exits this session"),
        ]),
        None => Line::from("No active session"),
    };
    frame.render_widget(header.centered(), header_area);

    let sidebar = render_sidebar(history, session);
    frame.render_widget(sidebar, sidebar_area);

    let transcript = List::new(transcript_lines.into_iter().skip(transcript_start).collect::<Vec<_>>()).block(
        Block::bordered()
            .title(Line::from(vec![
                Span::from("Transcript").style(Style::new().bold()),
                Span::from(" "),
                Span::from(if follow_transcript { "auto-follow" } else { "paused" })
                    .style(if follow_transcript {
                        Style::new().dark_gray()
                    } else {
                        Style::new().fg(Color::Yellow)
                    }),
            ])),
    );
    frame.render_widget(transcript, transcript_body);

    let input_widget = Paragraph::new(input.value())
        .style(Style::new().fg(Color::Yellow))
        .block(Block::bordered().title("Prompt"));
    frame.render_widget(input_widget, input_area);

    let status = if let Some(picker) = command_picker {
        match picker.kind {
            CommandKind::Files => Some("picker open: files".to_string()),
            CommandKind::Actions => Some("picker open: actions".to_string()),
        }
    } else if matches!(command_mode, CommandMode::None) {
        if let Some(session) = session {
            if session.messages.iter().any(|message| {
                message.role == crate::agent::Role::Tool
                    && message.content.contains("command interrupted by user")
            }) {
                Some("interrupt requested".to_string())
            } else if session.messages.iter().any(|message| {
                message.role == crate::agent::Role::Tool
                    && message.content.contains("running ->")
            }) {
                Some(format!("command running {}", spinner(spinner_frame)))
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    };

    if let (Some(picker), Some(picker_area)) = (command_picker, picker_area) {
        let picker_list = render_command_picker(picker);
        frame.render_widget(picker_list, picker_area);
    }

    let scroll = input.visual_scroll(input_area.width.saturating_sub(3) as usize);
    let cursor = input.visual_cursor().saturating_sub(scroll) as u16;
    frame.set_cursor_position((input_area.x + 1 + cursor, input_area.y + 1));

    let footer = Line::from_iter(
        status
            .into_iter()
            .flat_map(|status| {
                vec![
                    Span::from(status).style(Style::new().fg(Color::Magenta)),
                    Span::from("  "),
                ]
            })
            .chain([
                Span::from("Enter").bold(),
                Span::from(" send  "),
                Span::from("q").bold(),
                Span::from(" quit  "),
                Span::from("@").bold(),
                Span::from(" files/folders  "),
                Span::from("/").bold(),
                Span::from(" actions"),
            ])
            .collect::<Vec<_>>(),
    )
    .style(Style::new().dark_gray());
    frame.render_widget(footer, input_area.inner(Margin::new(1, 0)));
}

fn render_command_picker(picker: &CommandPicker) -> List<'static> {
    let title = match picker.kind {
        CommandKind::Files => format!("files (@{})", picker.prompt()),
        CommandKind::Actions => format!("actions (/{})", picker.prompt()),
    };
    let visible = picker.visible_items();
    let window_height = picker_popup_height(picker).max(1) as usize;
    let start = picker_window_start(picker.selected, visible.len(), window_height);
    let end = (start + window_height).min(visible.len());
    let mut rendered = Vec::new();
    for (visible_index, item) in visible[start..end].iter().enumerate() {
        let absolute_index = start + visible_index;
        match &item.action {
            PickerAction::EnterFiles | PickerAction::EnterFolders | PickerAction::EnterDirectory(_) => {
                let line = if absolute_index == picker.selected {
                    Line::from(item.label.clone()).style(Style::new().fg(Color::Black).bg(Color::Cyan))
                } else {
                    Line::from(item.label.clone())
                };
                rendered.push(ListItem::new(line));
            }
            PickerAction::Insert(_) => {
                let active_style = match picker.kind {
                    CommandKind::Files => Style::new().fg(Color::Black).bg(Color::Cyan),
                    CommandKind::Actions => Style::new().fg(Color::Black).bg(Color::Yellow),
                };
                let primary = if absolute_index == picker.selected {
                    Line::from(item.label.clone()).style(active_style)
                } else {
                    Line::from(item.label.clone()).style(Style::new().fg(Color::White))
                };
                let mut lines = vec![primary];
                if let Some(secondary) = &item.secondary {
                    let secondary_line = if absolute_index == picker.selected {
                        Line::from(format!("  {secondary}"))
                            .style(active_style.patch(Style::new().dark_gray()))
                    } else {
                        Line::from(format!("  {secondary}")).style(Style::new().dark_gray())
                    };
                    lines.push(secondary_line);
                }
                rendered.push(ListItem::new(lines));
            }
        }
    }
    if rendered.is_empty() {
        rendered.push(ListItem::new(
            Line::from("no matches").style(Style::new().dark_gray()),
        ));
    }
    List::new(rendered).block(Block::bordered().title(Line::from(title).bold()))
}

fn picker_popup_height(picker: &CommandPicker) -> u16 {
    let visible_count = picker.visible_items().len().max(1);
    visible_count.min(10) as u16
}

fn picker_window_start(selected: usize, visible_len: usize, window_height: usize) -> usize {
    if visible_len <= window_height {
        return 0;
    }

    let selected = selected.min(visible_len.saturating_sub(1));
    let mut start = if selected + 1 > window_height {
        selected + 1 - window_height
    } else {
        0
    };
    let max_start = visible_len - window_height;
    if start > max_start {
        start = max_start;
    }
    start
}

fn render_sidebar(history: &[Session], session: Option<&Session>) -> List<'static> {
    let current = session.map(|session| {
        format!("{} / {}", session.kind.label(), session.model.label())
    });
    let items = vec![
        ListItem::new(Line::from("sessions").bold()),
        ListItem::new(Line::from(match current {
            Some(ref label) => format!("active: {label}"),
            None => "active: none".to_string(),
        })),
        ListItem::new(Line::from(format!("history: {}", history.len()))),
        ListItem::new(Line::from("")),
        ListItem::new(Line::from("commands").bold()),
        ListItem::new(Line::from("/plan  create a work plan")),
        ListItem::new(Line::from("/run   execute a shell command")),
        ListItem::new(Line::from("/review inspect a diff")),
    ];
    List::new(items).block(Block::bordered().title("Control"))
}

fn transcript_lines(history: &[Session], session: Option<&Session>) -> Vec<ListItem<'static>> {
    let mut transcript_lines: Vec<ListItem> = Vec::new();
    for session in history.iter().rev().take(3) {
        transcript_lines.push(ListItem::new(Line::from(format!(
            "session: {} / {}",
            session.kind.label(),
            session.model.label()
        ))));
        for message in &session.messages {
            transcript_lines.push(render_message(message));
        }
    }
    if let Some(session) = session {
        transcript_lines.push(ListItem::new(Line::from(format!(
            "live: {} / {}",
            session.kind.label(),
            session.model.label()
        ))));
        for message in &session.messages {
            transcript_lines.push(render_message(message));
        }
    }
    transcript_lines
}

fn render_message(message: &Message) -> ListItem<'static> {
    if message.role == crate::agent::Role::Tool {
        return render_tool_message(message);
    }

    let role_style = match message.role {
        crate::agent::Role::System => Style::new().fg(Color::Blue),
        crate::agent::Role::User => Style::new().fg(Color::Green),
        crate::agent::Role::Assistant => {
            if message.content.starts_with("thinking...") {
                Style::new().fg(Color::Yellow)
            } else {
                Style::new().fg(Color::Cyan)
            }
        },
        crate::agent::Role::Tool => Style::new().fg(Color::Yellow),
    };
    let mut lines = vec![
        Line::from(vec![
            Span::from(format!("{}: ", message.role_label())).style(role_style),
            Span::from(message.content.lines().next().unwrap_or_default().to_string()),
        ]),
    ];
    for extra in message.content.lines().skip(1) {
        let extra_style = if message.content.starts_with("thinking...") {
            Style::new().fg(Color::Yellow)
        } else {
            Style::new().dark_gray()
        };
        lines.push(Line::from(format!("  {extra}")).style(extra_style));
    }
    ListItem::new(lines)
}

fn render_tool_message(message: &Message) -> ListItem<'static> {
    let mut lines = Vec::new();
    for (index, line) in message.content.lines().enumerate() {
        let rendered = match index {
            0 => Line::from(vec![
                Span::from("tool: ").style(Style::new().fg(Color::Magenta).bold()),
                Span::from(line.strip_prefix("tool: ").unwrap_or(line).to_string()),
            ]),
            1 if line.starts_with("status:") => Line::from(vec![
                Span::from("  ").style(Style::new().dark_gray()),
                Span::from(line.to_string()).style(Style::new().fg(Color::Yellow)),
            ]),
            1 if line.contains("running ->") => Line::from(vec![
                Span::from("  ").style(Style::new().dark_gray()),
                Span::from(line.to_string()).style(Style::new().fg(Color::Yellow).bold()),
            ]),
            1 if line.starts_with("launch error:") => Line::from(vec![
                Span::from("  ").style(Style::new().dark_gray()),
                Span::from(line.to_string()).style(Style::new().fg(Color::Red)),
            ]),
            _ if line == "stdout:" || line == "stderr:" => Line::from(vec![
                Span::from("  ").style(Style::new().dark_gray()),
                Span::from(line.to_string()).style(Style::new().fg(Color::Cyan).bold()),
            ]),
            _ if line.trim() == "<empty>" => Line::from(vec![
                Span::from("    ").style(Style::new().dark_gray()),
                Span::from(line.trim().to_string()).style(Style::new().dark_gray()),
            ]),
            _ if line.starts_with("  ") => Line::from(vec![
                Span::from("  ").style(Style::new().dark_gray()),
                Span::from(line.trim_start().to_string()),
            ]),
            _ => Line::from(line.to_string()),
        };
        lines.push(rendered);
    }
    ListItem::new(lines)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Message, ModelKind, Role, Session, SessionKind};
    use ratatui::{backend::TestBackend, Terminal};
    use tui_input::Input;

    #[test]
    fn render_chat_handles_interrupted_session_state() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("create terminal");
        let mut session = Session::new(SessionKind::Coder, ModelKind::HermesCode);
        session.messages.push(Message {
            role: Role::Tool,
            content: "tool: command interrupted by user".to_string(),
        });

        terminal
            .draw(|frame| {
                render_chat(
                    frame,
                    Some(&session),
                    &[],
                    &Input::default(),
                    CommandMode::None,
                    None,
                    1,
                    true,
                )
            })
            .expect("draw interrupted session");
    }

    #[test]
    fn render_chat_shows_spinner_for_running_commands() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("create terminal");
        let mut session = Session::new(SessionKind::Coder, ModelKind::HermesCode);
        session.messages.push(Message {
            role: Role::Tool,
            content: "tool: shell_command running -> sleep 2".to_string(),
        });

        terminal
            .draw(|frame| {
                render_chat(
                    frame,
                    Some(&session),
                    &[],
                    &Input::default(),
                    CommandMode::None,
                    None,
                    2,
                    true,
                )
            })
            .expect("draw running session");
    }

    #[test]
    fn render_chat_formats_tool_output_as_sections() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("create terminal");
        let mut session = Session::new(SessionKind::Coder, ModelKind::HermesCode);
        session.messages.push(Message {
            role: crate::agent::Role::Tool,
            content: "tool: shell_command -> /run echo hi\nstatus: exit status 0\nstdout:\n  hi\nstderr:\n  <empty>".to_string(),
        });

        terminal
            .draw(|frame| {
                render_chat(
                    frame,
                    Some(&session),
                    &[],
                    &Input::default(),
                    CommandMode::None,
                    None,
                    3,
                    true,
                )
            })
            .expect("draw tool transcript");
    }

    #[test]
    fn render_chat_highlights_thinking_and_running_states() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("create terminal");
        let mut session = Session::new(SessionKind::Coder, ModelKind::HermesCode);
        session.messages.push(Message::assistant("thinking...\n- analyzing request"));
        session.messages.push(Message::tool("tool: shell_command running -> sleep 2"));

        terminal
            .draw(|frame| {
                render_chat(
                    frame,
                    Some(&session),
                    &[],
                    &Input::default(),
                    CommandMode::None,
                    None,
                    3,
                    true,
                )
            })
            .expect("draw highlighted states");

        let transcript = transcript_lines(&[], Some(&session));
        let transcript_dump = transcript
            .iter()
            .map(|item| format!("{item:?}"))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(transcript_dump.contains("thinking..."));
        assert!(transcript_dump.contains("running ->"));
    }

    #[test]
    fn render_chat_labels_transcript_as_auto_following() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("create terminal");

        terminal
            .draw(|frame| {
                render_chat(
                    frame,
                    None,
                    &[],
                    &Input::default(),
                    CommandMode::None,
                    None,
                    0,
                    true,
                )
            })
            .expect("draw chat");

        let buffer = terminal.backend().buffer();
        let rendered = buffer
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(rendered.contains("Transcript"));
        assert!(rendered.contains("auto-follow"));
    }

    #[test]
    fn render_chat_labels_transcript_as_paused_when_follow_is_disabled() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("create terminal");

        terminal
            .draw(|frame| {
                render_chat(
                    frame,
                    None,
                    &[],
                    &Input::default(),
                    CommandMode::None,
                    None,
                    0,
                    false,
                )
            })
            .expect("draw chat");

        let buffer = terminal.backend().buffer();
        let rendered = buffer
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(rendered.contains("Transcript"));
        assert!(rendered.contains("paused"));
    }

    #[test]
    fn render_chat_shows_root_file_selector_choices() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("create terminal");
        let picker = CommandPicker::files(".");

        terminal
            .draw(|frame| {
                let _ = frame;
                let _ = &picker;
            })
            .expect("draw picker");
    }

    #[test]
    fn picker_popup_height_caps_at_ten_rows() {
        let mut picker = CommandPicker::files(".");
        picker.stage = crate::input_commands::PickerStage::Files;
        picker.items = (0..12)
            .map(|i| crate::input_commands::PickerItem {
                label: format!("@l{i}.rs"),
                secondary: None,
                action: crate::input_commands::PickerAction::Insert(format!("@l{i}.rs")),
            })
            .collect();

        assert_eq!(picker_popup_height(&picker), 10);
        picker.items.truncate(4);
        assert_eq!(picker_popup_height(&picker), 4);
    }

    #[test]
    fn picker_window_scrolls_when_selection_nears_bottom_edge() {
        assert_eq!(picker_window_start(0, 12, 10), 0);
        assert_eq!(picker_window_start(8, 12, 10), 0);
        assert_eq!(picker_window_start(9, 12, 10), 0);
        assert_eq!(picker_window_start(10, 12, 10), 1);
        assert_eq!(picker_window_start(11, 12, 10), 2);
    }

    #[test]
    fn picker_items_render_secondary_path_for_files() {
        let picker = CommandPicker {
            kind: CommandKind::Files,
            stage: crate::input_commands::PickerStage::Files,
            root: ".".into(),
            filter: String::new(),
            selected: 0,
            items: vec![crate::input_commands::PickerItem {
                label: "alpha.rs".to_string(),
                secondary: Some("src/alpha.rs".to_string()),
                action: crate::input_commands::PickerAction::Insert("@src/alpha.rs".to_string()),
            }],
        };

        let list = render_command_picker(&picker);
        let rendered = format!("{list:?}");
        assert!(rendered.contains("alpha.rs"));
        assert!(rendered.contains("src/alpha.rs"));
    }

    #[test]
    fn render_chat_footer_mentions_picker_shortcuts() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("create terminal");

        terminal
            .draw(|frame| {
                render_chat(
                    frame,
                    None,
                    &[],
                    &Input::default(),
                    CommandMode::None,
                    None,
                    0,
                    true,
                )
            })
            .expect("draw footer");

        let buffer = terminal.backend().buffer();
        let footer = buffer
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(footer.contains("@ files/folders"));
        assert!(footer.contains("/ actions"));
    }

    #[test]
    fn render_chat_keeps_latest_thinking_message_in_view() {
        let mut session = Session::new(SessionKind::Coder, ModelKind::HermesCode);
        session.messages.push(Message::assistant("thinking...\n- analyzing request"));
        for index in 0..20 {
            session.messages.push(Message::user(format!("message {index}")));
        }

        let transcript = transcript_lines(&[], Some(&session));
        assert!(transcript.iter().any(|item| {
            format!("{item:?}").contains("thinking...")
        }));
        assert!(transcript.iter().any(|item| {
            format!("{item:?}").contains("message 19")
        }));
    }
}

fn spinner(frame: usize) -> &'static str {
    match frame % 4 {
        0 => "⠋",
        1 => "⠙",
        2 => "⠹",
        _ => "⠸",
    }
}
