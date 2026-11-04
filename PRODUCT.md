# Product brief: Focus Editor

## Goal

A fast, Linux-first Rust desktop editor for project file management, source editing, Git review, an interactive shell, and Codex. Keep the working layout compact and make common actions easy to identify.

## Window and visual design

The left activity bar selects Explorer, Search, or Source Control. The center displays open files or a Source Control diff. An interactive shell occupies the bottom and Codex occupies the right. Pane sizes are adjustable and saved.

A coordinated light theme uses blue navigation, teal Codex controls, semantic Git colors, vector icons, colored file-type markers, and small empty-state illustrations. Focus, selected tabs, and status badges remain visible. Labels accompany colors, and input cursors are thick and steady. A bottom status bar reports file/save activity.

## Explorer and documents

- Browse the workspace tree and double-click files to edit them. An optional launch directory selects the workspace.
- Create a file or folder through a temporary inline name field beside the displayed file, with the workspace root as the fallback when no file is open. Reveal nested destinations; label destinations outside the workspace above the tree.
- Create on Enter only. Escape, focus loss, sidebar changes, or workspace changes cancel without filesystem writes. Validate names, preserve failed drafts for correction, and never overwrite existing entries.
- Provide right-click Rename, Move, and Delete actions. Deletion requires an in-app confirmation, warns about folder contents and unsaved edits, and removes affected tabs only on success. Deleting a symbolic link preserves its target.
- Edit/save text with lightweight extension-based syntax coloring. Keep source lines unwrapped and horizontally scrollable.
- Keep tabs on one scrollable row with close controls and a searchable open-files menu. Closing a modified file asks whether to save, discard, or cancel.
- Restore workspace, tab order, and active file on relaunch, skipping missing/unreadable files. A different launch workspace does not inherit unrelated tabs. Restore saved disk contents, not unsaved buffers or caret/scroll positions.

## Search and clipboard

Workspace search scans readable text files and groups matching lines by filename. Selecting a result opens the file with the matching line and nearby preceding context visible.

Ctrl+F opens case-insensitive Find in the active file, with match highlights, a count, and forward/backward navigation. Provide editor clipboard shortcuts and Edit-menu actions for Save, Copy, Paste, Find, and Select All.

## Source Control

Use a compact status view with branch/upstream information and distinct bordered groups: amber Unstaged, blue Untracked, green Staged, and red Conflicts. Identify file states and available addition/deletion counts; partially staged files may appear in both staged and unstaged groups.

Selecting a file displays its unified diff in the center. Provide whole-file/group staging and unstaging, plus hunk actions for eligible tracked text changes. Space stages/unstages the focused file row without interfering with text entry. Enter a commit message below the list and commit staged changes only; block commits for unresolved conflicts, an empty message, or no staged content.

Run Git operations asynchronously and refresh status while the view is open. Preserve commit-field focus and text during refreshes. Hunk changes require reselecting the file to reload its diff. Returning to Explorer/Search restores the active editor buffer; selecting an editor tab dismisses the preview.

## Shell and Codex

The bottom pane is a real PTY-backed interactive shell with a Restart action. The right pane is the Codex App Server client over JSONL stdio, with Start/Stop, visible connection/activity status, streaming turns, Interrupt, approval responses, and user-input responses.

Resume the latest workspace session at startup and allow choosing older sessions or creating a new one. Session labels are compact when closed and longer in the dropdown. Provide model, thinking-level, and mode selectors near Send, populated from Codex's catalog. Prefer Plan initially, with the first available mode as fallback. Support Ctrl+Enter to send and Tab/Shift+Tab to cycle modes in the idle composer.

Show proposed plans as expanded message cards and retain them in restored history. Request automatic reasoning summaries, preserve streamed sections, and show supplied reasoning content when no summary is available. Keep readable reasoning and command details expandable; show Thinking while an empty reasoning item is running, hide empty completed items, and report turn completion in the conversation. Show available account limits and token/context usage; refresh account limits every minute and on request.

## Platform and boundaries

- Linux desktop application using Rust and eframe/egui, with native filesystem operations and a PTY.
- Git is required for Source Control; an installed, authenticated Codex CLI is required for the Codex panel. Both capabilities depend on their local tools and configuration.
- Automatic Codex startup can be disabled in saved settings or for one launch with `--no-codex`.
- No extension marketplace, debugger, compiler-backed language services, Git history/branch-management UI, individual-line staging, or unsaved-buffer recovery is currently provided.
- Renaming and moving use existing dialogs; new-entry naming and delete confirmation are handled inside the application.
