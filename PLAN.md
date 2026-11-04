# Implementation and validation notes

## Implemented areas

| Area | Current implementation |
| --- | --- |
| Desktop foundation | Linux Rust/eframe application, directory launch argument, persistent settings and adjustable pane sizes |
| Visual design | Shared light palette, vector icons, file-type markers, active-tab/focus styling, status badges, empty-state illustrations, bottom status bar |
| Explorer | Workspace tree; open, rename, move, inline file/folder creation, and confirmed deletion |
| Inline creation | Target the displayed file's parent or workspace fallback; reveal nested destinations; Enter creates; Escape/focus loss cancels; reject invalid/duplicate names |
| Confirmed deletion | In-app modal with Cancel default, permanent-deletion warnings, affected-tab cleanup on success, and symlink-target protection |
| Editor | Text editing/saving, lightweight syntax colors, unwrapped source lines, horizontal scrolling, tabs, searchable open-files menu, modified-file close confirmation |
| Search and clipboard | Grouped workspace search, match navigation, Ctrl+F Find with highlights/count, clipboard and Edit actions |
| Session persistence | Restore last workspace, open-tab order, and active file from disk; skip unavailable files; isolate a different launch workspace |
| Source Control | Asynchronous Git status/actions, colored status groups, file/group staging, eligible hunk staging, commits, Space shortcut, editor-area diffs |
| Shell | Bottom interactive PTY-backed Bash terminal and Restart action |
| Codex sessions | JSONL App Server client, automatic latest-session resume, session picker/pagination, new session, Start/Stop, Interrupt |
| Codex controls | Catalog-based model/thinking/mode choices, Plan-first startup fallback, Ctrl+Enter send, Tab/Shift+Tab mode cycling, wrapping controls |
| Codex output | Streaming message cards, expanded proposed plans, restored plan history, reasoning/activity details, completion markers, approval/input requests |
| Codex usage | Available account limits with periodic/manual refresh, token/context updates, unavailable-data feedback |

The application remains a single crate. `src/editor/mod.rs` owns application state and calls focused modules for layout, documents, text rendering, Explorer, creation/deletion, search, shell, sidebar routing, and persistence. `src/codex_ui/mod.rs` owns panel state and delegates to header, conversation, composer, catalog, usage, request, response, event, message, reasoning-section, and completed-plan/Go modules. `src/git_ui/mod.rs` owns Source Control state and delegates repository/status handling, asynchronous actions/replies, panel controls, aligned comparison rendering, byte-preserving patch parsing, retained reviews, file/index validation, hunk application, and presentation to focused sibling modules. Theme, settings, terminal, syntax, and App Server transport remain separate top-level modules.

The original monolithic editor, Codex UI, and Git UI source files have been replaced by these directories. Feature tests live under `src/editor/tests/` and in `src/codex_ui/tests.rs` with focused reasoning tests under `src/codex_ui/tests/`. Git tests live in `src/git_ui/tests.rs` and `src/git_ui/tests/`. Public entry types remain `editor::EditorApp`, `codex_ui::CodexUi`, and `git_ui::GitUi`; internal helpers use module-scoped visibility.

## Automated checks

Run:

```sh
cargo check
cargo test
cargo fmt --check
```

The current suite covers:

- Restoring tab order and the active file, skipping missing files, older settings, and workspace isolation.
- Inline file/folder creation, initial focus, Enter submission, Escape/focus-loss cancellation, invalid/duplicate names, nested-folder reveal, external destinations, and workspace/sidebar cancellation.
- File/folder delete confirmation cancellation, Enter safety, failed deletion, open-tab cleanup, folder removal, and symbolic-link target preservation; right-click Delete on collapsed and expanded folder headers.
- Window layout at compact and standard sizes; long Codex messages/plans and composer visibility at narrow widths.
- Icon-button keyboard activation and disabled actions.
- Git status parsing for partial changes, renames, unusual names, and conflicts; repository staging/unstaging/commit operations; commit draft/focus stability across refresh and failure.
- Codex history conversion, thinking-level selection/fallback, startup mode preference/fallback, and usage-title formatting.
- Codex event routing for streamed/final plans, preserved reasoning summaries, unknown item types, thread isolation, correlated responses, server questions, errors, and shutdown.
- Go request payloads and mode changes, draft/model/thinking preservation, duplicate prevention, submission rejection/transport failure, restored and stale plan readiness, session changes, unavailable-action states, and narrow-panel button interaction.
- Summary/content fallback in live and restored reasoning, indexed sections, empty completion preservation, summary preference without duplication, automatic-summary requests, and the Thinking/hidden-empty presentation states.

Passing these checks does not establish full manual desktop or live Codex validation.

## Manual acceptance checklist

These checks remain manual acceptance tasks, rather than claims that every desktop interaction has already been verified.

- [ ] Launch with no workspace, a saved workspace, a directory argument, and `--no-codex`; verify expected pane and session restoration.
- [ ] Resize all panes and verify labels, icons, focus outlines, scrollbars, and message wrapping at the minimum window size.
- [ ] Create files/folders beside root, nested, and external open files; confirm focus, cancellation, errors, and no overwrite.
- [ ] Confirm and cancel deletion of files and folders, including targets with unsaved edits and filesystem errors.
- [ ] Edit/save/close files; check long lines, tab scrolling/menu, search navigation, Find, and clipboard shortcuts.
- [ ] Review staged/unstaged/untracked/conflicting files; use Space and buttons; stage/unstage hunks without losing the comparison or scroll position; verify blue staged backgrounds and reverse actions; reload after external changes; commit and inspect failures.
- [ ] Switch between Source Control, Explorer, Search, and editor tabs without replacing the active unsaved buffer with a diff.
- [ ] Type a commit message through multiple automatic refreshes without focus loss or layout flicker.
- [ ] Exercise shell input, resize, and Restart.
- [ ] With Codex configured, resume/switch/create sessions; choose model/thinking/mode; verify keyboard shortcuts, streaming plans, completion, Interrupt, approvals, input requests, and usage refresh.

## Current limits

The app does not recover unsaved buffers or caret/scroll positions on relaunch. Git history, branch switching, and individual-line staging are outside the implemented UI. Diff previews may need a file reselection after index changes. Codex choices and usage fields depend on the installed server/account, and reasoning cannot be displayed when the server supplies neither a readable summary nor content.

Follow-up work should extend the implementation and its validation together; planned features should not be documented as already available.
