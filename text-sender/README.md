# text-sender

A terminal user interface (TUI) application that lets you compose a block of text and send it to an HTTP API endpoint.

## Technology

- **[ratatui](https://ratatui.rs/)** — terminal UI framework
- **[crossterm](https://github.com/crossterm-rs/crossterm)** — cross-platform terminal manipulation
- **[tokio](https://tokio.rs/)** — async runtime
- **[reqwest](https://docs.rs/reqwest)** — HTTP client

## Building

```bash
# from the text-sender directory
cargo build --release

# or from the repository root
cargo build --release --manifest-path text-sender/Cargo.toml
```

The compiled binary will be at `text-sender/target/release/text-sender`.

## Running

```bash
cargo run --release
# or
./target/release/text-sender
```

## Layout

```
┌─ Method ─┐┌─ URL (Alt-←/→ to change method, Enter or Ctrl-S to send) ──────────────────┐
│  POST    ││  https://httpbin.org/post                                                    │
└──────────┘└──────────────────────────────────────────────────────────────────────────────┘
┌─ Body (Ctrl-S to send) ──────────────────────────────────────────────────────────────────┐
│  Hello, world!                                                                           │
│  This text will be sent as the request body.                                             │
│                                                                                          │
└──────────────────────────────────────────────────────────────────────────────────────────┘
┌─ Response — 200 (↑/↓ or j/k to scroll) ─────────────────────────────────────────────────┐
│  {                                                                                       │
│    "data": "Hello, world!\nThis text will be sent as the request body.",                 │
│    ...                                                                                   │
│  }                                                                                       │
└──────────────────────────────────────────────────────────────────────────────────────────┘
✓ 200 OK              Tab: switch focus  Ctrl-S/Enter: send  Ctrl-C/Q: quit
```

## Key Bindings

| Key | Action |
|-----|--------|
| **Tab** / **Shift-Tab** | Cycle focus between URL bar, Body editor, and Response pane |
| **Ctrl-S** | Send request |
| **Enter** (in URL bar) | Send request |
| **Alt-←** / **Alt-→** | Change HTTP method (GET → POST → PUT → PATCH → DELETE → …) |
| **↑ ↓ ← →** | Navigate cursor (in URL / Body) or scroll (in Response) |
| **Home** / **End** | Move to start / end of current line |
| **Backspace** / **Delete** | Delete characters |
| **j** / **k** | Scroll response pane down / up (when Response is focused) |
| **Page Up** / **Page Down** | Scroll response pane by 10 lines |
| **Ctrl-C** or **Ctrl-Q** | Quit |

## HTTP Methods

The HTTP method defaults to **POST**. Use **Alt-←** / **Alt-→** while the URL bar is focused to cycle through:

`GET` → `POST` → `PUT` → `PATCH` → `DELETE` → `GET` → …

Methods that carry a body (`POST`, `PUT`, `PATCH`) will attach the body text with `Content-Type: text/plain`. `GET` and `DELETE` send no body.

## Running Tests

```bash
cargo test
```
