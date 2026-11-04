# Implementation and validation notes

## Implemented areas

| Area | Current implementation |
| --- | --- |
| Desktop foundation | Linux and macOS Rust/eframe application, directory launch argument, persistent settings and adjustable pane sizes |
| Visual design | Shared light palette, vector icons, file-type markers, active-tab/focus styling, status badges, empty-state illustrations, bottom status bar |
| Explorer | Workspace tree; open, rename, move, inline file/folder creation, and confirmed deletion |
| Inline creation | Target the displayed file's parent or workspace fallback; reveal nested destinations; Enter creates; Escape/focus loss cancels; reject invalid/duplicate names |
| Confirmed deletion | In-app modal with Cancel default, permanent-deletion warnings, affected-tab cleanup on success, and symlink-target protection |
| Editor | Text editing/saving, lightweight syntax colors, unwrapped source lines, horizontal scrolling, single-click preview tabs, double-click/edit pinning, per-file unsaved contents and Undo state, searchable open-files menu, modified-file close confirmation |
| Search and clipboard | Grouped workspace search, match navigation, Ctrl+F Find with highlights/count, clipboard and Edit actions |
| Session persistence | Restore last workspace, open-tab order, and active file from disk; skip unavailable files; isolate a different launch workspace |
| Source Control | Asynchronous Git status/actions, colored status groups, file/group staging, eligible hunk staging, commits, Space shortcut, editor-area diffs |
| Shell | Bottom interactive PTY-backed Bash terminal, Restart action, exclusive focused keyboard routing, zsh key encoding, application-cursor/bracketed-paste modes |
| Assistant pane | Independent Codex/OpenCode enable switches, status tabs, hidden-client event processing, full editor width when both disabled, saved visibility and workspace state |
| OpenCode | Native local HTTP/SSE client, session/history restore, provider/model/variant/agent catalogs, streaming messages/tools/reasoning, permissions/questions, interrupt, Plan-to-Build Go, reported usage |
| Codex sessions | JSONL App Server client, automatic latest-session resume, session picker/pagination, new session, Start/Stop, Interrupt |
| Codex controls | Catalog-based model/thinking/mode choices, Plan-first startup fallback, Ctrl+Enter send, Tab/Shift+Tab mode cycling, wrapping controls |
| Codex output | Streaming message cards, expanded proposed plans, restored plan history, reasoning/activity details, completion markers, approval/input requests |
| Codex usage | Available account limits with periodic/manual refresh, token/context updates, unavailable-data feedback |

The application remains a single crate. `src/editor/mod.rs` owns application state and calls focused modules for layout, document lifecycle, tabs, clipboard, close confirmation, document path changes, text rendering, Explorer, creation/deletion, search, shell, sidebar routing, and persistence. `src/codex_ui/mod.rs` owns panel state and delegates to header, conversation, composer, catalog, usage, request, response, event, message, reasoning-section, and completed-plan/Go modules. `src/git_ui/mod.rs` owns Source Control state and delegates repository/status handling, asynchronous actions/replies, panel controls, aligned comparison rendering, byte-preserving patch parsing, retained reviews, file/index validation, hunk application, and presentation to focused sibling modules. Theme, settings, terminal, syntax, and App Server transport remain separate top-level modules.

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
- [ ] Single-click files to replace one preview; double-click a file/tab or edit to pin it. Switch among modified files, verify their contents/Undo/modified indicators, and save/discard/cancel inactive tab closes.
- [ ] Edit/save/close files; check long lines, tab scrolling/menu, search navigation, Find, and clipboard shortcuts.
- [ ] Review staged/unstaged/untracked/conflicting files; use Space and buttons; stage/unstage hunks without losing the comparison or scroll position; verify blue staged backgrounds and reverse actions; reload after external changes; commit and inspect failures.
- [ ] Switch between Source Control, Explorer, Search, and editor tabs without replacing the active unsaved buffer with a diff.
- [ ] Type a commit message through multiple automatic refreshes without focus loss or layout flicker.
- [ ] Exercise shell input, resize, and Restart; run zsh and check completion, history, Ctrl/Alt editing, interrupt, paste, and click-based focus transfer.
- [ ] With Codex configured, resume/switch/create sessions; choose model/thinking/mode; verify keyboard shortcuts, streaming plans, completion, Interrupt, approvals, input requests, and usage refresh.

## Current limits

The app does not recover unsaved buffers or caret/scroll positions on relaunch. Git history, branch switching, and individual-line staging are outside the implemented UI. Diff previews may need a file reselection after index changes. Codex choices and usage fields depend on the installed server/account, and reasoning cannot be displayed when the server supplies neither a readable summary nor content.

Follow-up work should extend the implementation and its validation together; planned features should not be documented as already available.

## Background launch and workspace title

Implemented: Linux session detachment before GUI initialization, `--foreground`, append-only background launch logging, argument/directory/environment preservation, and dynamic workspace window titles. Launch handling lives in `src/launch.rs`. Tests cover detachment, redirected streams, argument preservation, recursion prevention, and workspace title changes.

Manual desktop check: launch from a terminal, confirm the prompt returns while the window stays open, verify its workspace title in the Linux window list, switch workspaces, and check `--foreground` retains terminal diagnostics.

Terminal keyboard routing runs through the pre-frame input hook before egui navigation and editor shortcuts. Encoding and terminal input modes are isolated in `src/terminal_input.rs`; tests cover focus transfer, exact byte sequences, mode updates, and a real zsh PTY when zsh is installed.

## OpenCode integration validation

The OpenCode transport and panel use focused sibling modules; the editor coordinates their lifecycles with Codex. Mock HTTP/SSE and UI tests cover request routing, reconnect snapshots, session generations, approvals/questions, model/agent choices, send shortcuts, Go, narrow layouts, and space reclamation. An ignored integration test checks the installed OpenCode server's health/catalogs and process cleanup without model prompts: `cargo test opencode_server::tests::installed_server_catalog_smoke -- --ignored`.

Manual desktop checks: enable both assistants, switch during work and pending approvals, disable each individually, disable both and check editor width, re-enable and check drafts/session restoration, then switch workspaces and relaunch to check saved preferences. Provider-level generation behavior remains dependent on configured accounts and is exercised by normal user prompts.

## Logging validation

Application logging uses a dedicated argument parser and a single `log`/`env_logger` initialization before workers start. Tests cover level parsing, workspace ordering, precedence, dependency filtering, off/error levels, and background forwarding. Manual troubleshooting: launch with `--foreground --log-level debug .` or inspect the background launch log. Discrete actions are instrumented without content/payload logging or per-frame rendering logs.

Dev Container diagnostics now leave failure details in the application log rather than the toolbar menu. Tests cover bounded/redacted diagnostics, transition-only warnings, compact repeated menu rendering, configuration failures, and successful lifecycle Info records without starting Docker.

Dev Container build-failure capture preserves stdout and stderr with process exit status, bounds long streams while retaining their tails, and writes separate diagnostic records. BuildKit fixtures verify that command redaction retains the failure explanation and exit code; stdout-only errors and credential redaction are covered without rebuilding containers.

Dev Container assistant provisioning and authentication use official native curl installers and a focused authentication module. Validate noninteractive CLI availability, host path overrides, configured-user home resolution, atomic private writes, preservation of installed packages/history, and safe failure reporting. Container connection runs credential transfer on the lifecycle worker before assistant startup; reconnect refreshes the copies. No session/configuration synchronization or assistant-data mounts.

Validated the bundled image with Codex 0.166.0 and OpenCode 1.19.5 as `vscode` through noninteractive Docker exec. Authentication-only transfer preserved host file hashes and installed Codex packages, produced 0600 files owned by `vscode`, and passed Codex login status (ChatGPT). OpenCode server health and connected-provider metadata passed without sending a model prompt. Automated suite: 85 passed, one optional installed-server test ignored; compile, formatting, and whitespace checks passed.

## Multi-platform release validation

Release packaging uses native Ubuntu 24.04 amd64/arm64 and macOS 15 arm64 runners, stable Rust, and the committed lockfile. Validate tests/formatting, executable architectures, Linux shared-library resolution, macOS system-library-only linkage, bundle metadata, ad-hoc signatures, read-only DMG mounting, archive contents, and the complete three-asset checksum set before attaching downloads to an existing published release. Locally validate the workflow and Linux packaging; native macOS/arm64 build results require hosted runs, and Finder/GUI behavior requires a Mac. No release is created as part of implementation.

Local release validation passed: actionlint, 89 Rust tests (one optional integration test ignored), two Python packaging tests, Linux compilation, a native Linux amd64 optimized build and verified tar.gz, plus compile-only checks of the application and tests for macOS arm64. Native Linux arm64 execution, macOS linking/signing/DMG mounting, and release upload remain exercised by the hosted workflow. No release was created or uploaded.
