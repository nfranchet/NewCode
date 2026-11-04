# Focus Editor

A compact Rust editor for Linux and Apple Silicon macOS. Browse projects, edit source files, review Git changes, and work with a shell, Codex, and OpenCode.

## Release downloads

Publishing a GitHub release (including a prerelease) builds the tagged source and attaches:

- `focus-editor-<tag>-linux-amd64.tar.gz`
- `focus-editor-<tag>-linux-arm64.tar.gz`
- `focus-editor-<tag>-macos-arm64.dmg`
- `SHA256SUMS`

Linux binaries are built on Ubuntu 24.04 and require glibc 2.39 or newer, an X11 or Wayland desktop, and working OpenGL/EGL libraries. They are dynamically linked; run `ldd focus-editor` to identify missing runtime libraries. A desktop portal provides native file dialogs. Extract the archive and install the executable with `install -m 755 focus-editor ~/.local/bin/focus-editor` after creating that directory.

On Apple Silicon macOS 11 or newer, open the DMG and drag **Focus Editor.app** to **Applications**. The app has an ad-hoc signature and is not notarized or signed with an Apple Developer ID. If macOS blocks the first launch, attempt to open it, then use **System Settings → Privacy & Security → Open Anyway** when offered. The macOS app stays attached when launched from a terminal:

```sh
/Applications/Focus\ Editor.app/Contents/MacOS/focus-editor ~/projects/my-project
```

Finder launches retain the inherited PATH and add `~/.local/bin`, `~/.opencode/bin`, `/opt/homebrew/bin`, and `/usr/local/bin` for external tools. Git, Docker, Codex, and OpenCode are separate installations; authenticate assistants with their CLI first. Run `focus-editor --version` (or the app's executable path with `--version`) to check the version without opening a window.

To publish: merge the workflow and packaging scripts into `master`, bump the Cargo package version as appropriate, create a tag containing those changes, then publish a release for that tag. Saving a draft or pushing a tag alone does not build downloads. All three native builds and packaging checks must pass before any assets are uploaded. Rerun failed jobs from GitHub Actions to retry; matching assets are replaced. Release tags containing filename-unsafe characters such as `/` use `-` in download names. The bundle and executable version come from Cargo metadata; release filenames use the tag.

The workflow needs no personal access token or Apple credentials: its final upload job uses GitHub's built-in token with `contents: write`. Native Linux amd64/arm64 and macOS arm64 builds run tests, validate architectures and runtime dependencies, and verify package contents. The macOS job validates the app signature and mounts the DMG read-only before publishing. Builds are not triggered until a release is published; local checks cannot substitute for a GUI launch check on a Mac.

## Build and run

Install a recent Rust toolchain and the system libraries required by eframe/winit for your Linux distribution, or Xcode Command Line Tools on macOS. Git must be installed for Source Control. The Codex panel requires an installed, authenticated `codex` executable on your `PATH`. OpenCode requires an installed `opencode` executable with a configured provider; it is disabled by default.

```sh
cargo run --release
```

On Linux, the editor opens in the background and immediately releases the terminal. macOS uses the normal application lifecycle. Each launch opens a separate instance. Its window title is **Focus Editor — workspace-name**, identifying the current workspace in Linux window lists that show window titles. Relative workspace paths are resolved for display: launching with `.` shows the actual folder name in the title and toolbar, with the full resolved path in the toolbar tooltip.

Keep the editor attached to the terminal for debugging:

```sh
cargo run --release -- --foreground
# Or, for an installed executable:
focus-editor --foreground
```

On Linux, background output and startup errors are appended to `$XDG_DATA_HOME/focus-editor/logs/launch.log`, usually `~/.local/share/focus-editor/logs/launch.log`. Failures to create the log or spawn the editor are reported in the launching terminal. Errors after detachment are recorded in the log.

Open a workspace directly:

```sh
cargo run --release -- ~/projects/my-app
```

## Dev Containers

Focus Editor can use a Docker Compose development container without Node.js or the Dev Container CLI. With a workspace open, use **Dev Container** in the toolbar to start the service defined by `.devcontainer/devcontainer.json`. The container stays running when the editor closes; use **Stop** or **Rebuild** from the same menu when needed.

The built-in backend supports Compose-based configurations with `dockerComposeFile`, `service`, `workspaceFolder`, and optional `remoteUser`. It starts the service with Docker Compose, then runs the integrated shell, Source Control commands, Codex, and OpenCode with `docker exec` in that container. Explorer, search, editing, and saved files stay on the host bind mount.

Docker Engine and the Docker Compose plugin are required. Node.js and `@devcontainers/cli` are not required or installed. The bundled image installs Codex and OpenCode using their official curl installers as `vscode`, with both commands available to noninteractive `docker exec`. Custom images must provide the tools you enable. Dev Container Features and lifecycle commands are not interpreted by Focus Editor; express required setup in the Dockerfile or Compose service instead.

Skip automatic Codex startup for a launch:

```sh
cargo run --release -- --no-codex ~/projects/my-app
```

The first build downloads crates from crates.io. Settings are stored in the platform configuration directory, usually `~/.config/focus-editor/settings.toml`. Set `codex_app_server = false` there to disable automatic Codex startup on future launches; the panel's Start button remains available.

The bundled Rust Dev Container image updates the stable Rust toolchain during its build, installs matching `llvm-tools-preview`, and installs `grcov` with its published lockfile. Use **Dev Container → Rebuild** after changing the Dockerfile. The image also runs the official installers at `https://chatgpt.com/codex/install.sh` and `https://opencode.ai/install`; rebuilding selects their current default releases.

## Logging

On **Open**, **Reconnect**, and **Rebuild**, Focus Editor copies only host Codex/OpenCode `auth.json` files into the configured container user's home before connecting assistants. Host `CODEX_HOME` and `XDG_DATA_HOME` overrides are respected; container destinations use their own overrides. Writes are atomic, with directory mode 0700 and file mode 0600; credentials travel through stdin and are never included in image layers or logs. Codex uses file-backed credentials in the container. Missing host auth files leave existing container credentials intact and produce a warning; keyring-only host logins require a file-backed login first. Unreadable or invalid credentials and transfer failures stop the connection and are reported in the application log.

Assistant launches forward present `OPENAI_API_KEY`, `OPENAI_BASE_URL`, `OPENAI_ORG_ID`, `OPENAI_PROJECT_ID`, `ANTHROPIC_API_KEY`, `ANTHROPIC_AUTH_TOKEN`, `ANTHROPIC_BASE_URL`, `GEMINI_API_KEY`, `GOOGLE_API_KEY`, `OPENCODE_API_KEY`, and OpenCode server username/password variables by name. Restart the editor to pick up changes to its inherited environment. Use **Reconnect** after refreshing host credentials. Session history, databases, configuration, and container token refreshes remain independent; no assistant data directories are shared or copied back to the host.

Dev Container failure details are written to the application log at **Warn** level. Its toolbar menu shows a short failure message and retains **Retry**; opening the menu does not repeat diagnostics. Configuration, start/rebuild/reconnect, and stop errors include operation context. Failure records include the Compose exit status and captured stdout/stderr, with long output bounded while retaining its tail. Each diagnostic line gets its own timestamped record. Sensitive values and build-command payloads are redacted while surrounding failure explanations and exit codes remain visible.

Logging defaults to **Info**. Change it easily for a launch:

```sh
focus-editor --log-level debug .
focus-editor --foreground --log-level=trace .
cargo run --release -- --log-level debug .
```

Supported levels are `trace`, `debug`, `info`, `warn`, `error`, and `off`. Both flag forms work before or after the workspace argument, and background launching forwards the chosen level. Invalid levels fail before the GUI launches.

- **Debug:** discrete editor/file/search/Git actions, terminal lifecycle and resizing, assistant controls, session and request activity. Rendering frames, text contents, prompts, answers, terminal input/output, credentials, and protocol payloads are excluded.
- **Info:** important application, workspace, assistant, Dev Container, and session lifecycle events; successful commits, deletion, and path changes.
- **Warn:** recoverable operation failures, configuration fallbacks, and connection recovery. **Error** records fatal application failure. **Trace** adds low-level event metadata.

Records contain a timestamp, level, process ID, and component. Foreground launches write to the terminal's stderr; background launches append to `$XDG_DATA_HOME/focus-editor/logs/launch.log`, normally `~/.local/share/focus-editor/logs/launch.log`.

For component filters, use `RUST_LOG`, for example `RUST_LOG=warn,focus_editor::git_ui=debug focus-editor .`. An explicit `--log-level` takes precedence over `RUST_LOG`. With neither set, application Info and dependency Warn are enabled. Log files retain the existing append behavior without rotation.

## Layout and appearance

- The left activity bar switches between **Explorer**, **Search**, and **Source Control**.
- The center contains file tabs, the editor, and Git diff previews.
- The bottom pane is an interactive Bash terminal, with a Restart action.
- **Dev Container** starts a configured Docker Compose service and changes the terminal, Git, and assistants to use it.
- The optional right pane contains Codex and/or OpenCode. Its header keeps status visible alongside horizontally scrollable session controls; **Usage / status** has its own section below.
- Drag the dividers to resize the sidebar, terminal, and assistant pane. Their sizes are saved.

The light theme uses blue navigation, teal Codex accents, colored file-type markers, vector icons, and illustrations in empty views. Source Control groups use amber, blue, green, and red. Text labels accompany status colors, and input cursors are thick and steady. The bottom status bar shows file and save feedback.

## Explorer and editing

Choose a workspace from the toolbar. **Single-click** a file in Explorer to open an italic **preview tab**. Clicking another file replaces that preview in the same position. **Double-click** the file in Explorer or its tab to keep it permanently open. Editing a preview also makes it permanent. Clicking a file already open permanently selects its existing tab.

Edit in the center and use **Save**, **Ctrl+S**, or **Edit → Save** to write changes to disk. Source lines stay unwrapped; use horizontal scrolling for long lines. Syntax coloring uses a lightweight built-in tokenizer rather than compiler-backed language services.

Tabs remain in one horizontally scrollable row. Use the **⌄** open-files menu to search and select any open tab. Close a tab with **×** or **Ctrl+W**; closing a modified file offers **Save**, **Discard**, and **Cancel**, including inactive tabs. Unsaved contents and Undo histories stay with their file while switching tabs. Search results and Source Control’s **Open file** action open permanent tabs.

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

Files are read from disk, and restored tabs are permanent, including the last preview. Unsaved edits, cursor positions, and scroll positions are not recovered.

## Source Control

Open Source Control to see branch/upstream information and saved changes. Status refreshes automatically while the view is open, and Refresh requests an immediate update. Git author configuration is required to commit.

| Group | Color | Meaning |
| --- | --- | --- |
| Unstaged | Amber | Tracked changes excluded from the next commit |
| Untracked | Blue | New files not yet staged |
| Staged | Green | Changes included in the next commit |
| Conflicts | Red | Files needing resolution before committing |

Select a file row to preview its aligned **Before / After** comparison in the center. Use **Stage file** or **Unstage file** for the selection, **Stage all** or **Unstage all** for a group, and **Open file** to edit it. A partially staged file appears in both applicable groups. Counts show additions and removals; renames, conflicts, and binary changes are identified where available.

With a file row focused, **Space** stages or unstages the whole file according to its group. It does not affect typing in the commit message or another panel. The shortcut is disabled while a Git operation is pending.

For eligible tracked text modifications, each hunk has a **Stage hunk** or **Unstage hunk** action. Both sides stay visible in the same position after the action. Staged hunks have a blue background and a **Staged** label; red/green text still identifies removed/added lines. The reverse action applies the original hunk in reverse, preserving other staged changes.

The comparison is a retained review snapshot. **Reload diff** loads the current changes; external file/index changes and whole-file operations require a reload before further hunk actions. New files, deletions, renames, mode changes, symlinks, conflicts, and binary changes use whole-file actions. An empty diff may mean the file is untracked or has no available text diff.

Enter a message below the file groups, then click **Commit staged changes**. Only staged content is committed; save editor edits before staging. Commits are disabled when nothing is staged, the message is empty, or conflicts remain. A successful commit clears the message; failures preserve it and report the error.

Returning to Explorer or Search shows the active editor buffer without reloading it. Returning to Source Control can show its retained diff. Selecting a file tab or closing the preview dismisses the diff.

## Shell keyboard input

Click the terminal surface to give it keyboard focus. **Tab** performs shell completion; **Shift+Tab**, **Escape**, arrows, Home/End, Insert/Delete, Page Up/Down, and F1–F12 go to the terminal. Ctrl and Alt combinations reach the shell, including **Ctrl+C** to interrupt, **Ctrl+R** to search history, and **Ctrl+W** to delete the previous word. Editor shortcuts are suspended while the terminal owns focus. Click the editor or Codex to return keyboard input there.

**Ctrl+V** remains clipboard paste. Pasted text uses bracketed paste when requested by the shell. Application-cursor mode is respected for navigation. The default shell remains Bash; running `zsh` inside the terminal supports completion and command-line editing. Shortcuts reserved by your Linux desktop remain controlled by the desktop.

## Assistants: Codex and OpenCode

Use **Assistants** in the main toolbar to enable or disable **Codex** and **OpenCode** independently. When both are enabled, use their tabs at the top of the right pane to switch. Each tab shows activity or **Needs you** for an approval/question. Conversations, drafts, selections, and scroll positions remain independent; work continues in an inactive tab.

Disabling an assistant stops its connection and running work and hides its tab. With both disabled, the right pane disappears and the editor gets its width back. Enabling an assistant starts it and selects it; the previous pane width is restored. **Stop** inside an enabled panel leaves that panel visible so you can use **Start** again.

Enabled assistants and the selected tab are saved. Initially, Codex is enabled and OpenCode is disabled. Workspace changes stop the old connections and resume the new workspace's sessions; drafts survive switching workspaces during the current editor run. Each assistant maintains its own conversations; nothing is copied between Codex and OpenCode.

`--no-codex` and `--no-opencode` skip automatic startup of their respective connections for the launch while keeping enabled panels available for manual Start. The existing `codex_app_server = false` setting still disables automatic Codex startup. Panel visibility is controlled by `codex_enabled` and `opencode_enabled`, or the toolbar menu.

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

## OpenCode

The native panel uses a private local `opencode serve` process and your existing OpenCode configuration and provider authentication. Configure providers using OpenCode's CLI first; Focus Editor does not change those credentials or permission rules. Missing executables, unavailable models, connection failures, and unsupported responses appear in the panel. The transport has been smoke-tested with OpenCode **1.19.5**.

- The compact session dropdown lists conversations in the current workspace. **+** creates a session, **↻** refreshes it, and **…** requests older sessions. The last selected workspace session is remembered; missing sessions fall back to a new session. **Load earlier messages** retrieves earlier history.
- **Send** or **Ctrl+Enter** submits the draft. Select the connected provider/model, supported **Thinking/variant**, and primary **Agent** beside Send. New choices prefer **Plan**, otherwise the first available primary agent. **Tab / Shift+Tab** cycles agents in the idle composer.
- Responses stream into the conversation. Reasoning and tool activity are expandable when supplied. **Interrupt** stops active work. Permission and question cards use OpenCode's own responses; approvals are never supplied automatically by the editor.
- After a successful completed Plan response, **Go** selects **Build** and sends exactly `Go`, preserving your draft, model, and variant. It is unavailable during work, pending questions/approvals, disconnected streams, missing Build agents, failed turns, or cancelled turns.
- **Usage / status** shows reported tokens, cost, and model context capacity. Where session totals are unavailable, counts are labeled for loaded messages. Account quotas unavailable from OpenCode are stated explicitly. Usage refreshes every minute and on request.

Switching tabs continues processing both backends. Stream reconnects reload authoritative session state without automatically resending prompts. Disabling OpenCode or closing the editor terminates its owned server and tool process group.

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
| Tab / Shift+Tab, Escape, navigation, F1–F12 | Terminal | Send to the shell |
| Ctrl+C / Ctrl+R / Ctrl+W | Terminal | Interrupt / history search / delete previous word |
| Ctrl+V | Terminal | Paste |
| Ctrl+Enter | Codex or OpenCode message box | Send |
| Tab / Shift+Tab | Idle assistant message box | Next / previous Codex mode or OpenCode agent |

## Source organization

The application uses one crate with focused feature module directories:

- `src/editor/mod.rs` owns application state and coordinates the window. Its sibling modules separate layout/sidebar routing, document lifecycle and preview state, tab rendering, clipboard and close-dialog actions, document path changes, text rendering, Explorer, inline creation, confirmed deletion, search/Find, shell rendering, and persistence.
- `src/codex_ui/mod.rs` owns Codex panel state and connection lifecycle. Its sibling modules separate the header, transcript, composer, catalog helpers, usage, outgoing requests, incoming responses/events, message updates, indexed reasoning sections, and completed-plan/Go state.
- `src/git_ui/mod.rs` owns Source Control state. Its sibling modules separate repository access/status parsing, background actions and replies, panel rendering, comparison rendering (`diff`, `diff_rows`), byte-preserving patch parsing (`patch_model`), retained reviews (`review`), file/index validation (`fingerprint`), hunk application (`patch`), and presentation helpers. Git tests live in `src/git_ui/tests.rs` and `src/git_ui/tests/`.
- `src/opencode_ui/` separates OpenCode lifecycle, catalogs, sessions, events, approvals, composer, conversation, and usage. `src/opencode_server/` owns background HTTP/SSE workers and server cleanup. `src/editor/assistants.rs` coordinates the shared pane and independent workspace states.
- Editor tests are grouped by feature under `src/editor/tests/`. Codex tests cover panel layout, selection helpers, history, and event routing in `src/codex_ui/tests.rs`.

Other top-level modules provide the shared theme/icons, settings, terminal state and input encoding, syntax coloring, and the App Server transport. Panel methods preserve their widget scopes and IDs; implementation details remain private to their feature module.

See [PRODUCT.md](PRODUCT.md) for the current product scope and [PLAN.md](PLAN.md) for implementation and validation notes.
