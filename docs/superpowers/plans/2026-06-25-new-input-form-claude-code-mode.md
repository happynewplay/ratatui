# new-input-form Claude Code Mode 实现计划

> **面向 AI 代理的工作者：** 必需子技能：使用 superpowers:subagent-driven-development（推荐）或 superpowers:executing-plans 逐任务实现此计划。步骤使用复选框（`- [ ]`）语法来跟踪进度。

**目标：** 在 `examples/apps/new-input-form` 的菜单末尾新增 `Claude Code` 入口，选择后进入一个 workspace 绑定的分段工具面板，实时展示 `read`、`write`、`execute` 三类工具状态和活动日志。

**架构：** 现有的 `SessionSelect -> ModelSelect -> Chat` 流程保持不变，只新增一个 `SessionSelect -> ClaudeCodeDashboard` 分支。新 dashboard 负责展示工具状态，不负责调度逻辑；调度和真实执行仍由 `app` 与现有 shell/文件操作路径产生并更新。为避免 `ui.rs` 继续膨胀，新 dashboard 应拆出专用渲染函数或模块，并引入一个统一的工具事件/状态类型来承载三类工具的共用字段。

**技术栈：** Rust, `crossterm`, `ratatui`, `tui-input`, `serde`, `serde_json`

---

## 文件结构

- 修改：`examples/apps/new-input-form/src/app.rs`
  - 增加 `ClaudeCodeDashboard` 状态，处理菜单跳转、返回菜单、以及 Claude Code 模式的焦点/事件分发。
- 修改：`examples/apps/new-input-form/src/ui.rs`
  - 新增 Claude Code dashboard 的渲染函数，负责头部、三个工具块和活动日志的布局与着色。
- 修改：`examples/apps/new-input-form/src/agent.rs`
  - 增加统一的工具事件/状态模型，承载 read/write/execute 的状态、目标、结果、错误和耗时。
  - 复用现有 shell 执行路径的结果格式，确保 execute 块显示真实输出。
- 修改：`examples/apps/new-input-form/src/input_commands.rs`
  - 如需从菜单或 dashboard 触发 workspace 内文件读取/写入命令，补充必要的命令构造或路径筛选辅助函数。
- 修改：`examples/apps/new-input-form/src/main.rs`
  - 通常不需要改；仅在启动流程需要传递额外上下文时再改。
- 测试：`examples/apps/new-input-form/src/app.rs`
  - 菜单跳转、返回菜单、焦点切换、状态保留的内联测试。
- 测试：`examples/apps/new-input-form/src/ui.rs`
  - dashboard 渲染测试，验证三个工具块和活动日志的文本输出。
- 测试：`examples/apps/new-input-form/src/agent.rs`
  - 工具事件结构、状态映射、结果序列化或摘要生成的内联测试。

---

### 任务 1：定义 Claude Code dashboard 的工具状态模型

**文件：**
- 修改：`examples/apps/new-input-form/src/agent.rs`
- 测试：`examples/apps/new-input-form/src/agent.rs` 内联单测

- [ ] **步骤 1：编写失败的测试**

```rust
#[test]
fn tool_event_model_covers_read_write_execute() {
    let read = ToolEvent::new(ToolKind::Read, "workspace/src/main.rs");
    let write = ToolEvent::new(ToolKind::Write, "workspace/src/ui.rs");
    let execute = ToolEvent::new(ToolKind::Execute, "cargo test -p new-input-form");

    assert_eq!(read.kind, ToolKind::Read);
    assert_eq!(write.kind, ToolKind::Write);
    assert_eq!(execute.kind, ToolKind::Execute);
    assert_eq!(read.status, ToolStatus::Idle);
}
```

- [ ] **步骤 2：运行测试验证失败**

运行：`cargo test -p new-input-form agent::tests::tool_event_model_covers_read_write_execute -- --exact --nocapture --test-threads=1`
预期：FAIL，报错 `ToolEvent` / `ToolKind` / `ToolStatus` 未定义。

- [ ] **步骤 3：编写最少实现代码**

```rust
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
}

impl ToolEvent {
    pub fn new(kind: ToolKind, target: impl Into<String>) -> Self {
        Self {
            kind,
            status: ToolStatus::Idle,
            target: target.into(),
            summary: String::new(),
            error: None,
        }
    }
}
```

- [ ] **步骤 4：运行测试验证通过**

运行：`cargo test -p new-input-form agent::tests::tool_event_model_covers_read_write_execute -- --exact --nocapture --test-threads=1`
预期：PASS。

- [ ] **步骤 5：Commit**

```bash
git add examples/apps/new-input-form/src/agent.rs
git commit -m "feat: add claude code tool event model"
```

### 任务 2：把 session 菜单末尾接到 Claude Code dashboard

**文件：**
- 修改：`examples/apps/new-input-form/src/app.rs`
- 修改：`examples/apps/new-input-form/src/ui.rs`
- 测试：`examples/apps/new-input-form/src/app.rs` 内联单测

- [ ] **步骤 1：编写失败的测试**

```rust
#[test]
fn session_select_lists_claude_code_last_and_enters_dashboard() {
    let mut app = App::default();
    app.state = AppState::SessionSelect;

    assert_eq!(SessionKind::ALL.last().copied(), Some(SessionKind::ClaudeCode));
    app.session_index = SessionKind::ALL.len() - 1;
    app.handle_session_select(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    assert!(matches!(app.state, AppState::ClaudeCodeDashboard));
}
```

- [ ] **步骤 2：运行测试验证失败**

运行：`cargo test -p new-input-form app::tests::session_select_lists_claude_code_last_and_enters_dashboard -- --exact --nocapture --test-threads=1`
预期：FAIL，报错 `ClaudeCodeDashboard` 未定义或菜单分支缺失。

- [ ] **步骤 3：编写最少实现代码**

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AppState {
    SessionSelect,
    ModelSelect,
    Chat,
    ClaudeCodeDashboard,
}

impl SessionKind {
    pub const ALL: [Self; 4] = [Self::Planner, Self::Coder, Self::Reviewer, Self::ClaudeCode];
}

// In handle_session_select:
match key.code {
    KeyCode::Enter => {
        if self.session_index == SessionKind::ALL.len() - 1 {
            self.state = AppState::ClaudeCodeDashboard;
        } else {
            self.state = AppState::ModelSelect;
        }
    }
    _ => {}
}
```

- [ ] **步骤 4：运行测试验证通过**

运行：`cargo test -p new-input-form app::tests::session_select_lists_claude_code_last_and_enters_dashboard -- --exact --nocapture --test-threads=1`
预期：PASS。

- [ ] **步骤 5：Commit**

```bash
git add examples/apps/new-input-form/src/app.rs examples/apps/new-input-form/src/ui.rs
git commit -m "feat: add claude code menu entry"
```

### 任务 3：实现 Claude Code dashboard 的分段渲染

**文件：**
- 修改：`examples/apps/new-input-form/src/ui.rs`
- 测试：`examples/apps/new-input-form/src/ui.rs` 内联单测

- [ ] **步骤 1：编写失败的测试**

```rust
#[test]
fn renders_claude_code_dashboard_with_three_tool_blocks() {
    let dashboard = ClaudeCodeDashboardState::new();
    let rendered = render_claude_code_dashboard_for_test(&dashboard);

    assert!(rendered.contains("Read"));
    assert!(rendered.contains("Write"));
    assert!(rendered.contains("Execute"));
    assert!(rendered.contains("Activity log"));
}
```

- [ ] **步骤 2：运行测试验证失败**

运行：`cargo test -p new-input-form ui::tests::renders_claude_code_dashboard_with_three_tool_blocks -- --exact --nocapture --test-threads=1`
预期：FAIL，报错 dashboard 渲染函数或状态类型未定义。

- [ ] **步骤 3：编写最少实现代码**

```rust
pub fn render_claude_code_dashboard(frame: &mut Frame, state: &ClaudeCodeDashboardState) {
    let layout = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(1),
        Constraint::Length(8),
    ]);
    let [header_area, main_area, log_area] = frame.area().layout(&layout);

    frame.render_widget(Block::bordered().title("Claude Code"), header_area);
    // render three tool blocks in the main area and the activity log at the bottom
}
```

- [ ] **步骤 4：运行测试验证通过**

运行：`cargo test -p new-input-form ui::tests::renders_claude_code_dashboard_with_three_tool_blocks -- --exact --nocapture --test-threads=1`
预期：PASS。

- [ ] **步骤 5：Commit**

```bash
git add examples/apps/new-input-form/src/ui.rs examples/apps/new-input-form/src/agent.rs
git commit -m "feat: render claude code dashboard"
```

### 任务 4：把真实 read / write / execute 事件映射到面板状态

**文件：**
- 修改：`examples/apps/new-input-form/src/app.rs`
- 修改：`examples/apps/new-input-form/src/agent.rs`
- 测试：`examples/apps/new-input-form/src/app.rs` 内联单测

- [ ] **步骤 1：编写失败的测试**

```rust
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
    });

    assert_eq!(app.claude_code_state.execute.status, ToolStatus::Running);
}
```

- [ ] **步骤 2：运行测试验证失败**

运行：`cargo test -p new-input-form app::tests::execute_tool_event_updates_dashboard_state -- --exact --nocapture --test-threads=1`
预期：FAIL，报错 `apply_tool_event` 或 dashboard 状态字段缺失。

- [ ] **步骤 3：编写最少实现代码**

```rust
impl App {
    fn apply_tool_event(&mut self, event: ToolEvent) {
        match event.kind {
            ToolKind::Read => self.claude_code_state.read = event,
            ToolKind::Write => self.claude_code_state.write = event,
            ToolKind::Execute => self.claude_code_state.execute = event,
        }
        self.claude_code_state.activity_log.push(event);
    }
}
```

- [ ] **步骤 4：运行测试验证通过**

运行：`cargo test -p new-input-form app::tests::execute_tool_event_updates_dashboard_state -- --exact --nocapture --test-threads=1`
预期：PASS。

- [ ] **步骤 5：Commit**

```bash
git add examples/apps/new-input-form/src/app.rs examples/apps/new-input-form/src/agent.rs
git commit -m "feat: wire claude code tool events"
```

### 任务 5：补齐 workspace 限定的文件读写与 shell 执行边界

**文件：**
- 修改：`examples/apps/new-input-form/src/input_commands.rs`
- 修改：`examples/apps/new-input-form/src/app.rs`
- 测试：`examples/apps/new-input-form/src/input_commands.rs` 内联单测

- [ ] **步骤 1：编写失败的测试**

```rust
#[test]
fn workspace_file_picker_filters_to_workspace_paths() {
    let picker = CommandPicker::files("/workspace");
    assert!(picker.visible_items().iter().all(|item| {
        item.secondary
            .as_deref()
            .map(|value| value.starts_with("src") || value.starts_with("docs") || value.is_empty())
            .unwrap_or(true)
    }));
}
```

- [ ] **步骤 2：运行测试验证失败**

运行：`cargo test -p new-input-form input_commands::tests::workspace_file_picker_filters_to_workspace_paths -- --exact --nocapture --test-threads=1`
预期：FAIL，报错 workspace 过滤逻辑未定义或不完整。

- [ ] **步骤 3：编写最少实现代码**

```rust
fn relative_path(root: &Path, path: &Path) -> PathBuf {
    path.strip_prefix(root)
        .map(Path::to_path_buf)
        .unwrap_or_else(|_| path.to_path_buf())
}
```

- [ ] **步骤 4：运行测试验证通过**

运行：`cargo test -p new-input-form input_commands::tests::workspace_file_picker_filters_to_workspace_paths -- --exact --nocapture --test-threads=1`
预期：PASS。

- [ ] **步骤 5：Commit**

```bash
git add examples/apps/new-input-form/src/input_commands.rs examples/apps/new-input-form/src/app.rs
git commit -m "feat: constrain claude code workspace paths"
```

### 任务 6：补最终回归测试并收尾

**文件：**
- 修改：`examples/apps/new-input-form/src/app.rs`
- 修改：`examples/apps/new-input-form/src/ui.rs`
- 修改：`examples/apps/new-input-form/src/agent.rs`
- 测试：现有内联测试 + 新增回归测试

- [ ] **步骤 1：编写失败的测试**

```rust
#[test]
fn claude_code_dashboard_returns_to_menu_with_escape() {
    let mut app = App::default();
    app.state = AppState::ClaudeCodeDashboard;

    let should_quit = app.handle_claude_code(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));

    assert!(!should_quit);
    assert!(matches!(app.state, AppState::SessionSelect));
}
```

- [ ] **步骤 2：运行测试验证失败**

运行：`cargo test -p new-input-form app::tests::claude_code_dashboard_returns_to_menu_with_escape -- --exact --nocapture --test-threads=1`
预期：FAIL，报错 dashboard 退出处理未实现。

- [ ] **步骤 3：编写最少实现代码**

```rust
fn handle_claude_code(&mut self, key: KeyEvent) -> bool {
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') => {
            self.state = AppState::SessionSelect;
            true
        }
        _ => false,
    }
}
```

- [ ] **步骤 4：运行测试验证通过**

运行：`cargo test -p new-input-form app::tests::claude_code_dashboard_returns_to_menu_with_escape -- --exact --nocapture --test-threads=1`
预期：PASS。

- [ ] **步骤 5：Commit**

```bash
git add examples/apps/new-input-form/src/app.rs examples/apps/new-input-form/src/ui.rs examples/apps/new-input-form/src/agent.rs examples/apps/new-input-form/src/input_commands.rs
git commit -m "feat: complete claude code dashboard flow"
```

## 自检

**规格覆盖度：**
- 菜单末尾新增 `Claude Code`：任务 2
- 直接进入新的 dashboard 状态：任务 2
- 分段工具面板布局：任务 3
- 真实 read / write / execute 状态映射：任务 4
- workspace 约束：任务 5
- 退出与返回菜单：任务 6

**占位符扫描：**
- 未使用 “待定” / “TODO” / “后续实现” / “补充细节” 等占位说法。
- 每个任务都包含具体测试、代码和 commit。

**类型一致性：**
- `ToolKind` / `ToolStatus` / `ToolEvent` 在任务 1 中定义，后续任务复用同名类型。
- `ClaudeCodeDashboardState`、`AppState::ClaudeCodeDashboard`、`handle_claude_code`、`apply_tool_event` 在后续任务中保持一致命名。
- 计划中的包名使用 `new-input-form`，与 `examples/apps/new-input-form/Cargo.toml` 一致。

**范围检查：**
- 计划只覆盖 `examples/apps/new-input-form`。
- 没有把任务扩展到其他示例或项目级 runtime。
- 该计划可以单独落地并验证，不需要再拆分成更小的规格。
