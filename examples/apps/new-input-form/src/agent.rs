use std::process::{Command as ProcessCommand, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionKind {
    Planner,
    Coder,
    Reviewer,
}

impl SessionKind {
    pub const ALL: [Self; 3] = [Self::Planner, Self::Coder, Self::Reviewer];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Planner => "Planner",
            Self::Coder => "Coder",
            Self::Reviewer => "Reviewer",
        }
    }

    pub const fn summary(self) -> &'static str {
        match self {
            Self::Planner => "Break work into steps and scope.",
            Self::Coder => "Implement code and validate changes.",
            Self::Reviewer => "Inspect diffs and note risks.",
        }
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

#[derive(Debug, Clone)]
pub struct Session {
    pub kind: SessionKind,
    pub model: ModelKind,
    pub messages: Vec<Message>,
}

impl Session {
    pub fn new(kind: SessionKind, model: ModelKind) -> Self {
        let mut session = Self {
            kind,
            model,
            messages: vec![],
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
                TurnAction::ReviewDiff { .. } | TurnAction::BuildPlan { .. } => {}
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
                _ => {
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
        }
    }

    fn start_message(&self) -> String {
        match self {
            Self::ShellCommand { command } => format!("tool: shell_command -> {command}"),
            Self::ReviewDiff { target } => format!("tool: code_review -> {target}"),
            Self::BuildPlan { subject } => format!("tool: planner -> {subject}"),
        }
    }

    fn running_message(&self) -> String {
        match self {
            Self::ShellCommand { command } => format!("tool: shell_command running -> {command}"),
            Self::ReviewDiff { target } => format!("tool: code_review running -> {target}"),
            Self::BuildPlan { subject } => format!("tool: planner running -> {subject}"),
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
