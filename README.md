# Focus Editor

A compact Linux-first Rust editor for browsing projects, editing source files, reviewing Git changes, and working with a shell and Codex.

## Build and run

Install a recent Rust toolchain and the system libraries required by eframe/winit for your Linux distribution. Git must be installed for Source Control. The Codex panel requires an installed, authenticated `codex` executable on your `PATH`.

```sh
cargo run --release
```

Open a workspace directly:

```sh
cargo run --release -- ~/projects/my-app
```

Skip automatic Codex startup for a launch:

```sh
cargo run --release -- --no-codex ~/projects/my-app
```

The first build downloads crates from crates.io. Settings are stored in the platform configuration directory, usually `~/.config/focus-editor/settings.toml`. Set `codex_app_server = false` there to disable automatic Codex startup on future launches; the panel's Start button remains available.

## Layout and appearance

- The left activity bar switches between **Explorer**, **Search**, and **Source Control**.
- The center contains file tabs, the editor, and Git diff previews.
- The bottom pane is an interactive Bash terminal, with a Restart action.
- The right pane contains Codex. Its header keeps status visible alongside horizontally scrollable session controls; **Usage / status** has its own section below.
- Drag the dividers to resize the sidebar, terminal, and Codex pane. Their sizes are saved.

The light theme uses blue navigation, teal Codex accents, colored file-type markers, vector icons, and illustrations in empty views. Source Control groups use amber, blue, green, and red. Text labels accompany status colors, and input cursors are thick and steady. The bottom status bar shows file and save feedback.

## Explorer and editing

Choose a workspace from the toolbar. Double-click a file in Explorer to open it, edit in the center, and use **Save**, **Ctrl+S**, or **Edit → Save** to write changes to disk. Source lines stay unwrapped; use horizontal scrolling for long lines. Syntax coloring uses a lightweight built-in tokenizer rather than compiler-backed language services.

Tabs remain in one horizontally scrollable row. Use the **⌄** open-files menu to search and select any open tab. Close a tab with **×** or **Ctrl+W**; closing a modified file offers **Save**, **Discard**, and **Cancel**.

### Create files and folders inline

1. Display a file in the editor, then click **New file** or **New folder** beside the Explorer title.
2. A temporary name field appears in that file's directory. Explorer expands the destination and focuses the field. If no file is open, the destination is the workspace root. For a file outside the workspace, a temporary row above the tree labels the destination path.
3. Type a single name and press **Enter** to create it on disk. Spaces and dotfiles are supported; nested paths and empty names are rejected.

**Escape**, moving focus elsewhere, switching sidebars, or changing workspaces cancels the temporary entry without creating anything. An error leaves the name available for correction. Existing files and directories are never overwritten.

Right-click a file row or a folder header (name, icon, or expand arrow) to **Rename…**, **Move to…**, or **Delete…**. Folders offer the same menu whether collapsed or expanded. Delete opens an in-app confirmation showing the target path and warnings for folder contents or unsaved edits. Click the red **Delete** button to permanently remove it, or use **Cancel**, **Escape**, or dismiss the dialog to leave it unchanged. Enter does not confirm deletion. Successfully deleted entries are removed from open tabs; deleting a symbolic link leaves its target intact.

### Search and clipboard

**Search** scans saved text files in the workspace and groups matching lines by file. Select a result to open the file and reveal the matching line with nearby context. Unreadable and non-text files are skipped.

**Ctrl+F** opens Find in the current file. Matching is case-insensitive and all matches are highlighted. Enter navigates forward, Shift+Enter navigates backward, and the arrow buttons show the active match and count. Escape closes Find.

Use **Ctrl+C** and **Ctrl+V** for the editor clipboard. **Edit** also provides Save, Copy, Paste, Find in File, and Select All.

### Restore open files

On relaunch, the editor restores the last workspace, open tabs in order, and active file. Missing or unreadable files are skipped. Launching with a different workspace argument does not restore unrelated tabs.

Files are read from disk. Unsaved edits, cursor positions, and scroll positions are not recovered.

## Source Control

Open Source Control to see branch/upstream information and saved changes. Status refreshes automatically while the view is open, and Refresh requests an immediate update. Git author configuration is required to commit.

| Group | Color | Meaning |
| --- | --- | --- |
| Unstaged | Amber | Tracked changes excluded from the next commit |
| Untracked | Blue | New files not yet staged |
| Staged | Green | Changes included in the next commit |
| Conflicts | Red | Files needing resolution before committing |

Select a file row to preview its unified diff in the center. Use **Stage file** or **Unstage file** for the selection, **Stage all** or **Unstage all** for a group, and **Open file** to edit it. A partially staged file appears in both applicable groups. Counts show additions and removals; renames, conflicts, and binary changes are identified where available.

With a file row focused, **Space** stages or unstages the whole file according to its group. It does not affect typing in the commit message or another panel. The shortcut is disabled while a Git operation is pending.

For eligible tracked text modifications, each diff hunk has a **Stage hunk** or **Unstage hunk** action. After applying a hunk, select the file again to load its updated diff. Structural changes such as new files, deletions, and renames use whole-file actions. An empty diff may mean the file is untracked or has no available text diff.

Enter a message below the file groups, then click **Commit staged changes**. Only staged content is committed; save editor edits before staging. Commits are disabled when nothing is staged, the message is empty, or conflicts remain. A successful commit clears the message; failures preserve it and report the error.

Returning to Explorer or Search shows the active editor buffer without reloading it. Returning to Source Control can show its retained diff. Selecting a file tab or closing the preview dismisses the diff.

## Codex

The panel starts `codex app-server` and exchanges JSONL messages over stdio. **Start** and **Stop** control the connection; **Interrupt** stops an active turn. Header badges show **Ready**, **Running**, **Waiting for you**, or **Offline**, with additional details on hover.

### Sessions

On startup, Codex resumes the most recently updated saved session for the workspace, including its conversation history, or starts a new one if none is available.

- Use the session dropdown to reopen another saved workspace conversation. Its selected label is short; dropdown entries are longer, with the full title on hover.
- **+** starts a new session.
- **↻** refreshes the session list.
- **…**, when present, loads older sessions.

Session switching and creation are disabled during active work or pending user responses.

### Send, model, thinking, and mode

Type in the message box and click **Send** or press **Ctrl+Enter** while it is focused. The bottom controls appear in this order: **Model → Thinking → Mode**. They share the send row when space permits and wrap on narrow panels.

Available models, thinking levels, and modes come from the installed Codex catalog. Selections apply to the next message. A thinking level is preserved across model changes when supported; otherwise the new model's default is used. Models without thinking options show a disabled Default control. Changing mode preserves the selected thinking level.

Codex initially selects **Plan** when offered, otherwise the first returned mode. While the message box is focused and Codex is idle, **Tab** cycles modes and **Shift+Tab** cycles backward.

### Replies, plans, and requests

Replies appear as message cards. Commands, file activity, and readable reasoning are expandable. New turns request automatic reasoning summaries. The panel prefers a supplied summary and falls back to readable reasoning content supplied by Codex, preserving section boundaries and streamed text in live and restored conversations. An empty running reasoning item shows **Thinking…**; empty completed items are hidden. Reasoning cannot be displayed when Codex supplies neither a summary nor readable content.

Proposed plans are displayed as expanded cards and restored with session history. Completion appears in the conversation as **Plan ready**, **Response complete**, or a failure/interruption message, in addition to the header status.

When the latest plan has completed successfully and Plan mode is selected, **Go** appears beneath that plan. Click it to switch to the available Default mode and send exactly **Go** in the same session. The model, thinking level, and unsent draft are preserved. The button is disabled while busy, disconnected, awaiting your response, or when Default mode is unavailable. A failed submission restores the plan action for an explicit retry; it never resends automatically. The latest completed plan can also offer Go when reopening a saved session. Normal approval prompts still apply.

Approval requests provide Allow, Allow this session, and Decline actions. Input requests provide a response field. Codex waits for your response before continuing where required.

### Usage

Expand **Usage / status** for the session ID, model, account limits, session tokens, and context usage when supplied by Codex. Account limits refresh every minute while connected; **Refresh usage** requests an immediate update. Unsupported or unavailable account usage is shown explicitly. Token/context figures update from session events.

## Keyboard reference

| Shortcut | Context | Action |
| --- | --- | --- |
| Ctrl+S | Editor | Save the file |
| Ctrl+W | Editor | Close the active tab |
| Ctrl+F | Editor | Open Find |
| Enter / Shift+Enter | Find field | Next / previous match |
| Ctrl+C / Ctrl+V | Editor | Copy / paste |
| Enter | New-entry field | Create the file or folder |
| Escape | New-entry field | Cancel creation |
| Escape | Delete confirmation | Cancel deletion |
| Space | Focused Source Control file row | Stage / unstage the file |
| Ctrl+Enter | Codex message box | Send |
| Tab / Shift+Tab | Codex message box, while idle | Next / previous mode |

## Source organization

The application uses one crate with three feature module directories:

- `src/editor/mod.rs` owns application state and coordinates the window. Its sibling modules separate layout/sidebar routing, document and tab actions, text rendering, Explorer, inline creation, confirmed deletion, search/Find, shell rendering, and persistence.
- `src/codex_ui/mod.rs` owns Codex panel state and connection lifecycle. Its sibling modules separate the header, transcript, composer, catalog helpers, usage, outgoing requests, incoming responses/events, message updates, indexed reasoning sections, and completed-plan/Go state.
- `src/git_ui/mod.rs` owns Source Control state. Its sibling modules separate repository access/status parsing, background actions and replies, panel rendering, diff/hunk controls, and presentation helpers. Git tests live in `src/git_ui/tests.rs`.
- Editor tests are grouped by feature under `src/editor/tests/`. Codex tests cover panel layout, selection helpers, history, and event routing in `src/codex_ui/tests.rs`.

Other top-level modules provide the shared theme/icons, settings, terminal state, syntax coloring, and the App Server transport. Panel methods preserve their widget scopes and IDs; implementation details remain private to their feature module.

See [PRODUCT.md](PRODUCT.md) for the current product scope and [PLAN.md](PLAN.md) for implementation and validation notes.
