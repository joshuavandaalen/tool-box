# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-03-22

### Added

- `text-sender`: Rust TUI console application using `ratatui`, `crossterm`, and `tokio`
  - Multi-line body editor with full cursor navigation
  - URL input bar with inline cursor display
  - HTTP method selector (GET / POST / PUT / PATCH / DELETE) cycled with Alt-←/→
  - Async HTTP requests dispatched in a background Tokio task (non-blocking UI)
  - JSON response pretty-printing via `serde_json`
  - Scrollable response pane
  - Color-coded status bar (idle / sending / success / error)
  - 15 unit tests covering URL editing, body editing, method cycling, and scroll
