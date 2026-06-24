# Ratatui 组件使用文档

本文档介绍 Ratatui 各组件的使用方法和代码示例。

## 快速开始

### 基本模板

每个 Ratatui 应用的最低要求结构：

```rust
use color_eyre::Result;
use crossterm::event::{self, KeyCode};
use ratatui::widgets::Paragraph;
use ratatui::{DefaultTerminal, Frame};

fn main() -> Result<()> {
    color_eyre::install()?;
    ratatui::run(run)
}

fn run(terminal: &mut DefaultTerminal) -> Result<()> {
    loop {
        terminal.draw(render)?;
        if should_quit()? {
            break;
        }
    }
    Ok(())
}

fn render(frame: &mut Frame) {
    let greeting = Paragraph::new("Hello World!");
    frame.render_widget(greeting, frame.area());
}

fn should_quit() -> Result<bool> {
    use std::time::Duration;
    use color_eyre::eyre::Context;
    if event::poll(Duration::from_millis(250)).context("event poll failed")? {
        let q_pressed = event::read()
            .context("event read failed")?
            .as_key_press_event()
            .is_some_and(|key| key.code == KeyCode::Char('q'));
        Ok(q_pressed)
    } else {
        Ok(false)
    }
}
```

---

## 布局系统

### Layout — 基础布局

使用 `Layout` 将 `Rect` 分割为多个子区域：

```rust
use ratatui::{Frame, layout::{Layout, Constraint, Direction, Flex}};

fn render(frame: &mut Frame) {
    // 垂直分割：顶部标题行 + 下方主内容区
    let main_layout = Layout::vertical([
        Constraint::Length(3),    // 固定高度
        Constraint::Min(0),       // 剩余空间
    ]);

    let [title_area, body_area] = frame.area().layout(&main_layout);

    // 水平分割：左右侧栏
    let body_layout = Layout::horizontal([
        Constraint::Length(20),   // 左侧固定宽度
        Constraint::Min(0),       // 右侧剩余
    ]);

    let [sidebar, content] = body_area.layout(&body_layout);

    frame.render_widget(Paragraph::new("Sidebar"), sidebar);
    frame.render_widget(Paragraph::new("Content"), content);
}
```

### Constraint — 约束类型

```rust
use ratatui::layout::Constraint;

// 固定长度
Constraint::Length(10)        // 10 个字符

// 百分比
Constraint::Percentage(50)    // 50%

// 比率
Constraint::Ratio(1, 3)       // 1:3

// 最小值
Constraint::Min(5)            // 至少 5

// 最大值
Constraint::Max(80)           // 至多 80

// 填充（Flexbox 风格）
Constraint::Fill(1)           // 按比例填充
Constraint::Fill(2)           // 2 倍权重

// 快捷转换
Constraint::from(10)          // 等同于 Length(10)
```

### Flex — 对齐方式

```rust
use ratatui::layout::Flex;

Layout::horizontal([
    Constraint::Length(20),
    Constraint::Min(0),
]).flex(Flex::Center);        // 居中对齐

.flex(Flex::Start);           // 起始对齐（默认）
.flex(Flex::End);             // 末尾对齐
.flex(Flex::Legacy);          // 旧版布局行为
```

### Rect — 区域操作

```rust
use ratatui::layout::Rect;

// 创建
let area = Rect::new(x, y, width, height);

// 常用方法
area.area();                  // 面积 (width * height)
area.left();                   // x 坐标
area.right();                  // x + width
area.top();                    // y 坐标
area.bottom();                 // y + height

// 布局分割
let [left, right] = area.layout(&Layout::horizontal([
    Constraint::Percentage(30),
    Constraint::Percentage(70),
]));

// 居中
let centered = area.centered(Constraint::Length(40));

// 限制在范围内
let clamped = area.clamp(Rect::new(0, 0, u16::MAX, u16::MAX));
```

---

## 基础组件

### Paragraph — 段落

```rust
use ratatui::{
    widgets::{Paragraph, Block},
    text::{Line, Span, Text},
    style::{Style, Color, Stylize, Modifier},
};

// 基本用法
frame.render_widget(
    Paragraph::new("Hello World!"),
    frame.area(),
);

// 多行文本
frame.render_widget(
    Paragraph::new(Text::from(vec![
        Line::from("第一行"),
        Line::from("第二行"),
    ])),
    frame.area(),
);

// 带样式
frame.render_widget(
    Paragraph::new("带样式的文本")
        .style(Style::new().fg(Color::Yellow))
        .bold(),
    frame.area(),
);

// 带边框
frame.render_widget(
    Paragraph::new("内容")
        .block(Block::bordered().title("标题")),
    frame.area(),
);

// 带滚动
frame.render_widget(
    Paragraph::new("很长的内容...")
        .scroll((3, 0)),  // (y, x) 偏移
    frame.area(),
);

// 对齐 — 显式指定
use ratatui::layout::Alignment;
Paragraph::new("居中对齐").alignment(Alignment::Center);
Paragraph::new("右对齐").alignment(Alignment::Right);

// 对齐 — 快捷方法
Paragraph::new("左对齐").left_aligned();
Paragraph::new("居中对齐").centered();
Paragraph::new("右对齐").right_aligned();

// 自动换行
Paragraph::new("这是一段很长的文本，在宽度不够时会自动换行。")
    .wrap(ratatui::widgets::Wrap { trim: true });

// 尺寸测量
let para = Paragraph::new("Hello World");
let line_count = para.line_count(20);  // 在宽度为 20 时的行数
let line_width = para.line_width();     // 最长行的宽度
```

### Block — 块容器

```rust
use ratatui::{
    widgets::{Block, Borders, Padding},
    text::Line,
    layout::Alignment,
};

// 基本边框
Block::bordered();                    // 四边边框

Block::new().borders(Borders::ALL);   // 同上
Block::new().borders(Borders::LEFT);  // 左边框
Block::new().borders(Borders::RIGHT); // 右边框
Block::new().borders(Borders::TOP);   // 上边框
Block::new().borders(Borders::BOTTOM);// 下边框

// 自定义边框样式
Block::new()
    .borders(Borders::ALL)
    .border_type(ratatui::symbols::border::Double)
    .border_style(Style::new().fg(Color::Cyan));

// 标题 — 位置
Block::bordered()
    .title("标题")                    // 顶部标题
    .title_top("顶部标题")            // 明确指定顶部
    .title_bottom("底部标题");        // 底部标题

// 标题 — 样式和对齐
Block::bordered()
    .title(Line::from("左标题").alignment(Alignment::Left))
    .title(Line::from("右标题").alignment(Alignment::Right))
    .title_style(Style::new().fg(Color::Yellow))   // 所有标题共用样式
    .title_alignment(Alignment::Center);           // 所有标题共用对齐

// 内边距
Block::bordered()
    .padding(Padding::new(right, left, top, bottom));

// 背景色
Block::bordered().style(Style::new().bg(Color::Blue));

// 圆角
Block::bordered()
    .border_set(ratatui::symbols::border::Rounded);

// 加粗边框
Block::bordered()
    .border_set(ratatui::symbols::border::Thick);

// 阴影
Block::bordered()
    .shadow(ratatui::widgets::Shadow::default());

// 获取内部区域（不含边框和内边距）
let block = Block::bordered().padding(Padding::uniform(1));
let inner_area = block.inner(frame.area());

// 查询标题
block.has_title_at_position(ratatui::widgets::TitlePosition::Top);

// 组合使用
Block::bordered()
    .title("状态栏")
    .border_style(Style::new().fg(Color::Cyan))
    .padding(Padding::uniform(1))
    .style(Style::new().bg(Color::Black));
```

---

## 数据展示组件

### List — 列表

```rust
use ratatui::{
    widgets::{List, ListItem, ListState},
    style::{Style, Color, Stylize},
};

// 基本用法
let items = vec![
    ListItem::new("项目 1"),
    ListItem::new("项目 2"),
    ListItem::new("项目 3"),
];
let list = List::new(items);
frame.render_widget(list, frame.area());

// 带状态（可选中项）
let mut list_state = ListState::default();
list_state.select(Some(0));  // 选中第一项

let list = List::new(items)
    .highlight_style(Style::new().bg(Color::Blue).fg(Color::White))
    .highlight_symbol(">> ");

frame.render_stateful_widget(list, frame.area(), &mut list_state);

// 方向
list.direction(ratatui::widgets::ListDirection::TopToBottom);
list.direction(ratatui::widgets::ListDirection::BottomToTop);

// 滚动偏移
let mut list_state = ListState::default();
list_state.select(Some(5));
list_state.set_offset(5);

// 列表导航
fn handle_key(key: KeyCode, state: &mut ListState, item_count: usize) {
    match key {
        KeyCode::Down => {
            if let Some(i) = state.selected() {
                if i < item_count - 1 {
                    state.select(Some(i + 1));
                }
            } else {
                state.select(Some(0));
            }
        }
        KeyCode::Up => {
            if let Some(i) = state.selected() {
                if i > 0 {
                    state.select(Some(i - 1));
                }
            } else {
                state.select(Some(0));
            }
        }
        _ => {}
    }
}
```

### Table — 表格

```rust
use ratatui::{
    widgets::{Table, Row, Cell, TableState},
    layout::Constraint,
    style::{Style, Color, Stylize},
};

// 基本用法
let rows = vec![
    Row::new(["第一行", "第一列", "数据"]),
    Row::new(["第二行", "第二列", "数据"]),
];
let widths = [
    Constraint::Length(10),
    Constraint::Percentage(50),
    Constraint::Percentage(50),
];

let table = Table::new(rows, widths);
frame.render_widget(table, frame.area());

// 带表头
let header = Row::new(["名称", "状态", "描述"])
    .style(Style::new().bold().fg(Color::Yellow))
    .bottom_margin(1);

let table = Table::new(rows, widths)
    .header(header);

// 带选中行
let mut table_state = TableState::default();
table_state.select(Some(0));

let table = Table::new(rows, widths)
    .highlight_style(Style::new().bg(Color::Blue).fg(Color::White))
    .highlight_symbol(">>")
    .highlight_spacing(ratatui::widgets::HighlightSpacing::Always);

frame.render_stateful_widget(table, frame.area(), &mut table_state);

// 列分割线
Table::new(rows, widths)
    .column_spacing(1);

// 带边框
Table::new(rows, widths)
    .block(Block::bordered().title("表格标题"));

// 单元格样式
let row = Row::new([
    Cell::new("数据 1").style(Style::new().fg(Color::Red)),
    Cell::new("数据 2").style(Style::new().fg(Color::Green)),
]);

// 行高度
let row = Row::new(["多行", "内容"]).height(3);
let row = Row::new(["带边距"]).top_margin(1).bottom_margin(1);
```

### Gauge — 进度条

```rust
use ratatui::{
    widgets::{Gauge, Block},
    style::{Style, Color, Stylize},
};

// 基本用法
frame.render_widget(
    Gauge::default(),
    frame.area(),
);

// 百分比进度
frame.render_widget(
    Gauge::default()
        .gauge_style(Style::new().fg(Color::Yellow).bg(Color::Black))
        .percent(75),
    frame.area(),
);

// 带标签
frame.render_widget(
    Gauge::default()
        .percent(50)
        .label("50% 完成"),
    frame.area(),
);

// 带标题边框
frame.render_widget(
    Gauge::default()
        .block(Block::bordered().title("下载进度"))
        .gauge_style(Style::new().white().on_black())
        .percent(20)
        .label("20%"),
    frame.area(),
);

// 范围进度
frame.render_widget(
    Gauge::default()
        .ratio(0.75)
        .label("自定义"),
    frame.area(),
);

// 使用 Unicode
frame.render_widget(
    Gauge::default()
        .percent(60)
        .use_unicode(true),
    frame.area(),
);

// 自定义进度条样式
Gauge::default()
    .gauge_style(Style::new().fg(Color::Cyan).bg(Color::Blue))
    .percent(40);
```

### BarChart — 柱状图

```rust
use ratatui::{
    widgets::{BarChart, Bar, BarGroup},
    style::{Style, Color, Stylize},
    layout::Direction,
};

// 基本用法
let bars = vec![
    Bar::new("A", 3),
    Bar::new("B", 5),
    Bar::new("C", 2),
    Bar::new("D", 7),
];

let bar_chart = BarChart::default()
    .data(bars)
    .bar_width(3)
    .bar_gap(1)
    .max(10);

frame.render_widget(bar_chart, frame.area());

// 带样式
let bar_chart = BarChart::default()
    .data(vec![
        Bar::new(3).style(Style::new().fg(Color::Yellow).bg(Color::Black)),
        Bar::new(5).label("B").value_style(Style::new().fg(Color::Red)),
        Bar::new(2),
    ])
    .bar_style(Style::new().fg(Color::Blue))
    .value_style(Style::new().fg(Color::Yellow))
    .label_style(Style::new().fg(Color::Green))
    .max(10);

// 水平柱状图
let bar_chart = BarChart::horizontal(vec![
    Bar::new("A", 10),
    Bar::new("B", 20),
]);

// 多组柱状图
let bar_chart = BarChart::default()
    .data(vec![
        Bar::default().value(2),
        Bar::default().value(3),
        Bar::default().value(4),
    ])
    .data(vec![
        Bar::default().value(3),
        Bar::default().value(4),
        Bar::default().value(5),
    ])
    .group_gap(1)
    .direction(Direction::Horizontal);

// 柱状图分组
let bargroup = BarGroup::default()
    .label("第一组")
    .bars(&[
        Bar::default().value(2),
        Bar::default().value(3),
        Bar::default().value(4),
    ]);

let bar_chart = BarChart::default()
    .data(bargroup)
    .group_gap(1);
```

### Chart — 图表

```rust
use ratatui::{
    widgets::{
        Chart, Axis, Dataset, GraphType,
        LegendPosition,
    },
    style::{Style, Color, Stylize},
    layout::Constraint,
};

// 基本用法
let datasets = vec![
    Dataset::default()
        .name("数据集 1")
        .marker(ratatui::symbols::Marker::Dot)
        .style(Style::new().fg(Color::Cyan))
        .graph_type(GraphType::Line)
        .data(&[(0.0, 1.0), (1.0, 2.0), (2.0, 3.0)]),

    Dataset::default()
        .name("数据集 2")
        .marker(ratatui::symbols::Marker::Braille)
        .style(Style::new().fg(Color::Magenta))
        .graph_type(GraphType::Area)
        .data(&[(0.0, 2.0), (1.0, 3.0), (2.0, 5.0)]),
];

let chart = Chart::new(datasets)
    .x_axis(Axis::default()
        .title("X 轴")
        .style(Style::new().fg(Color::White)))
    .y_axis(Axis::default()
        .title("Y 轴")
        .style(Style::new().fg(Color::White)));

frame.render_widget(chart, frame.area());

// 带图例
let chart = Chart::new(datasets)
    .x_axis(Axis::default())
    .y_axis(Axis::default())
    .legend_position(LegendPosition::TopLeft)
    .x_bounds([0.0, 5.0])
    .y_bounds([0.0, 5.0])
    .block(Block::bordered().title("数据图表"));

// 散点图
Dataset::default()
    .graph_type(GraphType::Scatter)
    .data(&[(1.0, 2.0), (3.0, 4.0)]);

// 柱状图（图表模式）
Dataset::default()
    .graph_type(GraphType::Bar)
    .data(&[(1.0, 5.0), (2.0, 8.0)]);

// 带约束
Chart::new(datasets)
    .x_constraint(Constraint::Percentage(90))
    .y_constraint(Constraint::Percentage(80));
```

### Sparkline — 迷你折线图

```rust
use ratatui::widgets::{Sparkline, RenderDirection};

// 基本用法
let data = vec![10, 15, 8, 12, 20, 18, 14];

frame.render_widget(
    Sparkline::default()
        .data(&data)
        .max(25)
        .style(Style::new().fg(Color::Cyan)),
    frame.area(),
);

// 水平方向
Sparkline::default()
    .data(&data)
    .direction(RenderDirection::RightToLeft);

// 带边框
Sparkline::default()
    .block(Block::bordered().title("趋势"))
    .data(&data);

// 自定义柱状符号
Sparkline::default()
    .bar_set(ratatui::symbols::bar::THREE_LEVELS)
    .data(&data);
```

---

## 导航组件

### Tabs — 标签页

```rust
use ratatui::{
    widgets::{Tabs, Block},
    style::{Style, Color, Stylize},
    text::Line,
};

// 基本用法
let titles = vec!["标签 1".into(), "标签 2".into(), "标签 3".into()];
let tabs = Tabs::new(titles);
frame.render_widget(tabs, frame.area());

// 带样式
let tabs = Tabs::new(titles)
    .block(Block::bordered())
    .style(Style::new().fg(Color::White))
    .highlight_style(Style::new().fg(Color::Yellow).bg(Color::Black))
    .divider(ratatui::symbols::line::VERTICAL);

// 选中项
let tabs = Tabs::new(titles).select(1);  // 选中第二个

// 带分隔符
let tabs = Tabs::new(titles)
    .divider("|")
    .padding_left(Line::from(" "))
    .padding_right(Line::from(" "));

// 自定义标题样式
let titles = vec![
    Line::from(vec!["首页".green()]),
    Line::from(vec!["设置".blue()]),
    Line::from(vec!["关于".red()]),
];
let tabs = Tabs::new(titles);
```

### Scrollbar — 滚动条

```rust
use ratatui::{
    widgets::{Scrollbar, ScrollbarOrientation, ScrollbarState},
};

// 水平滚动条
let mut horizontal_scroll_state = ScrollbarState::new(100)
    .position(10);

frame.render_stateful_widget(
    Scrollbar::new(ScrollbarOrientation::HorizontalBottom)
        .thumb_symbol("█"),
    frame.area(),
    &mut horizontal_scroll_state,
);

// 垂直滚动条
let mut vertical_scroll_state = ScrollbarState::new(50)
    .position(25);

frame.render_stateful_widget(
    Scrollbar::new(ScrollbarOrientation::VerticalRight)
        .thumb_symbol("█")
        .begin_symbol(Some("↑"))
        .end_symbol(Some("↓")),
    frame.area(),
    &mut vertical_scroll_state,
);

// 自定义样式
Scrollbar::new(ScrollbarOrientation::HorizontalBottom)
    .thumb_style(Style::new().fg(Color::Cyan))
    .track_symbol(Some("─"))
    .begin_symbol(Some("◀"))
    .end_symbol(Some("▶"));

// 滚动条状态管理
ScrollbarState::new(total_length)
    .position(current_position)
    .viewport_content_length(viewport_size);  // 可选
```

---

## 其他组件

### Calendar — 日历

```rust
use ratatui::widgets::{
    Calendar, DateStyler, CalendarEventStore,
};
use time::Date;

// 基本用法
let store = CalendarEventStore::default();
let calendar = Calendar::new(&store);
frame.render_widget(calendar, frame.area());

// 高亮今天
#[cfg(feature = "std")]
let store = CalendarEventStore::today(Style::new().fg(Color::Yellow));

// 自定义日期样式
let mut store = CalendarEventStore::default();
store.add(
    Date::from_calendar_date(2026, time::Month::June, 15).unwrap(),
    Style::new().fg(Color::Red).add_modifier(Modifier::BOLD),
);

let calendar = Calendar::new(&store)
    .style(Style::new().fg(Color::White));
```

### Canvas — 画布

```rust
use ratatui::widgets::{Canvas, Block};
use ratatui::layout::Rect;

Canvas::default()
    .block(Block::bordered().title("地图"))
    .x_bounds(0.0, 20.0)
    .y_bounds(0.0, 20.0)
    .paint(|ctx| {
        // 绘制圆
        ctx.draw(&Circle::new(10.0, 10.0, 3.0));

        // 绘制矩形
        ctx.draw(&Rectangle::new(2.0, 2.0, 6.0, 6.0));

        // 绘制线段
        ctx.draw(&Line::new(0.0, 0.0, 10.0, 10.0));

        // 绘制散点
        ctx.draw(&points![1.0, 1.0; 2.0, 2.0; 3.0, 3.0]);

        // 设置标记
        ctx.set_marker(ratatui::symbols::Marker::Braille);

        // 设置背景色
        ctx.set_style(Style::new().bg(Color::Black));

        // 在坐标上绘制文本
        ctx.print(10.0, 10.0, Style::new(), "#");
    })
    .marker(ratatui::symbols::Marker::Dot);
```

### Clear — 清除区域

```rust
use ratatui::widgets::{Clear, Block};

// 在绘制弹出窗口前清除区域
fn render_popup(f: &mut Frame, area: Rect) {
    f.render_widget(Clear, area);           // 清除
    f.render_widget(
        Block::bordered().title("弹出窗口"),
        area,
    );
}
```

### Fill — 填充

```rust
use ratatui::widgets::Fill;

// 用内容填充整个区域
frame.render_widget(
    Fill::horizontal("─"),  // 水平填充
    frame.area(),
);

frame.render_widget(
    Fill::vertical("│"),    // 垂直填充
    frame.area(),
);

frame.render_widget(
    Fill::with_char('░'),   // 自定义字符填充
    frame.area(),
);
```

### Spinner — 加载指示器

```rust
use ratatui::widgets::Spinners;
use std::time::Duration;

// 在渲染循环中更新
fn render(frame: &mut Frame, spin_time: &mut Duration) {
    *spin_time += Duration::from_millis(50);
    let spinner = Spinners::Dots.frame(*spin_time as u8);

    frame.render_widget(
        Paragraph::new(format!("{} 加载中...", spinner))
            .style(Style::new().fg(Color::Yellow)),
        frame.area(),
    );
}
```

### Logo — 标志

```rust
use ratatui::widgets::Logo;

// 在区域中绘制 Ratatui Logo
frame.render_widget(Logo::default(), frame.area());
```

### Mascot — 吉祥物

```rust
use ratatui::widgets::{Mascot, MascotEyeColor};

// 默认吉祥物
frame.render_widget(Mascot::new(), frame.area());

// 眨眼效果
frame.render_widget(
    Mascot::new().set_eye(MascotEyeColor::Red),
    frame.area(),
);

// 自定义颜色
Mascot::new()
    .rat_color(Color::Indexed(252))
    .hat_color(Color::Indexed(231));
```

---

## 样式系统

### Style — 样式构建

```rust
use ratatui::style::{Style, Color, Modifier, Stylize};

// 链式方法
let style = Style::new()
    .fg(Color::Red)              // 前景色
    .bg(Color::Blue)             // 背景色
    .add_modifier(Modifier::BOLD | Modifier::ITALIC);

// 快捷链式方法（Stylize trait）
let text = "Hello".red().on_blue().bold().italic();

// 预定义颜色
Color::Black
Color::Red, Color::Green, Color::Yellow
Color::Blue, Color::Magenta, Color::Cyan
Color::Gray, Color::DarkGray
Color::LightRed, Color::LightGreen, Color::LightYellow
Color::LightBlue, Color::LightMagenta, Color::LightCyan
Color::White

// 自定义颜色
Color::Rgb(r, g, b)              // 256 色
Color::Indexed(n)                 // 256 索引色
Color::AnsiValue(n)              // ANSI 值

// 修饰符
Modifier::BOLD
Modifier::DIM
Modifier::ITALIC
Modifier::UNDERLINED
Modifier::REVERSED
Modifier::HIDDEN
Modifier::CROSSED_OUT

// 样式补丁
let base = Style::new().fg(Color::Red);
let patched = base.patch(Style::new().bg(Color::Blue));

// 转换
let style: Style = Color::Red.into();
let style: Style = Modifier::BOLD.into();
```

### Color — 颜色系统

```rust
use ratatui::style::Color;

// 基本色
Color::Red
Color::Blue

// RGB 色
Color::Rgb(255, 128, 0)

// 索引色 (256 色调色板)
Color::Indexed(196)

// HSL 转换
#[cfg(feature = "palette")]
Color::from_hsl(palette::Hsl::new(h, s, l))

#[cfg(feature = "palette")]
Color::from_hsluv(palette::Hsluv::new(h, s, l))
```

---

## 文本系统

### Line — 行

```rust
use ratatui::text::{Line, Span};
use ratatui::style::Style;
use ratatui::layout::Alignment;

// 从字符串创建
let line = Line::from("Hello World");

// 从 Span 列表创建
let line = Line::from(vec![
    Span::styled("Hello", Style::new().fg(Color::Red)),
    Span::styled(" World", Style::new().fg(Color::Blue)),
]);

// 对齐
let line = Line::from("居中").alignment(Alignment::Center);

// 拼接
let line1 = Line::from("Hello");
let line2 = Line::from("World");
let combined = line1 + " " + line2;

// 宽度
let width = line.width();
```

### Span — 文本片段

```rust
use ratatui::text::Span;
use ratatui::style::Style;

// 基本
let span = Span::raw("内容");
let span = Span::styled("内容", Style::new().fg(Color::Red));

// Stylize 链式
let span = Span::styled("内容", Style::new()).red().bold();

// 拼接
let span1 = Span::raw("Hello");
let span2 = Span::raw("World");
let combined = span1 + span2;
```

### Text — 文本块

```rust
use ratatui::text::Text;

// 从字符串
let text = Text::from("Hello\nWorld");

// 从行列表
let text = Text::from(vec![
    Line::from("第一行"),
    Line::from("第二行"),
]);

// 拼接
let text1 = Text::from("Hello");
let text2 = Text::from("World");
let combined = text1 + text2;
```

---

## 完整示例：Todo 列表

```rust
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use color_eyre::Result;
use ratatui::{
    backend::CrosstermBackend,
    Terminal,
    layout::{Constraint, Layout},
    style::{Style, Color, Stylize},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
};
use std::io;

struct App {
    items: Vec<String>,
    state: ListState,
    input: String,
}

impl App {
    fn new() -> Self {
        Self {
            items: vec![
                "任务 1".to_string(),
                "任务 2".to_string(),
                "任务 3".to_string(),
            ],
            state: ListState::default(),
            input: String::new(),
        }
    }

    fn next(&mut self) {
        let i = match self.state.selected() {
            Some(i) => {
                if i >= self.items.len().saturating_sub(1) { 0 }
                else { i + 1 }
            }
            None => 0,
        };
        self.state.select(Some(i));
    }

    fn previous(&mut self) {
        let i = match self.state.selected() {
            Some(i) => {
                if i == 0 { self.items.len().saturating_sub(1) }
                else { i - 1 }
            }
            None => 0,
        };
        self.state.select(Some(i));
    }

    fn add_item(&mut self, text: &str) {
        self.items.push(text.to_string());
    }

    fn remove_item(&mut self) {
        if let Some(i) = self.state.selected() {
            if i < self.items.len() {
                self.items.remove(i);
            }
        }
    }
}

fn main() -> Result<()> {
    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new();
    loop {
        terminal.draw(|f| {
            let chunks = Layout::vertical([
                Constraint::Length(3),
                Constraint::Min(0),
            ]).split(f.area());

            // 标题
            let title = Block::bordered()
                .title(Line::from(Span::styled(
                    "Todo List",
                    Style::new().fg(Color::Yellow).add_modifier(ratatui::style::Modifier::BOLD),
                )))
                .title_bottom(Line::from(format!(
                    "项目数: {}",
                    app.items.len()
                )));
            f.render_widget(title, chunks[0]);

            // 列表
            let list_items: Vec<ListItem> = app.items.iter()
                .map(|i| ListItem::new(i.as_str()))
                .collect();

            let list = List::new(list_items)
                .block(Block::bordered().title("任务列表"))
                .highlight_style(Style::new().bg(Color::Blue).fg(Color::White))
                .highlight_symbol(">>");

            f.render_stateful_widget(list, chunks[1], &mut app.state);
        })?;

        if let Event::Key(key) = event::read()? {
            if key.kind != KeyEventKind::Press {
                continue;
            }
            match key.code {
                KeyCode::Char('q') => break,
                KeyCode::Down => app.next(),
                KeyCode::Up => app.previous(),
                KeyCode::Char('a') => app.add_item("新项目"),
                KeyCode::Char('d') => app.remove_item(),
                _ => {}
            }
        }
    }

    Ok(())
}
```

---

## 组件速查表

| 组件 | 类型 | 说明 | 关键状态 |
|------|------|------|----------|
| `Paragraph` | Widget | 文本段落 | 无 |
| `Block` | Widget | 边框容器 | 无 |
| `List` | StatefulWidget | 可选中列表 | `ListState` |
| `Table` | StatefulWidget | 数据表格 | `TableState` |
| `Gauge` | Widget | 进度条 | 无 |
| `BarChart` | Widget | 柱状图 | 无 |
| `Chart` | Widget | 通用图表 | 无 |
| `Sparkline` | Widget | 迷你折线图 | 无 |
| `Tabs` | Widget | 标签页 | 无 |
| `Scrollbar` | StatefulWidget | 滚动条 | `ScrollbarState` |
| `Calendar` | Widget | 日历 | 无 |
| `Canvas` | Widget | 画布 | 无 |
| `Clear` | Widget | 清除区域 | 无 |
| `Fill` | Widget | 填充 | 无 |
| `Spinner` | 枚举 | 加载动画 | 无 |
| `Logo` | Widget | 标志 | 无 |
| `Mascot` | Widget | 吉祥物 | 无 |
