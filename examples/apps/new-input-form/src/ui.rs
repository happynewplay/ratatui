use crate::agent::{
    ChoiceGroupState, FocusTarget, Message, ModelKind, Session, SessionKind,
};
use crate::input_commands::{CommandKind, CommandMode, CommandPicker};
use ratatui::buffer::CellWidth;
use ratatui::layout::{Constraint, Layout, Margin};
use ratatui::style::{Color, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, List, ListItem, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState};
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
    choice_group: Option<&ChoiceGroupState>,
    spinner_frame: usize,
    follow_transcript: bool,
    transcript_scroll: usize,
) -> usize {
    let choice_height = choice_group
        .map(|group| choice_group_height(group, frame.area().width))
        .unwrap_or(0);
    let picker_height = command_picker
        .map(|picker| picker_popup_height(picker) + 2)
        .unwrap_or(0);
    let bottom_height = if choice_height > 0 {
        4 + choice_height
    } else if picker_height > 0 {
        4 + picker_height
    } else {
        4
    };
    let layout = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(1),
        Constraint::Length(bottom_height),
    ]);
    let [header_area, transcript_area, bottom_area] = frame.area().layout(&layout);
    let (input_area, footer_area, picker_area, choice_area) = if choice_height > 0 {
        let bottom_layout = Layout::vertical([
            Constraint::Length(3),
            Constraint::Length(1),
            Constraint::Length(choice_height),
        ]);
        let [input_area, footer_area, choice_area] = bottom_area.layout(&bottom_layout);
        (input_area, footer_area, None, Some(choice_area))
    } else if picker_height > 0 {
        let bottom_layout = Layout::vertical([
            Constraint::Length(3),
            Constraint::Length(1),
            Constraint::Length(picker_height),
        ]);
        let [input_area, footer_area, picker_area] = bottom_area.layout(&bottom_layout);
        (input_area, footer_area, Some(picker_area), None)
    } else {
        let bottom_layout = Layout::vertical([Constraint::Length(3), Constraint::Length(1)]);
        let [input_area, footer_area] = bottom_area.layout(&bottom_layout);
        (input_area, footer_area, None, None)
    };
    let body = Layout::horizontal([Constraint::Length(28), Constraint::Min(1)]);
    let [sidebar_area, transcript_body] = transcript_area.layout(&body);

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

    let transcript_inner = transcript_body.inner(Margin::new(1, 1));
    let transcript_content_width = transcript_inner.width.saturating_sub(1).max(1);
    let transcript_lines = transcript_lines(history, session, transcript_content_width);
    let transcript_total_len = transcript_lines.len();
    let transcript_height = transcript_inner.height as usize;
    let transcript_window = transcript_height.max(1);
    let transcript_start = if follow_transcript {
        transcript_total_len.saturating_sub(transcript_window)
    } else {
        transcript_scroll.min(transcript_total_len.saturating_sub(transcript_window))
    };
    let transcript_total = transcript_total_len.max(1);
    let transcript_position = transcript_start.saturating_add(1).min(transcript_total);
    let transcript_title = Line::from(vec![
        Span::from("Transcript").style(Style::new().bold()),
        Span::from(" "),
        Span::from(if follow_transcript {
            "auto-follow".to_string()
        } else {
            format!("paused {transcript_position}/{transcript_total}")
        })
        .style(if follow_transcript {
            Style::new().dark_gray()
        } else {
            Style::new().fg(Color::Yellow)
        }),
    ]);
    frame.render_widget(Block::bordered().title(transcript_title), transcript_body);
    let (transcript_content_area, transcript_scrollbar_area) =
        if transcript_lines.len() > transcript_window {
            let layout = Layout::horizontal([Constraint::Min(1), Constraint::Length(1)]);
            let [content_area, scrollbar_area] = transcript_inner.layout(&layout);
            (content_area, Some(scrollbar_area))
        } else {
            (transcript_inner, None)
        };

    let transcript = List::new(
        transcript_lines
            .into_iter()
            .skip(transcript_start)
            .collect::<Vec<_>>(),
    );
    frame.render_widget(transcript, transcript_content_area);

    if let Some(scrollbar_area) = transcript_scrollbar_area {
        let mut scrollbar_state = ScrollbarState::new(transcript_total_len)
            .position(transcript_start)
            .viewport_content_length(transcript_window);
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight);
        frame.render_stateful_widget(scrollbar, scrollbar_area, &mut scrollbar_state);
    }

    let scroll = input.visual_scroll(input_area.width.saturating_sub(3) as usize);
    let input_widget = Paragraph::new(input.value())
        .style(Style::new().fg(Color::Yellow))
        .scroll((0, scroll as u16))
        .block(Block::bordered().title("Prompt"));
    frame.render_widget(Clear, input_area);
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

    if let (Some(choice_group), Some(choice_area)) = (choice_group, choice_area) {
        let choice_widget = render_choice_group(choice_group, choice_area.width);
        frame.render_widget(choice_widget, choice_area);
    }

    if let (Some(picker), Some(picker_area)) = (command_picker, picker_area) {
        let picker_list = render_command_picker(picker);
        frame.render_widget(picker_list, picker_area);
    }

    let cursor = input.visual_cursor().saturating_sub(scroll) as u16;
    frame.set_cursor_position((input_area.x + 1 + cursor, input_area.y + 1));

    let footer = build_footer(command_mode, command_picker, choice_group, status);
    frame.render_widget(Clear, footer_area);
    frame.render_widget(footer, footer_area);

    transcript_total_len.saturating_sub(transcript_window)
}

fn build_footer(
    command_mode: CommandMode,
    command_picker: Option<&CommandPicker>,
    choice_group: Option<&ChoiceGroupState>,
    status: Option<String>,
) -> Line<'static> {
    let mut spans = Vec::new();
    if let Some(status) = status {
        spans.push(Span::from(status).style(Style::new().fg(Color::Magenta)));
        spans.push(Span::from("  "));
    }

    if choice_group.is_some() {
        spans.extend([
            Span::from("Enter").bold(),
            Span::from(" submit  "),
            Span::from("Esc").bold(),
            Span::from(" close  "),
            Span::from("q").bold(),
            Span::from(" quit"),
        ]);
    } else if command_picker.is_some() {
        spans.extend([
            Span::from("Enter").bold(),
            Span::from(" select  "),
            Span::from("Esc").bold(),
            Span::from(" close  "),
            Span::from("q").bold(),
            Span::from(" quit"),
        ]);
    } else {
        spans.extend([
            Span::from("Enter").bold(),
            Span::from(" send  "),
            Span::from("q").bold(),
            Span::from(" quit  "),
            Span::from("@").bold(),
            Span::from(" files/folders  "),
            Span::from("/").bold(),
            Span::from(" actions"),
        ]);
        if matches!(command_mode, CommandMode::None) {
            spans.push(Span::from(""));
        }
    }

    Line::from_iter(spans).style(Style::new().dark_gray())
}

fn choice_group_height(state: &ChoiceGroupState, area_width: u16) -> u16 {
    let inner_width = area_width.saturating_sub(2).max(1);
    let mut body_height = choice_group_lines(state, inner_width).len();
    body_height += 2;
    body_height.min(10).max(5) as u16
}

fn render_choice_group(state: &ChoiceGroupState, area_width: u16) -> Paragraph<'static> {
    let inner_width = area_width.saturating_sub(2).max(1);
    let lines = choice_group_lines(state, inner_width);
    Paragraph::new(lines)
        .wrap(ratatui::widgets::Wrap { trim: false })
        .block(Block::bordered().title("Choices"))
}

fn choice_group_lines(state: &ChoiceGroupState, width: u16) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    lines.extend(styled_wrapped_lines(
        &state.block.title,
        Style::new().bold(),
        width,
    ));
    for (question_index, question) in state.block.questions.iter().enumerate() {
        let question_focus = matches!(
            state.focus,
            FocusTarget::Question {
                question_index: index,
                ..
            } if index == question_index
        );
        let focus_style = if question_focus {
            Style::new().fg(Color::Black).bg(Color::Cyan)
        } else {
            Style::new()
        };
        lines.extend(styled_wrapped_lines(&question.prompt, focus_style, width));
        for (option_index, option) in question.options.iter().enumerate() {
            let checked = state.selected[question_index][option_index];
            let marker = if checked { "[x]" } else { "[ ]" };
            let option_style = if matches!(
                state.focus,
                FocusTarget::Question {
                    question_index: q_index,
                    option_index: o_index
                } if q_index == question_index && o_index == option_index
            ) {
                Style::new().fg(Color::Black).bg(Color::Yellow)
            } else if question_focus {
                Style::new().fg(Color::Yellow)
            } else {
                Style::new()
            };
            lines.extend(styled_wrapped_lines(
                &format!("{marker} {}", option.label),
                option_style,
                width,
            ));
        }
    }
    let submit_style = match state.focus {
        FocusTarget::Submit => Style::new().fg(Color::Black).bg(Color::Green),
        _ => Style::new().fg(Color::Green),
    };
    lines.extend(styled_wrapped_lines(
        &state.block.submit_label,
        submit_style,
        width,
    ));
    lines
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
        rendered.push(render_picker_item(
            item,
            absolute_index == picker.selected,
            picker.kind,
        ));
    }
    if rendered.is_empty() {
        rendered.push(ListItem::new(
            Line::from("no matches").style(Style::new().dark_gray()),
        ));
    }
    List::new(rendered).block(Block::bordered().title(Line::from(title).bold()))
}

fn render_picker_item(
    item: &crate::input_commands::PickerItem,
    selected: bool,
    kind: CommandKind,
) -> ListItem<'static> {
    let selected_style = match kind {
        CommandKind::Files => Style::new().fg(Color::Black).bg(Color::Cyan),
        CommandKind::Actions => Style::new().fg(Color::Black).bg(Color::Yellow),
    };
    let label_style = if selected {
        selected_style
    } else {
        Style::new().fg(Color::White)
    };
    let mut spans = vec![Span::from(item.label.clone()).style(label_style)];
    if let Some(secondary) = &item.secondary {
        spans.push(Span::from("  "));
        spans.push(Span::from(secondary.clone()).style(if selected {
            selected_style.patch(Style::new().dark_gray())
        } else {
            Style::new().dark_gray()
        }));
    }
    ListItem::new(Line::from(spans))
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

fn transcript_lines(
    history: &[Session],
    session: Option<&Session>,
    width: u16,
) -> Vec<ListItem<'static>> {
    let mut transcript_lines: Vec<ListItem> = Vec::new();
    for session in history.iter().rev().take(3) {
        transcript_lines.extend(
            styled_wrapped_lines(
                &format!("session: {} / {}", session.kind.label(), session.model.label()),
                Style::new(),
                width,
            )
            .into_iter()
            .map(ListItem::new),
        );
        for message in &session.messages {
            transcript_lines.extend(render_message_lines(message, width));
        }
    }
    if let Some(session) = session {
        transcript_lines.extend(
            styled_wrapped_lines(
                &format!("live: {} / {}", session.kind.label(), session.model.label()),
                Style::new(),
                width,
            )
            .into_iter()
            .map(ListItem::new),
        );
        for message in &session.messages {
            transcript_lines.extend(render_message_lines(message, width));
        }
    }
    transcript_lines
}

fn render_message_lines(message: &Message, width: u16) -> Vec<ListItem<'static>> {
    if message.role == crate::agent::Role::Tool {
        return render_tool_message_lines(message, width);
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
    let mut lines = styled_wrapped_lines(
        &format!(
            "{}: {}",
            message.role_label(),
            message.content.lines().next().unwrap_or_default()
        ),
        role_style,
        width,
    );
    for extra in message.content.lines().skip(1) {
        let extra_style = if message.content.starts_with("thinking...") {
            Style::new().fg(Color::Yellow)
        } else {
            Style::new().dark_gray()
        };
        lines.extend(styled_wrapped_lines(&format!("  {extra}"), extra_style, width));
    }
    lines.into_iter().map(ListItem::new).collect()
}

fn render_tool_message_lines(message: &Message, width: u16) -> Vec<ListItem<'static>> {
    let mut lines: Vec<Line<'static>> = Vec::new();
    for (index, line) in message.content.lines().enumerate() {
        let (text, style) = match index {
            0 => (
                format!("tool: {}", line.strip_prefix("tool: ").unwrap_or(line)),
                Style::new().fg(Color::Magenta).bold(),
            ),
            1 if line.starts_with("status:") => (format!("  {line}"), Style::new().fg(Color::Yellow)),
            1 if line.contains("running ->") => {
                (format!("  {line}"), Style::new().fg(Color::Yellow).bold())
            }
            1 if line.starts_with("launch error:") => {
                (format!("  {line}"), Style::new().fg(Color::Red))
            }
            _ if line == "stdout:" || line == "stderr:" => {
                (format!("  {line}"), Style::new().fg(Color::Cyan).bold())
            }
            _ if line.trim() == "<empty>" => {
                (format!("    {}", line.trim()), Style::new().dark_gray())
            }
            _ if line.starts_with("  ") => (line.to_string(), Style::new()),
            _ => (line.to_string(), Style::new()),
        };
        lines.extend(styled_wrapped_lines(&text, style, width));
    }
    lines.into_iter().map(ListItem::new).collect()
}

fn styled_wrapped_lines(text: &str, style: Style, width: u16) -> Vec<Line<'static>> {
    wrap_plain_lines(text, width)
        .into_iter()
        .map(|line| Line::from(line).style(style))
        .collect()
}

fn wrap_plain_lines(text: &str, width: u16) -> Vec<String> {
    let width = width.max(1);
    let mut wrapped = Vec::new();

    for raw_line in text.lines() {
        if raw_line.is_empty() {
            wrapped.push(String::new());
            continue;
        }

        let mut current = String::new();
        let mut current_width = 0u16;

        for ch in raw_line.chars() {
            let mut buf = [0; 4];
            let symbol = ch.encode_utf8(&mut buf);
            let symbol_width = symbol.cell_width();
            if symbol_width == 0 {
                current.push(ch);
                continue;
            }

            if current_width + symbol_width > width && !current.is_empty() {
                wrapped.push(std::mem::take(&mut current));
                current_width = 0;
            }

            current.push(ch);
            current_width = current_width.saturating_add(symbol_width);
        }

        if current.is_empty() {
            wrapped.push(String::new());
        } else {
            wrapped.push(current);
        }
    }

    if wrapped.is_empty() {
        wrapped.push(String::new());
    }

    wrapped
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{
        ChoiceGroupBlock, ChoiceGroupState, ChoiceMode, ChoiceOption, ChoiceQuestion, Message,
        ModelKind, Role, Session, SessionKind,
    };
    use ratatui::{backend::TestBackend, Terminal};
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};
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
                let _ = render_chat(
                    frame,
                    Some(&session),
                    &[],
                    &Input::default(),
                    CommandMode::None,
                    None,
                    None,
                    1,
                    true,
                    0,
                );
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
                let _ = render_chat(
                    frame,
                    Some(&session),
                    &[],
                    &Input::default(),
                    CommandMode::None,
                    None,
                    None,
                    2,
                    true,
                    0,
                );
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
                let _ = render_chat(
                    frame,
                    Some(&session),
                    &[],
                    &Input::default(),
                    CommandMode::None,
                    None,
                    None,
                    3,
                    true,
                    0,
                );
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
                let _ = render_chat(
                    frame,
                    Some(&session),
                    &[],
                    &Input::default(),
                    CommandMode::None,
                    None,
                    None,
                    3,
                    true,
                    0,
                );
            })
            .expect("draw highlighted states");

        let transcript = transcript_lines(&[], Some(&session), 40);
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
                let _ = render_chat(
                    frame,
                    None,
                    &[],
                    &Input::default(),
                    CommandMode::None,
                    None,
                    None,
                    0,
                    true,
                    0,
                );
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
                let _ = render_chat(
                    frame,
                    None,
                    &[],
                    &Input::default(),
                    CommandMode::None,
                    None,
                    None,
                    0,
                    false,
                    0,
                );
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
    fn render_chat_shows_transcript_position_when_paused() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("create terminal");
        let mut session = Session::new(SessionKind::Coder, ModelKind::HermesCode);
        session.messages.push(Message::assistant("thinking..."));
        session.messages.push(Message::user("one"));
        session.messages.push(Message::user("two"));

        terminal
            .draw(|frame| {
                let _ = render_chat(
                    frame,
                    Some(&session),
                    &[],
                    &Input::default(),
                    CommandMode::None,
                    None,
                    None,
                    0,
                    false,
                    1,
                );
            })
            .expect("draw paused transcript");

        let buffer = terminal.backend().buffer();
        let rendered = buffer
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(rendered.contains("paused"));
        assert!(rendered.contains("/"));
    }

    #[test]
    fn render_chat_clears_previous_chinese_input_from_prompt_area() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("create terminal");
        let input_with_chinese: Input = "中文".into();

        terminal
            .draw(|frame| {
                let _ = render_chat(
                    frame,
                    None,
                    &[],
                    &input_with_chinese,
                    CommandMode::None,
                    None,
                    None,
                    0,
                    true,
                    0,
                );
            })
            .expect("draw initial chinese input");

        terminal
            .draw(|frame| {
                let _ = render_chat(
                    frame,
                    None,
                    &[],
                    &Input::default(),
                    CommandMode::None,
                    None,
                    None,
                    0,
                    true,
                    0,
                );
            })
            .expect("draw cleared input");

        let buffer = terminal.backend().buffer();
        let rendered = buffer
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(!rendered.contains('中'));
        assert!(!rendered.contains('文'));
    }

    #[test]
    fn render_chat_scrolls_long_chinese_input_into_view() {
        let backend = TestBackend::new(20, 10);
        let mut terminal = Terminal::new(backend).expect("create terminal");
        let input_with_chinese: Input = "甲乙丙丁戊己庚辛壬癸".into();

        terminal
            .draw(|frame| {
                let _ = render_chat(
                    frame,
                    None,
                    &[],
                    &input_with_chinese,
                    CommandMode::None,
                    None,
                    None,
                    0,
                    true,
                    0,
                );
            })
            .expect("draw long chinese input");

        let buffer = terminal.backend().buffer();
        let rendered = buffer
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(rendered.contains('壬') || rendered.contains('癸'));
        assert!(!rendered.contains('甲'));
    }

    #[test]
    fn render_chat_places_cursor_with_long_chinese_input_in_visible_area() {
        let backend = TestBackend::new(20, 10);
        let mut terminal = Terminal::new(backend).expect("create terminal");
        let input_with_chinese: Input = "甲乙丙丁戊己庚辛壬癸".into();

        terminal
            .draw(|frame| {
                let _ = render_chat(
                    frame,
                    None,
                    &[],
                    &input_with_chinese,
                    CommandMode::None,
                    None,
                    None,
                    0,
                    true,
                    0,
                );
            })
            .expect("draw long chinese input");

        let cursor = terminal.backend().cursor_position();
        assert!(cursor.x > 0);
        assert!(cursor.y > 0);
        assert!(cursor.x < 20);
        assert!(cursor.y < 10);
    }

    #[test]
    fn render_chat_scrolls_transcript_when_paused() {
        let mut session = Session::new(SessionKind::Coder, ModelKind::HermesCode);
        session.messages.push(Message::assistant("thinking...\n- analyzing request"));
        for index in 0..10 {
            session.messages.push(Message::user(format!("message {index}")));
        }
        let transcript = transcript_lines(&[], Some(&session), 40);
        let transcript_dump = transcript
            .iter()
            .map(|item| format!("{item:?}"))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(transcript_dump.contains("message 9"));
        assert!(transcript_dump.contains("thinking..."));
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
    fn render_chat_shows_choice_group_block() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("create terminal");
        let choice_group = ChoiceGroupState::new(ChoiceGroupBlock {
            title: "Choose".to_string(),
            questions: vec![ChoiceQuestion {
                id: "mode".to_string(),
                mode: ChoiceMode::Single,
                prompt: "Mode?".to_string(),
                options: vec![
                    ChoiceOption {
                        id: "fast".to_string(),
                        label: "Fast".to_string(),
                    },
                    ChoiceOption {
                        id: "safe".to_string(),
                        label: "Safe".to_string(),
                    },
                ],
            }],
            submit_label: "Submit".to_string(),
        });

        terminal
            .draw(|frame| {
                let _ = render_chat(
                    frame,
                    None,
                    &[],
                    &Input::default(),
                    CommandMode::None,
                    None,
                    Some(&choice_group),
                    0,
                    true,
                    0,
                );
            })
            .expect("draw choice group");

        let buffer = terminal.backend().buffer();
        let rendered = buffer
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(rendered.contains("Choices"));
        assert!(rendered.contains("Submit"));
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
    fn command_picker_virtual_window_caps_at_ten_rows() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock before unix epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("new-input-form-picker-{unique}"));
        fs::create_dir_all(&root).expect("create picker root");
        for index in 0..12 {
            fs::write(root.join(format!("l{index}.rs")), "content").expect("write picker file");
        }

        let mut picker = CommandPicker::files(&root);
        picker.push_filter_char('l');
        picker.selected = 11;

        assert_eq!(picker.stage, crate::input_commands::PickerStage::Files);
        assert_eq!(picker.visible_items().len(), 12);

        let list = render_command_picker(&picker);
        let rendered = format!("{list:?}");

        assert!(rendered.contains("l2.rs"));
        assert!(rendered.contains("l11.rs"));
        assert!(!rendered.contains("l0.rs"));
        assert!(!rendered.contains("l1.rs"));
    }

    #[test]
    fn render_chat_shows_chinese_matches_in_picker_results() {
        let picker = CommandPicker {
            kind: CommandKind::Files,
            stage: crate::input_commands::PickerStage::Files,
            root: ".".into(),
            filter: "中文".to_string(),
            selected: 0,
            items: vec![
                crate::input_commands::PickerItem {
                    label: "中文说明.rs".to_string(),
                    secondary: Some("src/中文说明.rs".to_string()),
                    action: crate::input_commands::PickerAction::Insert("@中文说明.rs".to_string()),
                },
                crate::input_commands::PickerItem {
                    label: "latin.rs".to_string(),
                    secondary: Some("src/latin.rs".to_string()),
                    action: crate::input_commands::PickerAction::Insert("@latin.rs".to_string()),
                },
            ],
        };

        let list = render_command_picker(&picker);
        let rendered = format!("{list:?}");
        assert!(rendered.contains("中文说明.rs"));
        assert!(rendered.contains("src/中文说明.rs"));
        assert!(!rendered.contains("latin.rs"));
    }

    #[test]
    fn render_chat_shows_chinese_choice_labels() {
        let choice_group = ChoiceGroupState::new(ChoiceGroupBlock {
            title: "选择".to_string(),
            questions: vec![ChoiceQuestion {
                id: "mode".to_string(),
                mode: ChoiceMode::Single,
                prompt: "请选择模式".to_string(),
                options: vec![
                    ChoiceOption {
                        id: "快速".to_string(),
                        label: "快速".to_string(),
                    },
                    ChoiceOption {
                        id: "安全".to_string(),
                        label: "安全".to_string(),
                    },
                ],
            }],
            submit_label: "提交".to_string(),
        });

        let rendered = format!("{:?}", render_choice_group(&choice_group, 40));
        assert!(rendered.contains("选择"));
        assert!(rendered.contains("请选择模式"));
        assert!(rendered.contains("快速"));
        assert!(rendered.contains("安全"));
        assert!(rendered.contains("提交"));
    }

    #[test]
    fn render_chat_wraps_long_chinese_transcript_messages() {
        let backend = TestBackend::new(44, 16);
        let mut terminal = Terminal::new(backend).expect("create terminal");
        let mut session = Session::new(SessionKind::Coder, ModelKind::HermesCode);
        session.messages.push(Message::user(
            "这是一条很长的中文消息用于验证转录区会自动换行并保留末尾内容终".to_string(),
        ));

        terminal
            .draw(|frame| {
                let _ = render_chat(
                    frame,
                    Some(&session),
                    &[],
                    &Input::default(),
                    CommandMode::None,
                    None,
                    None,
                    0,
                    true,
                    0,
                );
            })
            .expect("draw wrapped chinese transcript");

        let buffer = terminal.backend().buffer();
        let rendered = buffer
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(rendered.contains('这'));
        assert!(rendered.contains('条'));
        assert!(rendered.contains('终'));
    }

    #[test]
    fn render_chat_wraps_long_chinese_choice_content() {
        let backend = TestBackend::new(44, 16);
        let mut terminal = Terminal::new(backend).expect("create terminal");
        let choice_group = ChoiceGroupState::new(ChoiceGroupBlock {
            title: "选择".to_string(),
            questions: vec![ChoiceQuestion {
                id: "mode".to_string(),
                mode: ChoiceMode::Single,
                prompt: "这是一个很长的中文问题用于验证选择区域会自动换行并展示结尾终".to_string(),
                options: vec![ChoiceOption {
                    id: "long".to_string(),
                    label: "这个选项说明也很长需要完整展示尾".to_string(),
                }],
            }],
            submit_label: "提交".to_string(),
        });

        terminal
            .draw(|frame| {
                let _ = render_chat(
                    frame,
                    None,
                    &[],
                    &Input::default(),
                    CommandMode::None,
                    None,
                    Some(&choice_group),
                    0,
                    true,
                    0,
                );
            })
            .expect("draw wrapped chinese choices");

        let buffer = terminal.backend().buffer();
        let rendered = buffer
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(rendered.contains('这'));
        assert!(rendered.contains('选'));
        assert!(rendered.contains('终'));
        assert!(rendered.contains('尾'));
    }

    #[test]
    fn render_chat_shows_transcript_scrollbar_for_overflowing_content() {
        let backend = TestBackend::new(80, 12);
        let mut terminal = Terminal::new(backend).expect("create terminal");
        let mut session = Session::new(SessionKind::Coder, ModelKind::HermesCode);
        for index in 0..20 {
            session.messages.push(Message::user(format!("message {index}")));
        }

        terminal
            .draw(|frame| {
                let _ = render_chat(
                    frame,
                    Some(&session),
                    &[],
                    &Input::default(),
                    CommandMode::None,
                    None,
                    None,
                    0,
                    true,
                    0,
                );
            })
            .expect("draw overflowing transcript");

        let buffer = terminal.backend().buffer();
        let rendered = buffer
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(rendered.contains("message 18") || rendered.contains("message 19"));
        assert!(rendered.contains("│"));
    }

    #[test]
    fn render_chat_footer_mentions_picker_shortcuts() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("create terminal");

        terminal
            .draw(|frame| {
                let _ = render_chat(
                    frame,
                    None,
                    &[],
                    &Input::default(),
                    CommandMode::None,
                    None,
                    None,
                    0,
                    true,
                    0,
                );
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
    fn render_chat_footer_mentions_choice_shortcuts() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("create terminal");
        let choice_group = ChoiceGroupState::new(ChoiceGroupBlock {
            title: "Choose".to_string(),
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
        });

        terminal
            .draw(|frame| {
                let _ = render_chat(
                    frame,
                    None,
                    &[],
                    &Input::default(),
                    CommandMode::None,
                    None,
                    Some(&choice_group),
                    0,
                    true,
                    0,
                );
            })
            .expect("draw choice footer");

        let buffer = terminal.backend().buffer();
        let footer = buffer
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(footer.contains("Esc"));
        assert!(footer.contains("submit"));
        assert!(footer.contains("close"));
    }

    #[test]
    fn render_chat_keeps_latest_thinking_message_in_view() {
        let mut session = Session::new(SessionKind::Coder, ModelKind::HermesCode);
        session.messages.push(Message::assistant("thinking...\n- analyzing request"));
        for index in 0..20 {
            session.messages.push(Message::user(format!("message {index}")));
        }

        let transcript = transcript_lines(&[], Some(&session), 40);
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




