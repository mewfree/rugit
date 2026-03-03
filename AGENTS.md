# Agent Guidelines for rugit

## Task Tracking

`project.org` is the canonical task tracker. Before starting any work:
1. Check `project.org` for relevant tasks and context.
2. Mark completed tasks as `** DONE` (org-mode style) when finished.

Do not add features or make significant changes without a corresponding task entry in `project.org`.

## Project Overview

rugit is a Magit-inspired TUI git client written in Rust.

### Key Files

| File | Purpose |
|------|---------|
| `src/main.rs` | Event loop, action handlers, TUI lifecycle |
| `src/app.rs` | App state, item list rebuilding |
| `src/keybindings.rs` | Key-to-action mapping |
| `src/backend/mod.rs` | Backend trait |
| `src/backend/git.rs` | libgit2 implementation |
| `src/ui/` | ratatui rendering |
| `project.org` | Task tracker (source of truth for TODOs) |

## Workflow

- Run `cargo build` to verify changes compile before finishing work.
- Write idiomatic Rust: avoid unnecessary `unwrap()`, prefer the `?` operator for error propagation.
- Do not add features not tracked in `project.org` without first adding a task entry.
