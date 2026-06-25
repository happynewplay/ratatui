use std::process::{Command as ProcessCommand, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionKind {
    Planner,
    Coder,
    Reviewer,
    ClaudeCode,
}

impl SessionKind {
    pub const ALL: [Self; 4] = [Self::Planner, Self::Coder, Self::Reviewer, Self::ClaudeCode];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Planner => "Planner",
            Self::Coder => "Coder",
            Self::Reviewer => "Reviewer",
            Self::ClaudeCode => "Claude Code",
        }
    }

    pub const fn summary(self) -> &'static str {
        match self {
            Self::Planner => "Break work into steps and scope.",
            Self::Coder => "Implement code and validate changes.",
            Self::Reviewer => "Inspect diffs and note risks.",
            Self::ClaudeCode => "Explore workspace tools in a segmented dashboard.",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolKind {
    Read,
    Write,
    Execute,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolStatus {
    Idle,
    Queued,
    Running,
    Done,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolEvent {
    pub kind: ToolKind,
    pub status: ToolStatus,
    pub target: String,
    pub summary: String,
    pub error: Option<String>,
    pub elapsed_ms: Option<u64>,
}

impl ToolEvent {
    pub fn new(kind: ToolKind, target: impl Into<String>) -> Self {
        Self {
            kind,
            status: ToolStatus::Idle,
            target: target.into(),
            summary: String::new(),
            error: None,
            elapsed_ms: None,
        }
    }

    pub fn transcript(&self) -> String {
        let kind = match self.kind {
            ToolKind::Read => "read",
            ToolKind::Write => "write",
            ToolKind::Execute => "execute",
        };
        let status = match self.status {
            ToolStatus::Idle => "idle",
            ToolStatus::Queued => "queued",
            ToolStatus::Running => "running",
            ToolStatus::Done => "done",
            ToolStatus::Error => "error",
        };
        let mut lines = vec![format!("tool: {kind} -> {}", self.target)];
        lines.push(format!("status: {status}"));
        if !self.summary.is_empty() {
            lines.push(format!("summary: {}", self.summary));
        }
        if let Some(error) = &self.error {
            lines.push(format!("error: {error}"));
        }
        if let Some(elapsed) = self.elapsed_ms {
            lines.push(format!("elapsed: {elapsed}ms"));
        }
        lines.join("\n")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaudeCodeDashboardState {
    pub read: ToolEvent,
    pub write: ToolEvent,
    pub execute: ToolEvent,
    pub activity_log: Vec<ToolEvent>,
    pub messages: Vec<Message>,
    pub focused: ToolKind,
}

impl ClaudeCodeDashboardState {
    pub fn new() -> Self {
        Self {
            read: ToolEvent::new(ToolKind::Read, ""),
            write: ToolEvent::new(ToolKind::Write, ""),
            execute: ToolEvent::new(ToolKind::Execute, ""),
            activity_log: Vec::new(),
            messages: Vec::new(),
            focused: ToolKind::Read,
        }
    }

    pub fn focus_next(&mut self) {
        self.focused = match self.focused {
            ToolKind::Read => ToolKind::Write,
            ToolKind::Write => ToolKind::Execute,
            ToolKind::Execute => ToolKind::Read,
        };
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelKind {
    HermesSmall,
    HermesPro,
    HermesCode,
}

impl ModelKind {
    pub const ALL: [Self; 3] = [Self::HermesSmall, Self::HermesPro, Self::HermesCode];

    pub const fn label(self) -> &'static str {
        match self {
            Self::HermesSmall => "hermes-small",
            Self::HermesPro => "hermes-pro",
            Self::HermesCode => "hermes-code",
        }
    }

    pub const fn summary(self) -> &'static str {
        match self {
            Self::HermesSmall => "Low latency general assistant.",
            Self::HermesPro => "Balanced reasoning and throughput.",
            Self::HermesCode => "Best fit for code-heavy sessions.",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone)]
pub struct Message {
    pub role: Role,
    pub content: String,
}

impl Message {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: Role::System,
            content: content.into(),
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            content: content.into(),
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: Role::Assistant,
            content: content.into(),
        }
    }

    pub fn tool(content: impl Into<String>) -> Self {
        Self {
            role: Role::Tool,
            content: content.into(),
        }
    }

    pub fn role_label(&self) -> &'static str {
        match self.role {
            Role::System => "system",
            Role::User => "user",
            Role::Assistant => "assistant",
            Role::Tool => "tool",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChoiceMode {
    Single,
    Multi,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChoiceOption {
    pub id: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChoiceQuestion {
    pub id: String,
    pub mode: ChoiceMode,
    pub prompt: String,
    pub options: Vec<ChoiceOption>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChoiceGroupBlock {
    pub title: String,
    pub questions: Vec<ChoiceQuestion>,
    pub submit_label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AssistantPayload {
    ChoiceGroup(ChoiceGroupBlock),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusTarget {
    Question { question_index: usize, option_index: usize },
    Submit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChoiceGroupState {
    pub block: ChoiceGroupBlock,
    pub focus: FocusTarget,
    pub selected: Vec<Vec<bool>>,
}

pub fn parse_choice_group_payload(content: &str) -> Option<ChoiceGroupBlock> {
    serde_json::from_str::<AssistantPayload>(content)
        .ok()
        .and_then(|payload| match payload {
            AssistantPayload::ChoiceGroup(block) => Some(block),
        })
}

pub fn serialize_choice_answers(answers: &[(String, Vec<String>)]) -> String {
    #[derive(Serialize)]
    struct Answer<'a> {
        question_id: &'a str,
        selected_ids: &'a [String],
    }

    #[derive(Serialize)]
    struct Submission<'a> {
        answers: Vec<Answer<'a>>,
    }

    let answers = answers
        .iter()
        .map(|(question_id, selected_ids)| Answer {
            question_id: question_id.as_str(),
            selected_ids,
        })
        .collect();
    serde_json::to_string(&Submission { answers }).unwrap_or_else(|_| "{}".to_string())
}

impl ChoiceGroupState {
    pub fn new(block: ChoiceGroupBlock) -> Self {
        let selected: Vec<Vec<bool>> = block
            .questions
            .iter()
            .map(|question| vec![false; question.options.len()])
            .collect();
        Self {
            block,
            focus: if selected.is_empty() {
                FocusTarget::Submit
            } else {
                FocusTarget::Question {
                    question_index: 0,
                    option_index: 0,
                }
            },
            selected,
        }
    }

    pub fn selected_answers(&self) -> Vec<(String, Vec<String>)> {
        self.block
            .questions
            .iter()
            .enumerate()
            .map(|(question_index, question)| {
                let selected = self.selected[question_index]
                    .iter()
                    .enumerate()
                    .filter_map(|(option_index, checked)| {
                        checked.then(|| question.options[option_index].id.clone())
                    })
                    .collect();
                (question.id.clone(), selected)
            })
            .collect()
    }

    pub fn question_count(&self) -> usize {
        self.block.questions.len()
    }

    pub fn option_count(&self, question_index: usize) -> usize {
        self.block.questions[question_index].options.len()
    }

    pub fn move_focus_down(&mut self) {
        self.focus = match self.focus {
            FocusTarget::Question { question_index, option_index } => {
                let option_count = self.option_count(question_index);
                if option_index + 1 < option_count {
                    FocusTarget::Question {
                        question_index,
                        option_index: option_index + 1,
                    }
                } else if question_index + 1 < self.question_count() {
                    FocusTarget::Question {
                        question_index: question_index + 1,
                        option_index: 0,
                    }
                } else {
                    FocusTarget::Submit
                }
            }
            FocusTarget::Submit => {
                if self.question_count() == 0 {
                    FocusTarget::Submit
                } else {
                    let last_question = self.question_count() - 1;
                    FocusTarget::Question {
                        question_index: last_question,
                        option_index: self.option_count(last_question).saturating_sub(1),
                    }
                }
            }
        };
    }

    pub fn move_focus_up(&mut self) {
        self.focus = match self.focus {
            FocusTarget::Question { question_index, option_index } => {
                if option_index > 0 {
                    FocusTarget::Question {
                        question_index,
                        option_index: option_index - 1,
                    }
                } else if question_index == 0 {
                    FocusTarget::Submit
                } else {
                    let prev_question = question_index - 1;
                    let last_option = self.option_count(prev_question).saturating_sub(1);
                    FocusTarget::Question {
                        question_index: prev_question,
                        option_index: last_option,
                    }
                }
            }
            FocusTarget::Submit => {
                let last_question = self.question_count().saturating_sub(1);
                let last_option = self.option_count(last_question).saturating_sub(1);
                if self.question_count() == 0 {
                    FocusTarget::Submit
                } else {
                    FocusTarget::Question {
                        question_index: last_question,
                        option_index: last_option,
                    }
                }
            }
        };
    }

    pub fn toggle_selected(&mut self, question_index: usize, option_index: usize) {
        let Some(question) = self.block.questions.get(question_index) else {
            return;
        };
        let Some(row) = self.selected.get_mut(question_index) else {
            return;
        };
        if option_index >= row.len() {
            return;
        }
        match question.mode {
            ChoiceMode::Single => {
                for value in row.iter_mut() {
                    *value = false;
                }
                row[option_index] = true;
            }
            ChoiceMode::Multi => {
                row[option_index] = !row[option_index];
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct Session {
    pub kind: SessionKind,
    pub model: ModelKind,
    pub messages: Vec<Message>,
    pending_choice_group: Option<ChoiceGroupBlock>,
}

impl Session {
    pub fn new(kind: SessionKind, model: ModelKind) -> Self {
        let mut session = Self {
            kind,
            model,
            messages: vec![],
            pending_choice_group: None,
        };
        session.messages.push(Message::system(format!(
            "Session started in {} mode using {}.",
            kind.label(),
            model.label()
        )));
        session
    }

    pub fn begin_turn(&mut self, input: &str) -> Option<PendingTurn> {
        let input = input.trim();
        if input.is_empty() {
            return None;
        }

        let action = turn_action_for(input, self.kind, self.model);
        if let TurnAction::ChoicePrompt { subject } = &action {
            self.pending_choice_group = Some(choice_group_block(subject, self.kind, self.model));
        }
        let assistant_text = action.assistant_text(self.kind, self.model);
        let chunks = chunk_text(&assistant_text);

        self.messages.push(Message::user(input));
        self.messages
            .push(Message::assistant(PendingTurn::thinking_message(
                &action,
                0,
                false,
                false,
                false,
            )));
        let assistant_index = self.messages.len();
        self.messages.push(Message::assistant(String::new()));

        Some(PendingTurn {
            assistant_index,
            chunks,
            action,
            stage: 0,
            shell_job: None,
            shell_result: None,
            shell_running_emitted: false,
            interrupt_requested: false,
        })
    }

    pub fn queue_choice_group(&mut self, block: ChoiceGroupBlock) {
        self.pending_choice_group = Some(block);
    }

    pub fn take_pending_choice_group(&mut self) -> Option<ChoiceGroupBlock> {
        self.pending_choice_group.take()
    }
}

#[derive(Debug)]
pub struct PendingTurn {
    assistant_index: usize,
    chunks: Vec<String>,
    action: TurnAction,
    stage: usize,
    shell_job: Option<CommandExecution>,
    shell_result: Option<CommandResult>,
    shell_running_emitted: bool,
    interrupt_requested: bool,
}

impl PendingTurn {
    pub fn request_interrupt(&mut self) {
        self.interrupt_requested = true;
    }

    pub fn is_interrupted(&self) -> bool {
        self.interrupt_requested
    }

    fn thinking_message(
        action: &TurnAction,
        stage: usize,
        shell_job_running: bool,
        shell_result_ready: bool,
        shell_running_emitted: bool,
    ) -> String {
        let mut lines = vec!["thinking...".to_string()];
        match action {
            TurnAction::ShellCommand { .. } => {
                lines.push("- analyzing request".to_string());
                if stage > 0 {
                    lines.push("- drafting response".to_string());
                }
                if stage >= 1 {
                    lines.push("- preparing execution".to_string());
                }
                if shell_job_running && !shell_result_ready {
                    lines.push("- running tools".to_string());
                    lines.push("- waiting for output".to_string());
                }
                if shell_running_emitted {
                    lines.push("- streaming command output".to_string());
                }
                if shell_result_ready {
                    lines.push("- combining results".to_string());
                }
            }
            TurnAction::ReviewDiff { target } => {
                lines.push(format!("- reviewing {target}"));
                if stage > 0 {
                    lines.push("- checking risks".to_string());
                }
                if shell_result_ready {
                    lines.push("- finalizing review".to_string());
                }
            }
            TurnAction::BuildPlan { subject } => {
                lines.push(format!("- outlining {subject}"));
                if stage > 0 {
                    lines.push("- ordering steps".to_string());
                }
                if shell_result_ready {
                    lines.push("- finalizing plan".to_string());
                }
            }
            TurnAction::ChoicePrompt { subject } => {
                lines.push(format!("- preparing choices for {subject}"));
                if stage > 0 {
                    lines.push("- drafting available options".to_string());
                }
                if shell_result_ready {
                    lines.push("- finalizing choice prompt".to_string());
                }
            }
        }
        lines.join("\n")
    }

    fn update_thinking_message(&self, session: &mut Session) {
        let thinking_index = self.assistant_index.saturating_sub(1);
        if let Some(message) = session.messages.get_mut(thinking_index) {
            message.content = Self::thinking_message(
                &self.action,
                self.stage,
                self.shell_job.is_some(),
                self.shell_result.is_some(),
                self.shell_running_emitted,
            );
        }
    }

    pub fn tick(&mut self, session: &mut Session) -> bool {
        self.update_thinking_message(session);

        if self.stage < self.chunks.len() {
            session.messages[self.assistant_index]
                .content
                .push_str(&self.chunks[self.stage]);
            self.stage += 1;
            self.update_thinking_message(session);
            return false;
        }

        if self.stage == self.chunks.len() {
            session.messages.push(Message::tool(self.action.start_message()));
            match &self.action {
                TurnAction::ShellCommand { command } => {
                    if command.trim().is_empty() {
                        self.shell_result = Some(CommandResult::empty_command(command.clone()));
                    } else {
                        self.shell_job = Some(CommandExecution::spawn(command.clone()));
                    }
                }
                TurnAction::ReviewDiff { .. }
                | TurnAction::BuildPlan { .. }
                | TurnAction::ChoicePrompt { .. } => {}
            }
            self.stage += 1;
            return false;
        }

        if self.stage == self.chunks.len() + 1 {
            match &self.action {
                TurnAction::ShellCommand { .. } => {
                    if self.shell_result.is_none() {
                        if self.interrupt_requested {
                            if let Some(job) = self.shell_job.as_mut() {
                                job.cancel();
                            }
                            let command = match &self.action {
                                TurnAction::ShellCommand { command } => command.clone(),
                                _ => String::new(),
                            };
                            self.shell_result = Some(CommandResult::interrupted(command));
                            self.finish_command(session);
                            self.update_thinking_message(session);
                            self.stage += 1;
                            return true;
                        }
                        let Some(job) = self.shell_job.as_ref() else {
                            self.shell_result = Some(CommandResult::worker_disconnected());
                            self.finish_command(session);
                            self.update_thinking_message(session);
                            self.stage += 1;
                            return true;
                        };
                        match job.try_finish() {
                            Some(result) => self.shell_result = Some(result),
                            None => {
                                if !self.shell_running_emitted {
                                    session
                                        .messages
                                        .push(Message::tool(self.action.running_message()));
                                    self.shell_running_emitted = true;
                                    self.update_thinking_message(session);
                                }
                                return false;
                            }
                        }
                    }
                    self.finish_command(session);
                    self.update_thinking_message(session);
                    self.stage += 1;
                    return true;
                }
                TurnAction::ReviewDiff { .. }
                | TurnAction::BuildPlan { .. }
                | TurnAction::ChoicePrompt { .. } => {
                    session.messages
                        .push(Message::assistant(self.action.final_message(None)));
                    self.update_thinking_message(session);
                    self.stage += 1;
                    return true;
                }
            }
        }

        true
    }

    fn finish_command(&mut self, session: &mut Session) {
        if let Some(result) = self.shell_result.take() {
            session.messages.push(Message::tool(result.transcript()));
            session
                .messages
                .push(Message::assistant(self.action.final_message(Some(&result))));
        }
    }
}

#[derive(Debug, Clone)]
enum TurnAction {
    ShellCommand { command: String },
    ReviewDiff { target: String },
    BuildPlan { subject: String },
    ChoicePrompt { subject: String },
}

impl TurnAction {
    fn assistant_text(&self, kind: SessionKind, model: ModelKind) -> String {
        match self {
            Self::ShellCommand { command } => format!(
                "{} mode on {} is preparing to run `{}` and capture the tool output.",
                kind.label(),
                model.label(),
                command
            ),
            Self::ReviewDiff { target } => format!(
                "{} mode on {} is reviewing {} and preparing feedback.",
                kind.label(),
                model.label(),
                target
            ),
            Self::BuildPlan { subject } => format!(
                "{} mode on {} is outlining the work for {}.",
                kind.label(),
                model.label(),
                subject
            ),
            Self::ChoicePrompt { subject } => format!(
                "{} mode on {} is preparing a choice prompt for {}.",
                kind.label(),
                model.label(),
                subject
            ),
        }
    }

    fn start_message(&self) -> String {
        match self {
            Self::ShellCommand { command } => format!("tool: shell_command -> {command}"),
            Self::ReviewDiff { target } => format!("tool: code_review -> {target}"),
            Self::BuildPlan { subject } => format!("tool: planner -> {subject}"),
            Self::ChoicePrompt { subject } => format!("tool: choice_prompt -> {subject}"),
        }
    }

    fn running_message(&self) -> String {
        match self {
            Self::ShellCommand { command } => format!("tool: shell_command running -> {command}"),
            Self::ReviewDiff { target } => format!("tool: code_review running -> {target}"),
            Self::BuildPlan { subject } => format!("tool: planner running -> {subject}"),
            Self::ChoicePrompt { subject } => format!("tool: choice_prompt running -> {subject}"),
        }
    }

    fn final_message(&self, result: Option<&CommandResult>) -> String {
        match self {
            Self::ShellCommand { command } => {
                if let Some(result) = result {
                    if result.execution_error.is_some() {
                        format!("assistant: command `{command}` could not be launched.")
                    } else if result.is_success() {
                        format!("assistant: command `{command}` completed successfully.")
                    } else {
                        format!(
                            "assistant: command `{command}` finished with {}.",
                            result.status_label()
                        )
                    }
                } else {
                    format!("assistant: command `{command}` is running.")
                }
            }
            Self::ReviewDiff { target } => format!("assistant: review notes recorded for {target}."),
            Self::BuildPlan { subject } => format!("assistant: plan drafted for {subject}."),
            Self::ChoicePrompt { subject } => format!("assistant: choice prompt ready for {subject}."),
        }
    }
}

#[derive(Debug)]
struct CommandExecution {
    cancel_requested: Arc<AtomicBool>,
    receiver: Receiver<CommandResult>,
}

impl CommandExecution {
    fn spawn(command: String) -> Self {
        let (tx, rx) = mpsc::channel();
        let cancel_requested = Arc::new(AtomicBool::new(false));
        let cancel_for_thread = Arc::clone(&cancel_requested);
        thread::spawn(move || {
            let result = run_shell_command(&command, cancel_for_thread);
            let _ = tx.send(result);
        });
        Self { cancel_requested, receiver: rx }
    }

    fn cancel(&mut self) {
        self.cancel_requested.store(true, Ordering::SeqCst);
    }

    fn try_finish(&self) -> Option<CommandResult> {
        match self.receiver.try_recv() {
            Ok(result) => Some(result),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => Some(CommandResult::worker_disconnected()),
        }
    }
}

#[derive(Debug, Clone)]
struct CommandResult {
    command: String,
    status_code: Option<i32>,
    stdout: String,
    stderr: String,
    execution_error: Option<String>,
}

impl CommandResult {
    fn interrupted(command: String) -> Self {
        Self {
            command,
            status_code: None,
            stdout: String::new(),
            stderr: String::new(),
            execution_error: Some("command interrupted by user".to_string()),
        }
    }

    fn empty_command(command: String) -> Self {
        Self {
            command,
            status_code: None,
            stdout: String::new(),
            stderr: String::new(),
            execution_error: Some("missing shell command after /run".to_string()),
        }
    }

    fn worker_disconnected() -> Self {
        Self {
            command: String::new(),
            status_code: None,
            stdout: String::new(),
            stderr: String::new(),
            execution_error: Some("shell command worker disconnected".to_string()),
        }
    }

    fn from_output(command: String, output: std::process::Output) -> Self {
        Self {
            command,
            status_code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            execution_error: None,
        }
    }

    fn from_launch_error(command: String, error: impl Into<String>) -> Self {
        Self {
            command,
            status_code: None,
            stdout: String::new(),
            stderr: String::new(),
            execution_error: Some(error.into()),
        }
    }

    fn is_success(&self) -> bool {
        self.execution_error.is_none() && self.status_code == Some(0)
    }

    fn status_label(&self) -> String {
        match self.status_code {
            Some(code) => format!("exit status {code}"),
            None => "an unknown exit status".to_string(),
        }
    }

    fn transcript(&self) -> String {
        let mut lines = vec![format!("tool: shell_command -> {}", self.command)];
        if let Some(error) = &self.execution_error {
            lines.push(format!("launch error: {error}"));
            return lines.join("\n");
        }

        lines.push(format!("status: {}", self.status_label()));
        lines.push("stdout:".to_string());
        if self.stdout.trim().is_empty() {
            lines.push("  <empty>".to_string());
        } else {
            lines.extend(self.stdout.trim_end().lines().map(|line| format!("  {line}")));
        }
        lines.push("stderr:".to_string());
        if self.stderr.trim().is_empty() {
            lines.push("  <empty>".to_string());
        } else {
            lines.extend(self.stderr.trim_end().lines().map(|line| format!("  {line}")));
        }
        lines.join("\n")
    }
}

fn turn_action_for(input: &str, kind: SessionKind, model: ModelKind) -> TurnAction {
    if let Some(command) = shell_command_from_input(input) {
        return TurnAction::ShellCommand { command };
    }
    if let Some(subject) = choice_command_from_input(input) {
        return TurnAction::ChoicePrompt { subject };
    }

    let lowered = input.to_ascii_lowercase();
    if lowered.starts_with("/review") || lowered.contains("review") || lowered.contains("diff") {
        return TurnAction::ReviewDiff {
            target: "changed files".to_string(),
        };
    }
    if lowered.starts_with("/plan") || lowered.contains("build") {
        return TurnAction::BuildPlan {
            subject: format!("{} session on {}", kind.label(), model.label()),
        };
    }

    TurnAction::BuildPlan {
        subject: format!("{} session on {}", kind.label(), model.label()),
    }
}

fn shell_command_from_input(input: &str) -> Option<String> {
    let trimmed = input.trim();
    if trimmed == "/run" {
        return Some(String::new());
    }
    let rest = trimmed.strip_prefix("/run")?;
    if rest.starts_with(char::is_whitespace) {
        Some(rest.trim_start().to_string())
    } else {
        None
    }
}

fn choice_command_from_input(input: &str) -> Option<String> {
    let trimmed = input.trim();
    if trimmed == "/choice" {
        return Some("the current task".to_string());
    }
    let rest = trimmed.strip_prefix("/choice")?;
    if rest.starts_with(char::is_whitespace) {
        let subject = rest.trim_start();
        Some(if subject.is_empty() {
            "the current task".to_string()
        } else {
            subject.to_string()
        })
    } else {
        None
    }
}

fn choice_group_block(subject: &str, kind: SessionKind, model: ModelKind) -> ChoiceGroupBlock {
    ChoiceGroupBlock {
        title: format!("Choose next step for {subject}"),
        submit_label: "Submit".to_string(),
        questions: vec![
            ChoiceQuestion {
                id: "scope".to_string(),
                mode: ChoiceMode::Single,
                prompt: format!("What should {} on {} focus on?", kind.label(), model.label()),
                options: vec![
                    ChoiceOption { id: "files".to_string(), label: "Files".to_string() },
                    ChoiceOption { id: "folders".to_string(), label: "Folders".to_string() },
                ],
            },
            ChoiceQuestion {
                id: "actions".to_string(),
                mode: ChoiceMode::Multi,
                prompt: "Which actions should be included?".to_string(),
                options: vec![
                    ChoiceOption { id: "plan".to_string(), label: "Plan".to_string() },
                    ChoiceOption { id: "review".to_string(), label: "Review".to_string() },
                    ChoiceOption { id: "shell".to_string(), label: "Shell".to_string() },
                ],
            },
        ],
    }
}

fn run_shell_command(command: &str, cancel_requested: Arc<AtomicBool>) -> CommandResult {
    let mut process = shell_process(command);
    process.stdout(Stdio::piped()).stderr(Stdio::piped());
    match process.spawn() {
        Ok(mut child) => {
            loop {
                if cancel_requested.load(Ordering::SeqCst) {
                    let _ = child.kill();
                    return CommandResult::interrupted(command.to_string());
                }

                match child.try_wait() {
                    Ok(Some(_)) => match child.wait_with_output() {
                        Ok(output) => return CommandResult::from_output(command.to_string(), output),
                        Err(error) => {
                            return CommandResult::from_launch_error(command.to_string(), error.to_string())
                        }
                    },
                    Ok(None) => thread::sleep(Duration::from_millis(25)),
                    Err(error) => {
                        return CommandResult::from_launch_error(command.to_string(), error.to_string())
                    }
                }
            }
        }
        Err(error) => CommandResult::from_launch_error(command.to_string(), error.to_string()),
    }
}

fn shell_process(command: &str) -> ProcessCommand {
    #[cfg(windows)]
    {
        let mut process = ProcessCommand::new("cmd");
        process.arg("/C").arg(command);
        process
    }

    #[cfg(not(windows))]
    {
        let mut process = ProcessCommand::new("sh");
        process.arg("-lc").arg(command);
        process
    }
}

fn chunk_text(text: &str) -> Vec<String> {
    let words: Vec<&str> = text.split_whitespace().collect();
    words
        .chunks(4)
        .map(|chunk| format!("{} ", chunk.join(" ")))
        .collect()
}

#[cfg(test)]
fn interruptable_command() -> String {
    #[cfg(windows)]
    {
        "ping 127.0.0.1 -n 5 > NUL".to_string()
    }

    #[cfg(not(windows))]
    {
        "sleep 2".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn begin_turn_ignores_empty_input() {
        let mut session = Session::new(SessionKind::Coder, ModelKind::HermesCode);

        assert!(session.begin_turn("   ").is_none());
    }

    #[test]
    fn choice_group_state_starts_unselected_and_submit_is_last() {
        let block = ChoiceGroupBlock {
            title: "Pick targets".to_string(),
            questions: vec![ChoiceQuestion {
                id: "scope".to_string(),
                mode: ChoiceMode::Single,
                prompt: "Scope?".to_string(),
                options: vec![
                    ChoiceOption { id: "files".to_string(), label: "Files".to_string() },
                    ChoiceOption { id: "folders".to_string(), label: "Folders".to_string() },
                ],
            }],
            submit_label: "Submit".to_string(),
        };

        let state = ChoiceGroupState::new(block);
        assert!(state.selected_answers().iter().all(|(_, selected)| selected.is_empty()));
        assert_eq!(state.focus, FocusTarget::Submit);
    }

    #[test]
    fn choice_group_state_moves_focus_and_toggles_selection() {
        let block = ChoiceGroupBlock {
            title: "Choose".to_string(),
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
        };

        let mut state = ChoiceGroupState::new(block);
        state.move_focus_up();
        assert_eq!(
            state.focus,
            FocusTarget::Question {
                question_index: 1,
                option_index: 1,
            }
        );
        state.move_focus_up();
        assert_eq!(
            state.focus,
            FocusTarget::Question {
                question_index: 1,
                option_index: 0,
            }
        );
        state.toggle_selected(0, 1);
        state.toggle_selected(1, 0);
        state.toggle_selected(1, 1);

        assert_eq!(
            state.selected_answers(),
            vec![
                ("mode".to_string(), vec!["safe".to_string()]),
                ("tags".to_string(), vec!["one".to_string(), "two".to_string()]),
            ]
        );
    }

    #[test]
    fn choice_group_payload_round_trips() {
        let json = r#"{
            "type":"choice_group",
            "title":"Pick",
            "submit_label":"Submit",
            "questions":[
                {
                    "id":"scope",
                    "mode":"Single",
                    "prompt":"Scope?",
                    "options":[
                        {"id":"files","label":"Files"},
                        {"id":"folders","label":"Folders"}
                    ]
                }
            ]
        }"#;

        let block = parse_choice_group_payload(json).expect("parse choice block");
        assert_eq!(block.title, "Pick");
        assert_eq!(block.submit_label, "Submit");
        assert_eq!(block.questions.len(), 1);
        assert_eq!(block.questions[0].id, "scope");
        assert_eq!(block.questions[0].options[0].id, "files");
    }

    #[test]
    fn choice_answers_are_serialized_as_json() {
        let payload = serialize_choice_answers(&[(
            "scope".to_string(),
            vec!["src/main.rs".to_string(), "src/ui.rs".to_string()],
        )]);

        assert!(payload.contains("\"answers\""));
        assert!(payload.contains("\"scope\""));
        assert!(payload.contains("src/main.rs"));
    }

    #[test]
    fn run_turn_preserves_the_requested_shell_command() {
        let mut session = Session::new(SessionKind::Coder, ModelKind::HermesCode);
        let mut turn = session
            .begin_turn(&format!("/run {}", interruptable_command()))
            .expect("expected a pending turn");

        while !turn.tick(&mut session) {}

        let transcript = session
            .messages
            .iter()
            .map(|message| message.content.as_str())
            .collect::<Vec<_>>()
            .join("\n");

        assert!(
            transcript.contains("tool: shell_command -> cargo test"),
            "transcript did not keep the requested shell command: {transcript}"
        );
    }

    #[test]
    fn parses_run_commands_with_tabs_and_multiple_spaces() {
        assert_eq!(
            shell_command_from_input("/run\tcargo test"),
            Some("cargo test".to_string())
        );
        assert_eq!(
            shell_command_from_input("/run    cargo test"),
            Some("cargo test".to_string())
        );
        assert_eq!(shell_command_from_input("/runtime"), None);
    }

    #[test]
    fn run_turn_emits_a_running_tool_message_before_finishing() {
        let mut session = Session::new(SessionKind::Coder, ModelKind::HermesCode);
        let mut turn = session
            .begin_turn(&format!("/run {}", interruptable_command()))
            .expect("expected a pending turn");

        while turn.tick(&mut session) == false {
            if session
                .messages
                .iter()
                .any(|message| message.content.contains("shell_command running"))
            {
                return;
            }
        }

        panic!("expected the turn to emit a running message before finishing");
    }

    #[test]
    fn command_result_transcript_uses_sections_for_output() {
        let result = CommandResult {
            command: "/run echo hi".to_string(),
            status_code: Some(0),
            stdout: "hi\nthere\n".to_string(),
            stderr: String::new(),
            execution_error: None,
        };

        let transcript = result.transcript();

        assert!(transcript.starts_with("tool: shell_command -> /run echo hi"));
        assert!(transcript.contains("status: exit status 0"));
        assert!(transcript.contains("stdout:\n  hi\n  there"));
        assert!(transcript.contains("stderr:\n  <empty>"));
    }

    #[test]
    fn tool_event_model_covers_read_write_execute() {
        let read = ToolEvent::new(ToolKind::Read, "workspace/src/main.rs");
        let write = ToolEvent::new(ToolKind::Write, "workspace/src/ui.rs");
        let execute = ToolEvent::new(ToolKind::Execute, "cargo test -p new-input-form");

        assert_eq!(read.kind, ToolKind::Read);
        assert_eq!(write.kind, ToolKind::Write);
        assert_eq!(execute.kind, ToolKind::Execute);
        assert_eq!(read.status, ToolStatus::Idle);
        assert!(ClaudeCodeDashboardState::new().activity_log.is_empty());
    }

    #[test]
    fn interrupting_a_run_turn_marks_it_as_cancelled() {
        let mut session = Session::new(SessionKind::Coder, ModelKind::HermesCode);
        let mut turn = session
            .begin_turn(&format!("/run {}", interruptable_command()))
            .expect("expected a pending turn");

        while turn.tick(&mut session) == false {
            if session
                .messages
                .iter()
                .any(|message| message.content.contains("shell_command running"))
            {
                break;
            }
        }
        turn.request_interrupt();

        let mut finished = false;
        for _ in 0..400 {
            if turn.tick(&mut session) {
                finished = true;
                break;
            }
        }

        assert!(finished, "interrupted turn did not finish");
        let transcript = session
            .messages
            .iter()
            .map(|message| message.content.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            transcript.contains("command interrupted by user"),
            "missing interrupted marker in transcript: {transcript}"
        );
    }
}
