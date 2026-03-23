use std::time::Duration;

use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};
use reqwest::Client;
use serde_json::Value;

// ─────────────────────────────────────────────
//  Domain types
// ─────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum HttpMethod {
    Get,
    Post,
    Put,
    Patch,
    Delete,
}

impl HttpMethod {
    fn as_str(self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
            Self::Put => "PUT",
            Self::Patch => "PATCH",
            Self::Delete => "DELETE",
        }
    }

    fn all() -> &'static [HttpMethod] {
        &[Self::Get, Self::Post, Self::Put, Self::Patch, Self::Delete]
    }

    fn next(self) -> Self {
        let all = Self::all();
        let idx = all.iter().position(|m| *m == self).unwrap_or(0);
        all[(idx + 1) % all.len()]
    }

    fn prev(self) -> Self {
        let all = Self::all();
        let idx = all.iter().position(|m| *m == self).unwrap_or(0);
        all[(idx + all.len() - 1) % all.len()]
    }

    fn color(self) -> Color {
        match self {
            Self::Get => Color::Green,
            Self::Post => Color::Blue,
            Self::Put => Color::Yellow,
            Self::Patch => Color::Cyan,
            Self::Delete => Color::Red,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Focus {
    Url,
    Body,
    Response,
}

#[derive(Clone)]
enum Status {
    Idle,
    Sending,
    Success { code: u16, body: String },
    Error(String),
}

impl Status {
    fn label(&self) -> String {
        match self {
            Self::Idle => "Idle — press Ctrl-S to send, Tab to switch focus".to_owned(),
            Self::Sending => "Sending…".to_owned(),
            Self::Success { code, .. } => format!("✓ {code} OK"),
            Self::Error(e) => format!("✗ {e}"),
        }
    }

    fn color(&self) -> Color {
        match self {
            Self::Idle => Color::DarkGray,
            Self::Sending => Color::Yellow,
            Self::Success { .. } => Color::Green,
            Self::Error(_) => Color::Red,
        }
    }
}

// ─────────────────────────────────────────────
//  App state
// ─────────────────────────────────────────────

struct App {
    // URL bar
    url: String,
    url_cursor: usize,

    // HTTP method
    method: HttpMethod,

    // Body editor — each element is a line
    body_lines: Vec<String>,
    body_row: usize,
    body_col: usize,

    // Response pane
    response_scroll: u16,

    // Which pane has keyboard focus
    focus: Focus,

    // Last request result
    status: Status,

    // Whether the app should exit
    should_quit: bool,

    // Reusable HTTP client (keeps connection pool alive across requests)
    client: Client,
}

impl App {
    fn new() -> Self {
        Self {
            url: String::new(),
            url_cursor: 0,
            method: HttpMethod::Post,
            body_lines: vec![String::new()],
            body_row: 0,
            body_col: 0,
            response_scroll: 0,
            focus: Focus::Url,
            status: Status::Idle,
            should_quit: false,
            client: Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .expect("Failed to build HTTP client"),
        }
    }

    // ── URL input helpers ──────────────────────────────────────────────

    fn url_insert(&mut self, ch: char) {
        self.url.insert(self.url_cursor, ch);
        self.url_cursor += ch.len_utf8();
    }

    fn url_backspace(&mut self) {
        if self.url_cursor == 0 {
            return;
        }
        let before = &self.url[..self.url_cursor];
        let prev = before.char_indices().next_back().map(|(i, _)| i).unwrap_or(0);
        self.url.drain(prev..self.url_cursor);
        self.url_cursor = prev;
    }

    fn url_delete(&mut self) {
        if self.url_cursor >= self.url.len() {
            return;
        }
        let next = self.url[self.url_cursor..]
            .char_indices()
            .nth(1)
            .map(|(i, _)| self.url_cursor + i)
            .unwrap_or(self.url.len());
        self.url.drain(self.url_cursor..next);
    }

    fn url_move_left(&mut self) {
        if self.url_cursor == 0 {
            return;
        }
        let before = &self.url[..self.url_cursor];
        self.url_cursor = before.char_indices().next_back().map(|(i, _)| i).unwrap_or(0);
    }

    fn url_move_right(&mut self) {
        if self.url_cursor >= self.url.len() {
            return;
        }
        self.url_cursor += self.url[self.url_cursor..]
            .chars()
            .next()
            .map(|c| c.len_utf8())
            .unwrap_or(0);
    }

    // ── Body editor helpers ────────────────────────────────────────────

    fn body_insert(&mut self, ch: char) {
        let line = &mut self.body_lines[self.body_row];
        let col = self.body_col.min(line.len());
        line.insert(col, ch);
        self.body_col = col + ch.len_utf8();
    }

    fn body_newline(&mut self) {
        let line = &mut self.body_lines[self.body_row];
        let col = self.body_col.min(line.len());
        let remainder = line[col..].to_owned();
        line.truncate(col);
        self.body_row += 1;
        self.body_lines.insert(self.body_row, remainder);
        self.body_col = 0;
    }

    fn body_backspace(&mut self) {
        if self.body_col == 0 {
            if self.body_row == 0 {
                return;
            }
            let current = self.body_lines.remove(self.body_row);
            self.body_row -= 1;
            let prev_len = self.body_lines[self.body_row].len();
            self.body_lines[self.body_row].push_str(&current);
            self.body_col = prev_len;
        } else {
            let line = &mut self.body_lines[self.body_row];
            let col = self.body_col.min(line.len());
            let before = &line[..col];
            let prev = before.char_indices().next_back().map(|(i, _)| i).unwrap_or(0);
            line.drain(prev..col);
            self.body_col = prev;
        }
    }

    fn body_delete(&mut self) {
        let line = &self.body_lines[self.body_row];
        let col = self.body_col.min(line.len());
        if col >= line.len() {
            if self.body_row + 1 < self.body_lines.len() {
                let next = self.body_lines.remove(self.body_row + 1);
                self.body_lines[self.body_row].push_str(&next);
            }
        } else {
            let line = &mut self.body_lines[self.body_row];
            let next = line[col..]
                .char_indices()
                .nth(1)
                .map(|(i, _)| col + i)
                .unwrap_or(line.len());
            line.drain(col..next);
        }
    }

    fn body_up(&mut self) {
        if self.body_row > 0 {
            self.body_row -= 1;
            self.body_col = self.body_col.min(self.body_lines[self.body_row].len());
        }
    }

    fn body_down(&mut self) {
        if self.body_row + 1 < self.body_lines.len() {
            self.body_row += 1;
            self.body_col = self.body_col.min(self.body_lines[self.body_row].len());
        }
    }

    fn body_move_left(&mut self) {
        if self.body_col == 0 {
            if self.body_row > 0 {
                self.body_row -= 1;
                self.body_col = self.body_lines[self.body_row].len();
            }
        } else {
            let line = &self.body_lines[self.body_row];
            let col = self.body_col.min(line.len());
            let before = &line[..col];
            self.body_col = before.char_indices().next_back().map(|(i, _)| i).unwrap_or(0);
        }
    }

    fn body_move_right(&mut self) {
        let line = &self.body_lines[self.body_row];
        let col = self.body_col.min(line.len());
        if col >= line.len() {
            if self.body_row + 1 < self.body_lines.len() {
                self.body_row += 1;
                self.body_col = 0;
            }
        } else {
            self.body_col = col
                + line[col..]
                    .chars()
                    .next()
                    .map(|c| c.len_utf8())
                    .unwrap_or(0);
        }
    }

    fn body_home(&mut self) {
        self.body_col = 0;
    }

    fn body_end(&mut self) {
        self.body_col = self.body_lines[self.body_row].len();
    }

    // ── Body as a single string ────────────────────────────────────────

    fn body_text(&self) -> String {
        self.body_lines.join("\n")
    }

    // ── Response scroll ────────────────────────────────────────────────

    fn scroll_response_up(&mut self) {
        self.response_scroll = self.response_scroll.saturating_sub(1);
    }

    fn scroll_response_down(&mut self) {
        self.response_scroll = self.response_scroll.saturating_add(1);
    }
}

// ─────────────────────────────────────────────
//  Event handling
// ─────────────────────────────────────────────

async fn handle_key(
    app: &mut App,
    key: crossterm::event::KeyEvent,
    tx: &tokio::sync::mpsc::Sender<Status>,
) {
    // Global shortcuts
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        match key.code {
            KeyCode::Char('c') | KeyCode::Char('q') => {
                app.should_quit = true;
                return;
            }
            KeyCode::Char('s') => {
                do_send(app, tx).await;
                return;
            }
            _ => {}
        }
    }

    match key.code {
        KeyCode::Tab => {
            app.focus = match app.focus {
                Focus::Url => Focus::Body,
                Focus::Body => Focus::Response,
                Focus::Response => Focus::Url,
            };
            return;
        }
        KeyCode::BackTab => {
            app.focus = match app.focus {
                Focus::Url => Focus::Response,
                Focus::Body => Focus::Url,
                Focus::Response => Focus::Body,
            };
            return;
        }
        _ => {}
    }

    match app.focus {
        Focus::Url => match key.code {
            KeyCode::Char(c) => app.url_insert(c),
            KeyCode::Backspace => app.url_backspace(),
            KeyCode::Delete => app.url_delete(),
            KeyCode::Left => {
                if key.modifiers.contains(KeyModifiers::ALT) {
                    app.method = app.method.prev();
                } else {
                    app.url_move_left();
                }
            }
            KeyCode::Right => {
                if key.modifiers.contains(KeyModifiers::ALT) {
                    app.method = app.method.next();
                } else {
                    app.url_move_right();
                }
            }
            KeyCode::Home => app.url_cursor = 0,
            KeyCode::End => app.url_cursor = app.url.len(),
            KeyCode::Enter => {
                do_send(app, tx).await;
            }
            _ => {}
        },
        Focus::Body => match key.code {
            KeyCode::Char(c) => app.body_insert(c),
            KeyCode::Enter => app.body_newline(),
            KeyCode::Backspace => app.body_backspace(),
            KeyCode::Delete => app.body_delete(),
            KeyCode::Up => app.body_up(),
            KeyCode::Down => app.body_down(),
            KeyCode::Left => app.body_move_left(),
            KeyCode::Right => app.body_move_right(),
            KeyCode::Home => app.body_home(),
            KeyCode::End => app.body_end(),
            _ => {}
        },
        Focus::Response => match key.code {
            KeyCode::Up | KeyCode::Char('k') => app.scroll_response_up(),
            KeyCode::Down | KeyCode::Char('j') => app.scroll_response_down(),
            KeyCode::PageUp => {
                for _ in 0..10 {
                    app.scroll_response_up();
                }
            }
            KeyCode::PageDown => {
                for _ in 0..10 {
                    app.scroll_response_down();
                }
            }
            _ => {}
        },
    }
}

// ─────────────────────────────────────────────
//  HTTP send
// ─────────────────────────────────────────────

async fn do_send(app: &mut App, tx: &tokio::sync::mpsc::Sender<Status>) {
    let url = app.url.trim().to_owned();
    if url.is_empty() {
        app.status = Status::Error("URL is empty — enter a URL first".to_owned());
        return;
    }
    let body = app.body_text();
    let method = app.method;
    app.status = Status::Sending;

    let result = send_request(&app.client, url, method, body).await;
    let _ = tx.send(result).await;
}

async fn send_request(client: &Client, url: String, method: HttpMethod, body: String) -> Status {
    let req = match method {
        HttpMethod::Get => client.get(&url),
        HttpMethod::Post => client.post(&url),
        HttpMethod::Put => client.put(&url),
        HttpMethod::Patch => client.patch(&url),
        HttpMethod::Delete => client.delete(&url),
    };

    // Attach body only for methods that carry a payload
    let req = match method {
        HttpMethod::Post | HttpMethod::Put | HttpMethod::Patch => {
            req.header("Content-Type", "text/plain").body(body)
        }
        _ => req,
    };

    match req.send().await {
        Ok(resp) => {
            let status = resp.status();
            let code = status.as_u16();
            let raw = resp.text().await.unwrap_or_default();
            // Pretty-print JSON responses when possible
            let pretty = serde_json::from_str::<Value>(&raw)
                .map(|v| serde_json::to_string_pretty(&v).unwrap_or(raw.clone()))
                .unwrap_or(raw);
            if status.is_success() {
                Status::Success { code, body: pretty }
            } else {
                let reason = status.canonical_reason().unwrap_or("Unknown Status");
                Status::Error(format!(
                    "HTTP {} {}{}{}",
                    code,
                    reason,
                    if pretty.is_empty() { "" } else { ":\n\n" },
                    pretty,
                ))
            }
        }
        Err(e) => Status::Error(e.to_string()),
    }
}

// ─────────────────────────────────────────────
//  Drawing
// ─────────────────────────────────────────────

fn draw(frame: &mut ratatui::Frame, app: &App) {
    let size = frame.area();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // URL + method bar
            Constraint::Min(8),     // body editor
            Constraint::Length(10), // response pane
            Constraint::Length(1),  // status bar
        ])
        .split(size);

    draw_url_bar(frame, app, chunks[0]);
    draw_body(frame, app, chunks[1]);
    draw_response(frame, app, chunks[2]);
    draw_status_bar(frame, app, chunks[3]);
}

fn focus_style(focused: bool) -> Style {
    if focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    }
}

fn draw_url_bar(frame: &mut ratatui::Frame, app: &App, area: Rect) {
    let focused = app.focus == Focus::Url;

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(9), Constraint::Min(10)])
        .split(area);

    // Method badge
    let method_block = Block::default()
        .borders(Borders::ALL)
        .border_style(focus_style(focused))
        .title(" Method ");
    let method_text = Paragraph::new(app.method.as_str())
        .block(method_block)
        .style(
            Style::default()
                .fg(app.method.color())
                .add_modifier(Modifier::BOLD),
        );
    frame.render_widget(method_text, cols[0]);

    // URL input
    let url_block = Block::default()
        .borders(Borders::ALL)
        .border_style(focus_style(focused))
        .title(" URL (Alt-←/→ to change method, Enter or Ctrl-S to send) ");

    let url_widget = if focused {
        let cursor = app.url_cursor.min(app.url.len());
        let before = &app.url[..cursor];
        let after = &app.url[cursor..];
        let cursor_ch_len = after.chars().next().map(|c| c.len_utf8()).unwrap_or(0);
        let cursor_str = if cursor_ch_len == 0 {
            " ".to_owned()
        } else {
            after[..cursor_ch_len].to_owned()
        };
        let rest = if cursor_ch_len == 0 {
            ""
        } else {
            &after[cursor_ch_len..]
        };
        let line = Line::from(vec![
            Span::raw(before.to_owned()),
            Span::styled(cursor_str, Style::default().add_modifier(Modifier::REVERSED)),
            Span::raw(rest.to_owned()),
        ]);
        Paragraph::new(line).block(url_block)
    } else {
        Paragraph::new(app.url.clone()).block(url_block)
    };

    frame.render_widget(url_widget, cols[1]);
}

fn draw_body(frame: &mut ratatui::Frame, app: &App, area: Rect) {
    let focused = app.focus == Focus::Body;
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(focus_style(focused))
        .title(" Body (Ctrl-S to send) ");

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let visible_height = inner.height as usize;
    let start_row = if app.body_row >= visible_height {
        app.body_row + 1 - visible_height
    } else {
        0
    };

    let lines: Vec<Line> = app
        .body_lines
        .iter()
        .enumerate()
        .skip(start_row)
        .take(visible_height)
        .map(|(row_idx, line)| {
            if focused && row_idx == app.body_row {
                let col = app.body_col.min(line.len());
                let before = &line[..col];
                let after = &line[col..];
                let cursor_ch_len = after.chars().next().map(|c| c.len_utf8()).unwrap_or(0);
                let cursor_str = if cursor_ch_len == 0 {
                    " ".to_owned()
                } else {
                    after[..cursor_ch_len].to_owned()
                };
                let rest = if cursor_ch_len == 0 {
                    ""
                } else {
                    &after[cursor_ch_len..]
                };
                Line::from(vec![
                    Span::raw(before.to_owned()),
                    Span::styled(cursor_str, Style::default().add_modifier(Modifier::REVERSED)),
                    Span::raw(rest.to_owned()),
                ])
            } else {
                Line::from(line.clone())
            }
        })
        .collect();

    frame.render_widget(Paragraph::new(lines), inner);
}

fn draw_response(frame: &mut ratatui::Frame, app: &App, area: Rect) {
    let focused = app.focus == Focus::Response;
    let title = match &app.status {
        Status::Success { code, .. } => {
            format!(" Response — {code} (↑/↓ or j/k to scroll) ")
        }
        _ => " Response ".to_owned(),
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(focus_style(focused))
        .title(title);

    let (content, style) = match &app.status {
        Status::Idle => (
            "No request sent yet.".to_owned(),
            Style::default(),
        ),
        Status::Sending => (
            "Waiting for response…".to_owned(),
            Style::default().fg(Color::Yellow),
        ),
        Status::Success { body, .. } => (body.clone(), Style::default().fg(Color::Green)),
        Status::Error(e) => (
            format!("Error: {e}"),
            Style::default().fg(Color::Red),
        ),
    };

    let widget = Paragraph::new(content)
        .block(block)
        .style(style)
        .wrap(Wrap { trim: false })
        .scroll((app.response_scroll, 0));
    frame.render_widget(widget, area);
}

fn draw_status_bar(frame: &mut ratatui::Frame, app: &App, area: Rect) {
    let right_text = " Tab: switch focus  Ctrl-S/Enter: send  Ctrl-C/Q: quit ";

    let bar = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Min(10),
            Constraint::Length(right_text.len() as u16),
        ])
        .split(area);

    frame.render_widget(
        Paragraph::new(app.status.label()).style(
            Style::default()
                .fg(app.status.color())
                .add_modifier(Modifier::BOLD),
        ),
        bar[0],
    );

    frame.render_widget(
        Paragraph::new(right_text).style(Style::default().fg(Color::DarkGray)),
        bar[1],
    );
}

// ─────────────────────────────────────────────
//  Entry point
// ─────────────────────────────────────────────

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Ensure terminal is restored even if the app panics
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        let _ = disable_raw_mode();
        let _ = execute!(std::io::stdout(), LeaveAlternateScreen, DisableMouseCapture);
        original_hook(panic_info);
    }));

    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new();
    let (tx, mut rx) = tokio::sync::mpsc::channel::<Status>(8);

    let result = run(&mut terminal, &mut app, &tx, &mut rx).await;

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
    terminal.show_cursor()?;

    if let Err(e) = result {
        eprintln!("Error: {e}");
    }
    Ok(())
}

async fn run(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    app: &mut App,
    tx: &tokio::sync::mpsc::Sender<Status>,
    rx: &mut tokio::sync::mpsc::Receiver<Status>,
) -> anyhow::Result<()> {
    loop {
        terminal.draw(|f| draw(f, app))?;

        // Flush any received async status updates
        while let Ok(status) = rx.try_recv() {
            app.status = status;
            app.response_scroll = 0;
        }

        // Poll for input with a short timeout so responses are shown quickly
        if event::poll(Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                handle_key(app, key, tx).await;
            }
        }

        if app.should_quit {
            break;
        }
    }
    Ok(())
}

// ─────────────────────────────────────────────
//  Unit tests
// ─────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_app() -> App {
        App::new()
    }

    // ── URL editing ────────────────────────────────────────────────────

    #[test]
    fn url_insert_and_cursor_advance() {
        let mut app = make_app();
        app.url_insert('h');
        app.url_insert('i');
        assert_eq!(app.url, "hi");
        assert_eq!(app.url_cursor, 2);
    }

    #[test]
    fn url_backspace_removes_previous_char() {
        let mut app = make_app();
        for c in "hello".chars() {
            app.url_insert(c);
        }
        app.url_backspace();
        assert_eq!(app.url, "hell");
        assert_eq!(app.url_cursor, 4);
    }

    #[test]
    fn url_backspace_at_start_is_noop() {
        let mut app = make_app();
        app.url_backspace(); // should not panic
        assert_eq!(app.url, "");
        assert_eq!(app.url_cursor, 0);
    }

    #[test]
    fn url_delete_removes_char_at_cursor() {
        let mut app = make_app();
        for c in "hello".chars() {
            app.url_insert(c);
        }
        // move cursor to start
        app.url_cursor = 0;
        app.url_delete();
        assert_eq!(app.url, "ello");
        assert_eq!(app.url_cursor, 0);
    }

    #[test]
    fn url_move_left_right() {
        let mut app = make_app();
        for c in "abc".chars() {
            app.url_insert(c);
        }
        assert_eq!(app.url_cursor, 3);
        app.url_move_left();
        assert_eq!(app.url_cursor, 2);
        app.url_move_right();
        assert_eq!(app.url_cursor, 3);
    }

    // ── Body editing ───────────────────────────────────────────────────

    #[test]
    fn body_insert_characters() {
        let mut app = make_app();
        app.body_insert('H');
        app.body_insert('i');
        assert_eq!(app.body_lines[0], "Hi");
        assert_eq!(app.body_col, 2);
    }

    #[test]
    fn body_newline_splits_line() {
        let mut app = make_app();
        for c in "Hello World".chars() {
            app.body_insert(c);
        }
        // move cursor to between 'Hello' and ' World'
        app.body_col = 5;
        app.body_newline();
        assert_eq!(app.body_lines[0], "Hello");
        assert_eq!(app.body_lines[1], " World");
        assert_eq!(app.body_row, 1);
        assert_eq!(app.body_col, 0);
    }

    #[test]
    fn body_backspace_merges_lines() {
        let mut app = make_app();
        for c in "Hello".chars() {
            app.body_insert(c);
        }
        app.body_newline();
        for c in "World".chars() {
            app.body_insert(c);
        }
        // cursor is at end of "World" on row 1
        // move to start of row 1 and backspace to merge
        app.body_col = 0;
        app.body_backspace();
        assert_eq!(app.body_lines.len(), 1);
        assert_eq!(app.body_lines[0], "HelloWorld");
        assert_eq!(app.body_row, 0);
        assert_eq!(app.body_col, 5);
    }

    #[test]
    fn body_text_joins_lines() {
        let mut app = make_app();
        app.body_lines = vec!["line one".to_owned(), "line two".to_owned()];
        assert_eq!(app.body_text(), "line one\nline two");
    }

    #[test]
    fn body_up_down_navigation() {
        let mut app = make_app();
        app.body_lines = vec!["aaa".to_owned(), "bbb".to_owned(), "ccc".to_owned()];
        app.body_row = 0;
        app.body_col = 2;

        app.body_down();
        assert_eq!(app.body_row, 1);
        app.body_down();
        assert_eq!(app.body_row, 2);
        app.body_down(); // at last row — should not panic
        assert_eq!(app.body_row, 2);

        app.body_up();
        assert_eq!(app.body_row, 1);
        app.body_up();
        assert_eq!(app.body_row, 0);
        app.body_up(); // at first row — should not panic
        assert_eq!(app.body_row, 0);
    }

    #[test]
    fn body_home_end() {
        let mut app = make_app();
        for c in "hello".chars() {
            app.body_insert(c);
        }
        app.body_home();
        assert_eq!(app.body_col, 0);
        app.body_end();
        assert_eq!(app.body_col, 5);
    }

    // ── HTTP method cycling ────────────────────────────────────────────

    #[test]
    fn method_next_wraps_around() {
        let mut m = HttpMethod::Delete;
        m = m.next();
        assert_eq!(m, HttpMethod::Get);
    }

    #[test]
    fn method_prev_wraps_around() {
        let mut m = HttpMethod::Get;
        m = m.prev();
        assert_eq!(m, HttpMethod::Delete);
    }

    // ── Response scroll ────────────────────────────────────────────────

    #[test]
    fn response_scroll_saturates_at_zero() {
        let mut app = make_app();
        app.scroll_response_up(); // already 0 — should not underflow
        assert_eq!(app.response_scroll, 0);
    }

    #[test]
    fn response_scroll_down_then_up() {
        let mut app = make_app();
        app.scroll_response_down();
        app.scroll_response_down();
        assert_eq!(app.response_scroll, 2);
        app.scroll_response_up();
        assert_eq!(app.response_scroll, 1);
    }
}
