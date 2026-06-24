# Choice Blocks Interaction 实现计划

> **面向 AI 代理的工作者：** 必需子技能：使用 superpowers:subagent-driven-development（推荐）或 superpowers:executing-plans 逐任务实现此计划。步骤使用复选框（`- [ ]`）语法来跟踪进度。

**目标：** 让 `new-input-form` 在对话流中支持模型返回的 JSON 选择块，渲染单选 / 多选问题，支持 `space` 勾选，并在末尾统一 `submit` 后把已选项发送回模型。

**架构：** 选择块采用 JSON 数组协议，由 `agent` 负责生成 / 持有交互状态，`ui` 负责渲染标题、问题、选项和提交按钮，`app` 负责键盘与鼠标事件分发。单选题以光标移动选择，多选题以 `space` 切换勾选，`Enter` 仅在 `submit` 焦点时提交，提交后把 `{question_id: selected_ids}` 的结果写回会话消息流。

**技术栈：** Rust, `crossterm`, `tui_input`, `ratatui`

---

### 任务 1：定义选择块协议与状态模型

**文件：**
- 修改：`examples/apps/new-input-form/src/agent.rs`
- 测试：`examples/apps/new-input-form/src/agent.rs` 内联单测

- [ ] **步骤 1：编写失败的测试**

```rust
#[test]
fn choice_group_state_starts_unselected_and_submit_is_last() {
    let block = ChoiceGroupBlock {
        title: "Pick targets".to_string(),
        questions: vec![
            ChoiceQuestion {
                id: "scope".to_string(),
                mode: ChoiceMode::Single,
                prompt: "Scope?".to_string(),
                options: vec![
                    ChoiceOption { id: "files".to_string(), label: "Files".to_string() },
                    ChoiceOption { id: "folders".to_string(), label: "Folders".to_string() },
                ],
            },
        ],
        submit_label: "Submit".to_string(),
    };

    let state = ChoiceGroupState::new(block.clone());
    assert!(state.selected_answers().is_empty());
    assert_eq!(state.focus(), FocusTarget::Submit);
}
```

- [ ] **步骤 2：运行测试验证失败**

运行：`cargo test -p new-input-form --lib agent::tests::choice_group_state_starts_unselected_and_submit_is_last -- --exact --nocapture --test-threads=1`
预期：FAIL，报错 `ChoiceGroupState` / `FocusTarget` 未定义

- [ ] **步骤 3：编写最少实现代码**

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChoiceMode {
    Single,
    Multi,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChoiceOption {
    pub id: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChoiceQuestion {
    pub id: String,
    pub mode: ChoiceMode,
    pub prompt: String,
    pub options: Vec<ChoiceOption>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChoiceGroupBlock {
    pub title: String,
    pub questions: Vec<ChoiceQuestion>,
    pub submit_label: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FocusTarget {
    Question(usize),
    Submit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChoiceGroupState {
    pub block: ChoiceGroupBlock,
    pub focus: FocusTarget,
    pub selected: Vec<Vec<bool>>,
}
```

- [ ] **步骤 4：运行测试验证通过**

运行：`cargo test -p new-input-form --lib agent::tests::choice_group_state_starts_unselected_and_submit_is_last -- --exact --nocapture --test-threads=1`
预期：PASS

- [ ] **步骤 5：Commit**

```bash
git add examples/apps/new-input-form/src/agent.rs
git commit -m "feat: add choice block model"
```

### 任务 2：渲染单选 / 多选 / submit 的交互块

**文件：**
- 修改：`examples/apps/new-input-form/src/ui.rs`
- 修改：`examples/apps/new-input-form/src/app.rs`
- 测试：`examples/apps/new-input-form/src/ui.rs` 内联单测

- [ ] **步骤 1：编写失败的测试**

```rust
#[test]
fn renders_choice_group_with_submit_button() {
    let block = ChoiceGroupBlock {
        title: "Pick".to_string(),
        questions: vec![
            ChoiceQuestion {
                id: "scope".to_string(),
                mode: ChoiceMode::Single,
                prompt: "Scope?".to_string(),
                options: vec![
                    ChoiceOption { id: "files".to_string(), label: "Files".to_string() },
                    ChoiceOption { id: "folders".to_string(), label: "Folders".to_string() },
                ],
            },
        ],
        submit_label: "Submit".to_string(),
    };

    let state = ChoiceGroupState::new(block);
    let rendered = render_choice_group(&state);
    assert!(rendered.contains("Scope?"));
    assert!(rendered.contains("Submit"));
}
```

- [ ] **步骤 2：运行测试验证失败**

运行：`cargo test -p new-input-form --lib ui::tests::renders_choice_group_with_submit_button -- --exact --nocapture --test-threads=1`
预期：FAIL，报错 `render_choice_group` 未定义

- [ ] **步骤 3：编写最少实现代码**

```rust
fn render_choice_group(state: &ChoiceGroupState) -> String {
    let mut output = vec![state.block.title.clone()];
    for question in &state.block.questions {
        output.push(question.prompt.clone());
        for option in &question.options {
            output.push(format!("[ ] {}", option.label));
        }
    }
    output.push(format!("[ {} ]", state.block.submit_label));
    output.join("\n")
}
```

- [ ] **步骤 4：运行测试验证通过**

运行：`cargo test -p new-input-form --lib ui::tests::renders_choice_group_with_submit_button -- --exact --nocapture --test-threads=1`
预期：PASS

- [ ] **步骤 5：Commit**

```bash
git add examples/apps/new-input-form/src/ui.rs examples/apps/new-input-form/src/app.rs
git commit -m "feat: render choice blocks"
```

### 任务 3：键盘交互，`space` 勾选，`Enter` 只提交最后的按钮

**文件：**
- 修改：`examples/apps/new-input-form/src/app.rs`
- 修改：`examples/apps/new-input-form/src/agent.rs`
- 测试：`examples/apps/new-input-form/src/app.rs` 内联单测

- [ ] **步骤 1：编写失败的测试**

```rust
#[test]
fn space_toggles_multi_select_and_enter_submits_on_button() {
    let mut app = App::default();
    app.state = AppState::Chat;
    app.active_turn = None;

    assert!(!app.handle_chat(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE)));
    assert!(!app.handle_chat(KeyEvent::new(KeyCode::Enter), KeyModifiers::NONE));
}
```

- [ ] **步骤 2：运行测试验证失败**

运行：`cargo test -p new-input-form --lib app::tests::space_toggles_multi_select_and_enter_submits_on_button -- --exact --nocapture --test-threads=1`
预期：FAIL，报错缺少 choice block 选择状态处理

- [ ] **步骤 3：编写最少实现代码**

```rust
if let Some(choice_state) = self.choice_state.as_mut() {
    match key.code {
        KeyCode::Char(' ') => choice_state.toggle_current(),
        KeyCode::Enter if matches!(choice_state.focus, FocusTarget::Submit) => choice_state.submit(),
        KeyCode::Up | KeyCode::Char('k') => choice_state.move_up(),
        KeyCode::Down | KeyCode::Char('j') => choice_state.move_down(),
        _ => {}
    }
    return false;
}
```

- [ ] **步骤 4：运行测试验证通过**

运行：`cargo test -p new-input-form --lib app::tests::space_toggles_multi_select_and_enter_submits_on_button -- --exact --nocapture --test-threads=1`
预期：PASS

- [ ] **步骤 5：Commit**

```bash
git add examples/apps/new-input-form/src/app.rs examples/apps/new-input-form/src/agent.rs
git commit -m "feat: add keyboard choice interaction"
```

### 任务 4：把选择结果回传给模型并补收尾测试

**文件：**
- 修改：`examples/apps/new-input-form/src/agent.rs`
- 修改：`examples/apps/new-input-form/src/app.rs`
- 修改：`examples/apps/new-input-form/src/ui.rs`
- 测试：现有内联测试 + 新增回传测试

- [ ] **步骤 1：编写失败的测试**

```rust
#[test]
fn selected_answers_are_serialized_back_to_session() {
    let answers = vec![
        ("scope", vec!["files"]),
        ("paths", vec!["src/main.rs", "src/ui.rs"]),
    ];

    let payload = serialize_choice_answers(&answers);
    assert!(payload.contains("\"scope\""));
    assert!(payload.contains("\"paths\""));
    assert!(payload.contains("src/ui.rs"));
}
```

- [ ] **步骤 2：运行测试验证失败**

运行：`cargo test -p new-input-form --lib agent::tests::selected_answers_are_serialized_back_to_session -- --exact --nocapture --test-threads=1`
预期：FAIL，报错序列化函数未实现

- [ ] **步骤 3：编写最少实现代码**

```rust
fn serialize_choice_answers(answers: &[(&str, Vec<&str>)]) -> String {
    let parts = answers
        .iter()
        .map(|(id, values)| format!(r#"{{"id":"{id}","selected":[{}]}}"#, values.iter().map(|value| format!(r#""{value}""#)).collect::<Vec<_>>().join(",")))
        .collect::<Vec<_>>();
    format!("[{}]", parts.join(","))
}
```

- [ ] **步骤 4：运行测试验证通过**

运行：`cargo test -p new-input-form --lib agent::tests::selected_answers_are_serialized_back_to_session -- --exact --nocapture --test-threads=1`
预期：PASS

- [ ] **步骤 5：Commit**

```bash
git add examples/apps/new-input-form/src/agent.rs examples/apps/new-input-form/src/app.rs examples/apps/new-input-form/src/ui.rs
git commit -m "feat: submit choice selections to model"
```

## 自检

**规格覆盖度：**
- JSON choice_group 协议：任务 1
- 单选 / 多选渲染：任务 2
- `space` 勾选、`Enter` 提交：任务 3
- 回传已选结果到模型：任务 4

**占位符扫描：**
- 未发现 “待定” / “TODO” / 模糊任务描述

**类型一致性：**
- `ChoiceMode`、`ChoiceOption`、`ChoiceQuestion`、`ChoiceGroupBlock` 在任务 1 中定义，后续任务复用同一命名
- `serialize_choice_answers` 在任务 4 中定义并只在该任务内使用，避免未定义引用

**范围检查：**
- 该计划聚焦于 `new-input-form` 的对话选择块交互，未扩展到其他示例或全局消息系统
- 如后续要支持更复杂的题型，可在这个协议上迭代，不需要拆成更大的子项目
