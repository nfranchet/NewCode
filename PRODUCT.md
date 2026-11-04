# Product brief: Focus Editor

## Goal

A fast, Linux-first Rust desktop editor for project file management, source editing, Git review, an interactive shell, and Codex. Keep the working layout compact and make common actions easy to identify.

## Window and visual design

The left activity bar selects Explorer, Search, or Source Control. The center displays open files or a Source Control diff. An interactive shell occupies the bottom and an optional Codex/OpenCode assistant pane occupies the right. Pane sizes are adjustable and saved.

A coordinated light theme uses blue navigation, teal Codex controls, semantic Git colors, vector icons, colored file-type markers, and small empty-state illustrations. Focus, selected tabs, and status badges remain visible. Labels accompany colors, and input cursors are thick and steady. A bottom status bar reports file/save activity.

## Explorer and documents

- Single-click Explorer files to open one replaceable preview tab. Double-click an Explorer file or its tab to keep it permanently; editing also pins it. Use italic preview labels with an explanatory tooltip. Selecting a permanent file activates its existing tab. An optional launch directory selects the workspace.
- Create a file or folder through a temporary inline name field beside the displayed file, with the workspace root as the fallback when no file is open. Reveal nested destinations; label destinations outside the workspace above the tree.
- Create on Enter only. Escape, focus loss, sidebar changes, or workspace changes cancel without filesystem writes. Validate names, preserve failed drafts for correction, and never overwrite existing entries.
- Provide right-click Rename, Move, and Delete actions. Deletion requires an in-app confirmation, warns about folder contents and unsaved edits, and removes affected tabs only on success. Deleting a symbolic link preserves its target.
- Edit/save text with lightweight extension-based syntax coloring. Keep source lines unwrapped and horizontally scrollable.
- Keep tabs on one scrollable row with close controls and a searchable open-files menu. Closing a modified file asks whether to save, discard, or cancel.
- Preserve unsaved contents and separate Undo state per open file during tab switches. Modified indicators and close confirmations cover inactive files too; Save targets the file being closed. Keep preview and document state synchronized during rename, move, and deletion.
- Restore all tabs as permanent, including the last preview; restore workspace, tab order, and active file on relaunch, skipping missing/unreadable files. A different launch workspace does not inherit unrelated tabs. Restore saved disk contents, not unsaved buffers or caret/scroll positions.

## Search and clipboard

Workspace search scans readable text files and groups matching lines by filename. Selecting a result opens the file with the matching line and nearby preceding context visible.

Ctrl+F opens case-insensitive Find in the active file, with match highlights, a count, and forward/backward navigation. Provide editor clipboard shortcuts and Edit-menu actions for Save, Copy, Paste, Find, and Select All.

## Source Control

Use a compact status view with branch/upstream information and distinct bordered groups: amber Unstaged, blue Untracked, green Staged, and red Conflicts. Identify file states and available addition/deletion counts; partially staged files may appear in both staged and unstaged groups.

Selecting a file displays its aligned Before/After comparison in the center. Provide whole-file/group staging and unstaging, plus hunk actions for eligible tracked text changes. Space stages/unstages the focused file row without interfering with text entry. Enter a commit message below the list and commit staged changes only; block commits for unresolved conflicts, an empty message, or no staged content.

Run Git operations asynchronously and refresh status while the view is open. Preserve commit-field focus and text during refreshes. Keep the comparison and scroll position after hunk actions; tint staged hunks blue on both sides with a Staged label and offer the reverse action. Retain red/green change indicators. Apply original patch bytes so unrelated staged changes remain intact. Detect changes outside the review and require Reload diff; whole-file operations also invalidate the review. Returning to Explorer/Search restores the active editor buffer; selecting an editor tab dismisses the preview.

## Shell and Codex

The bottom pane is a real PTY-backed interactive shell with a Restart action. While focused, it owns keyboard input before panel navigation or editor shortcuts: Tab/Shift+Tab, Escape, navigation, F1–F12, and Ctrl/Alt combinations go to the shell. Clicking another panel transfers focus normally. Ctrl+C interrupts and Ctrl+V pastes. Respect application-cursor and bracketed-paste modes for shells such as zsh. The right pane is the Codex App Server client over JSONL stdio, with Start/Stop, visible connection/activity status, streaming turns, Interrupt, approval responses, and user-input responses.

Resume the latest workspace session at startup and allow choosing older sessions or creating a new one. Session labels are compact when closed and longer in the dropdown. Provide model, thinking-level, and mode selectors near Send, populated from Codex's catalog. Prefer Plan initially, with the first available mode as fallback. Support Ctrl+Enter to send and Tab/Shift+Tab to cycle modes in the idle composer.

Show proposed plans as expanded message cards and retain them in restored history. Request automatic reasoning summaries, preserve streamed sections, and show supplied reasoning content when no summary is available. Keep readable reasoning and command details expandable; show Thinking while an empty reasoning item is running, hide empty completed items, and report turn completion in the conversation. Show available account limits and token/context usage; refresh account limits every minute and on request.

After a successful completed plan, provide a Go action in Plan mode that selects Default and sends Go in the same session, preserving the selected model, thinking level, and unsent draft. Disable it during work, pending user responses, disconnected sessions, or unavailable Default mode; prevent duplicate submissions and allow explicit retry after submission failure.

## Assistant switching and OpenCode

Provide an always-accessible Assistants toolbar menu with independent Codex and OpenCode enable switches. Use status-bearing tabs in one right pane when both are enabled. Inactive tabs continue processing events and retain their drafts, selections, conversation, and scroll state. Disabling stops and hides that assistant; disabling both releases the entire right-pane width to the editor. Save visibility, selected tab, and pane width. Initially enable Codex only. Keep Start/Stop separate from visibility.

OpenCode uses an owned localhost server and existing CLI configuration/authentication. Provide current-workspace session restore/selection/creation, streaming messages, reasoning/tool details, catalog-driven provider/model/variant/primary-agent choices, Plan-first fallback, Ctrl+Enter send, Tab/Shift+Tab agent cycling, interrupt, native permission replies, and user questions. A completed successful Plan response can offer Go to select Build and send Go while preserving the draft. Display reported tokens/cost/context information and explicitly unavailable account quotas. Poll enabled clients independently of rendering; recover stream state without resending prompts. Keep workspace sessions and drafts isolated; stop old workspace connections on switching. Clean up owned processes on disable/exit.

## Platform and boundaries

- Linux and Apple Silicon macOS desktop application using Rust and eframe/egui, with native filesystem operations and a PTY.
- Support Compose-based Dev Containers directly through Docker and the Docker Compose plugin. The native editor keeps the bind-mounted files on the host while the shell, Git, Codex, and OpenCode execute with `docker exec` in the selected service. Do not require, install, or invoke Node.js or the Dev Container CLI. Containers remain running on editor exit; start, reconnect, stop, and rebuild are explicit toolbar actions.
- Interpret only the Compose selection fields needed to connect (`dockerComposeFile`, `service`, `workspaceFolder`, and `remoteUser`). Dockerfiles and Compose define provisioning; Dev Container Features and lifecycle commands are not interpreted.
- Git is required for Source Control; an installed, authenticated Codex CLI is required for the Codex panel. Both capabilities depend on their local tools and configuration.
- Automatic Codex startup can be disabled in saved settings or with `--no-codex`; `--no-opencode` skips OpenCode startup. Visibility is independently controlled by the Assistants menu.
- No extension marketplace, debugger, compiler-backed language services, Git history/branch-management UI, individual-line staging, or unsaved-buffer recovery is currently provided.
- Renaming and moving use existing dialogs; new-entry naming and delete confirmation are handled inside the application.

## Launch behavior

On Linux, launch in the background by default, preserving the calling directory, environment, workspace argument, and `--no-codex`. Provide `--foreground` for terminal diagnostics. Append background output/errors to the local data directory under `focus-editor/logs/launch.log`; report failures to spawn or open the log in the terminal. Each launch opens an independent editor instance.

Use the native window title `Focus Editor — workspace-name`, updating it when a workspace is restored, supplied at launch, or changed in the UI. Use the full path for filesystem roots and `Focus Editor` without a workspace. Keep the application identity stable for Linux desktop grouping.

## Diagnostics

Default application logging to Info, reserving it for important lifecycle paths. Use Debug for discrete application actions, Warn for recovered failures/configuration issues, Error for fatal failures, and Trace for low-level metadata. Provide `--log-level LEVEL` and `--log-level=LEVEL`, preserve them across background launching, and keep their values out of workspace selection. Explicit levels override `RUST_LOG`; otherwise honor component filters and default dependencies to Warn. Include timestamp, level, PID, and component. Write stderr in the foreground and the existing launch log in the background. Exclude document/prompt/answer/terminal contents, raw payloads, credentials, and per-frame rendering noise.

Dev Container failures use compact menu feedback with Retry; detailed bounded diagnostics go to the application log once per configuration or lifecycle failure, with credentials and build-command payloads redacted. Preserve stdout, stderr, and process status; emit separate timestamped records for diagnostic lines, keeping surrounding failure explanations and exit codes. Successful connection and shutdown use Info.

Dev Container assistant authentication: install both native CLIs with official curl installers in the bundled image. Before container connection/reconnection, copy only host authentication files atomically with private permissions as the configured remote user; preserve package installations. Forward supported provider/server authentication environment variables by name. Keep history, databases, configuration, and refreshed container credentials independent. Missing host files retain container credentials with a warning; failed transfers fail connection without logging secrets.

## Release distribution

Published GitHub releases and prereleases build the exact release tag for Linux amd64 and Linux arm64 on native hosted runners. Package Linux executables with installation notes and the Apache license in tar.gz archives. Publish both verified archives and SHA-256 checksums only after both builds pass; macOS builds and DMGs are skipped for automatic releases. Retain macOS source compatibility and manual app/DMG packaging. Use Cargo metadata for executable/bundle versions, tag-based download names, least-privilege GitHub tokens, and rerunnable uploads. Do not bundle external Git, Docker, or assistant installations. macOS starts its GUI and discovers common user/Homebrew tool paths when opened from Finder; Developer ID signing/notarization is outside this workflow. Linux background launching is unchanged.
