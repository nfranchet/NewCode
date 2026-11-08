# Focus Editor

A compact Rust editor for Linux and Apple Silicon macOS. Browse projects, edit source files, review Git changes, and work with a shell, Codex, and OpenCode.

Licensed under the [Apache License 2.0](LICENSE).

## Release downloads

Publishing a GitHub release (including a prerelease) builds the tagged source and attaches:

- `focus-editor-<tag>-linux-amd64.tar.gz`
- `focus-editor-<tag>-linux-arm64.tar.gz`
- `focus-addon-<id>-<tag>-linux-{amd64,arm64}.tar.gz` for every addon in the tagged source
- `addons.json` (the manager catalog) and per-platform catalogs
- `SHA256SUMS`

Linux binaries are built on Ubuntu 24.04 and require glibc 2.39 or newer, an X11 or Wayland desktop, and working OpenGL/EGL libraries. They are dynamically linked; run `ldd focus-editor` to identify missing runtime libraries. A desktop portal provides native file dialogs. Extract the archive and install the executable with `install -m 755 focus-editor ~/.local/bin/focus-editor` after creating that directory.

Open **Addons** in the activity bar to install individual addons from the release catalog. The editor archives also include addon executables: run `python3 install-addons.py` from the extracted archive to install all of them, or add `--addon github` to select one (Python 3.11 or newer). Use **Rescan installed** in the manager afterward. The installer preserves addon data and installs executables beside logs.

Automatic releases currently publish Linux binaries only. macOS arm64 builds and DMGs are skipped. macOS remains available to build from source; its app packaging script remains available for manual use.

Finder launches retain the inherited PATH and add `~/.local/bin`, `~/.opencode/bin`, `/opt/homebrew/bin`, and `/usr/local/bin` for external tools. Git, Docker, Codex, and OpenCode are separate installations; authenticate assistants with their CLI first. Run `focus-editor --version` (or the app's executable path with `--version`) to check the version without opening a window.

To publish: merge the workflow and packaging scripts into the default branch, bump the Cargo package version as appropriate, create a tag containing those changes, then publish a release for that tag. Saving a draft or pushing a tag alone does not build downloads. Both Linux native builds and packaging checks must pass before any assets are uploaded. Rerun failed jobs from GitHub Actions to retry; matching assets are replaced. Release tags containing filename-unsafe characters such as `/` use `-` in download names. The bundle and executable version come from Cargo metadata; release filenames use the tag.

The workflow needs no personal access token: its final upload job uses GitHub's built-in token with `contents: write`. Native Linux amd64 and arm64 builds test the editor, SDK, and every addon discovered through `addons/*/addon.toml`, validate architectures and runtime dependencies, and verify package contents. Every editor/addon archive and both platform catalogs must be present and consistent before the combined catalog and checksums are generated and assets are uploaded. Builds are not triggered until a release is published. Branch pushes run the same shared checks.

## Build and run

Install Rust 1.95 or newer and the system libraries required by eframe/winit for your Linux distribution, or Xcode Command Line Tools on macOS. Git must be installed for Source Control. The Codex panel requires an installed, authenticated `codex` executable on your `PATH`. OpenCode requires an installed `opencode` executable with a configured provider; it is disabled by default.

The terminal uses the existing `egui_tty` widget and Ghostty VT engine. Building requires **Zig 0.15.2** on `PATH`; the development container and release workflow install it automatically. To install it into a dedicated build directory:

```sh
python3 scripts/install-zig.py /tmp/focus-zig
export PATH="/tmp/focus-zig:$PATH"
cargo run --release
```

The first build downloads the pinned Ghostty source and Zig packages. Subsequent builds reuse Cargo's build directory. For a network-free build, prefetch the pinned source and Zig packages and supply `GHOSTTY_SOURCE_DIR` and `GHOSTTY_ZIG_SYSTEM_DIR` as described by `libghostty-vt-sys`. `cargo --offline` alone does not prevent its native build script from fetching missing sources. Release binaries link the VT engine statically; users do not need Zig or the Ghostty application installed.

On Linux, the editor opens in the background and immediately releases the terminal. macOS uses the normal application lifecycle. Each launch opens a separate instance. Its window title is **Focus Editor — workspace-name**, identifying the current workspace in Linux window lists that show window titles. Relative workspace paths are resolved for display: launching with `.` shows the actual folder name in the title and toolbar, with the full resolved path in the toolbar tooltip.

Keep the editor attached to the terminal for debugging:

```sh
cargo run --release -- --foreground
# Or, for an installed executable:
focus-editor --foreground
```

Each launch starts a fresh diagnostic file at `$XDG_DATA_HOME/focus-editor/logs/launch.log`, usually `~/.local/share/focus-editor/logs/launch.log`. Failures to create the log or spawn the editor are reported in the launching terminal. Errors after detachment are recorded in the log.

Open a workspace directly:

```sh
cargo run --release -- ~/projects/my-app
```

## Addons

Addons compile independently from the editor and run as separate executables. The main Rust process renders their native panels. Open **Addons**, the activity-bar entry immediately below Source Control, or use **Addons → Manage addons…**. **Installed** lists local addons and their status; **Available** lists the official release packages. Search either list and select an addon to see its details in the center. Enable/disable, restart, credentials, and uninstall controls are in those details.

Choose **Refresh / check updates** to load the catalog from `nfranchet/NewCode`. Release builds default to their own release tag; source builds default to `latest`. You can enter an explicit tag, including a prerelease. Installation requires matching editor version, addon protocol, and platform. Older releases without `addons.json` cannot supply a catalog; use a new release or a local package. Catalog requests are anonymous and do not use stored addon credentials.

Installation and updates are manual. **Install local addon…** accepts a trusted `addon.toml` or standalone addon `.tar.gz`. Downloads, checksum checks, extraction, and replacement run in the background. Failed replacements restore the previous installation; an interrupted replacement is recovered on discovery. Updating restarts only the affected addon, and another editor instance using it prevents replacement. **Rescan installed** finds packages installed outside the editor. Installation does not load code from the open repository.

Installed addons live beside the logs directory:

```text
~/.local/share/focus-editor/
├── logs/
├── addons/github/       # executables and assets
├── addon-data/github/   # non-secret persistent state
└── addon-receipts/      # installed source/release metadata
```

Linux respects `XDG_DATA_HOME`; macOS uses `~/Library/Application Support/focus-editor/`. Legacy `addons/<id>/data` is moved to `addon-data/<id>` without overwriting existing data. Updates keep data and OS-keyring credentials. Uninstall keeps both by default; select **Also remove saved addon data** to remove non-secret state. Use **Credentials → Forget** before uninstalling to delete a stored token.

The editor loads installed addons only from this shared application data root. Build and install GitHub from source separately:

```sh
python3 scripts/addons.py --release --install --addon github
cargo run --release -- .
```

Omit `--addon` to build/install every discovered addon; repeat it to select several. Run `python3 scripts/addons.py --test` to format-check and test the SDK and addon crates. For an isolated Linux development installation, use `--data-root "$PWD/target/dev-data/focus-editor"` and launch the editor with `XDG_DATA_HOME="$PWD/target/dev-data"`. A plain `cargo build` builds the editor and SDK, without compiling addon executables. See the [addon SDK contract](crates/addon-sdk/README.md) for manifests, packaging, and development.

Codex and OpenCode are optional, independently compiled addons. They occupy the existing right-hand dock. Install either one or both, and use the **Enabled** control in the Addons manager to choose which run. When both are enabled, status-bearing tabs select the visible panel while both continue processing events. Disabling every right-dock addon gives the width back to the editor. A fresh installation enables Codex by default and leaves OpenCode disabled; existing preferences and remembered sessions migrate from the old settings.

To install only selected addons:

```sh
python3 scripts/addons.py --release --addon codex --install
python3 scripts/addons.py --release --addon opencode --install
# From an extracted release:
python3 install-addons.py --addon codex --addon opencode
```

All addon installations share the directory beside logs. Omitted addons are not required by the editor. The SDK's `panel = "right"` capability is available to any addon; simple panels use native declarative widgets, and richer panels can use the optional shared surface runner. See [the UI addon helper](crates/addon-support/README.md).

The GitHub icon opens **Issues** and **Pull requests** for the active workspace's GitHub.com remotes, preferring `origin`. Select a repository, filter loaded items, load more results, or open Markdown details. The panel provides:

- **Create issue branch & checkout** (`gcmr`): links an `issue-N` branch to its issue, with editable base/name and explicit existing-branch reuse.
- **Push & create PR** (`gpc`): previews editable title/body and the destination before pushing; associates known issue branches with `Closes #N` and recognizes existing PRs.
- **Auto-merge (squash)** (`gpma`): respects checks/protections and merge queues. **Delete branches after merge** is unchecked by default. Selected cleanup waits for a confirmed merge, preserves new commits and occupied worktrees, and resumes when the addon runs again. GitHub's own repository deletion setting still applies.

GitHub uses installed `gh` and its existing login. If `gh` is absent it uses GitHub's HTTPS REST/GraphQL APIs. New personal access tokens are entered through the native credential dialog and stored only in Linux Secret Service or macOS Keychain. A locked/unavailable keyring produces an error instead of a plaintext credential file. Credentials stay on the host, including when Git commands run in a Dev Container. Existing SSH agents and Git credential helpers remain supported. Anonymous API reads can work for public repositories; private reads and writes require appropriate access.

Git mutations share Source Control's operation lock. Checkout prompts to save unsaved editor buffers and refuses worktree conflicts. Addon diagnostic messages appear under **Logs → Addons**, with protocol payloads and credentials excluded.

PR creation uses the selected repository's configured remote. If that remote has a different push repository, configure/select a remote with matching fetch and push destinations before using this workflow.

## Dev Containers

Focus Editor can use a Docker Compose development container without Node.js or the Dev Container CLI. With a workspace open, use **Dev Container** in the toolbar to start the service defined by `.devcontainer/devcontainer.json`. The container stays running when the editor closes; use **Stop** or **Rebuild** from the same menu when needed.

The built-in backend supports Compose-based configurations with `dockerComposeFile`, `service`, `workspaceFolder`, and optional `remoteUser`. It starts the service with Docker Compose, then runs the integrated shell, Source Control commands, Codex, and OpenCode with `docker exec` in that container. Explorer, search, editing, and saved files stay on the host bind mount.

The launcher resolves build settings relative to the first Compose file's directory, matching Compose's normal build-context rules. For example, `.devcontainer/docker-compose.yml` with `context: .` and `dockerfile: ./vscode/Dockerfile` builds from `.devcontainer`, so `COPY ./vscode/files/toto.txt` reads `.devcontainer/vscode/files/toto.txt`. Start and Rebuild supply these resolved build settings through an in-memory override for every buildable service. The runtime project identity and workspace bind mounts retain their existing workspace-root resolution. No configuration, Dockerfile, or source file is rewritten, and Docker reports missing build files instead of the launcher guessing a replacement.

Docker Engine and the Docker Compose plugin are required. Node.js and `@devcontainers/cli` are not required or installed. The bundled image installs Codex and OpenCode using their official curl installers as `vscode`, with both commands available to noninteractive `docker exec`. Custom images must provide the tools you enable. Dev Container Features and lifecycle commands are not interpreted by Focus Editor; express required setup in the Dockerfile or Compose service instead.

Temporarily disable an addon for one launch:

```sh
cargo run --release -- --no-addon codex ~/projects/my-app
```

The first build downloads crates from crates.io. Settings are stored in the platform configuration directory, usually `~/.config/focus-editor/settings.toml`. Addon enablement is stored under `[addon_enabled]`; use the Addons manager to change it. The editor stores right-dock selection and width generically. Each assistant addon retains its own Start/Stop controls and non-secret session IDs under `addon-data/<id>/`.

The bundled Rust Dev Container image updates the stable Rust toolchain during its build, installs matching `llvm-tools-preview`, and installs `grcov` with its published lockfile. Use **Dev Container → Rebuild** after changing the Dockerfile. The image also runs the official installers at `https://chatgpt.com/codex/install.sh` and `https://opencode.ai/install`; rebuilding selects their current default releases.

## Logging

When an enabled assistant addon connects inside a Dev Container, that addon copies only its own host `auth.json` into the configured container user's home before starting its tool. Host `CODEX_HOME` and `XDG_DATA_HOME` overrides are respected; container destinations use their own overrides. Writes are atomic, with directory mode 0700 and file mode 0600; credentials travel through stdin and are never included in image layers or logs. Codex uses file-backed credentials in the container. Missing host auth files leave existing container credentials intact and produce a warning; keyring-only host logins require a file-backed login first. Unreadable or invalid credentials and transfer failures stop the connection and are reported in the application log.

Each addon declares the environment variables it needs in its manifest. The host forwards only those declared variables; the assistant addons support `OPENAI_API_KEY`, `OPENAI_BASE_URL`, `OPENAI_ORG_ID`, `OPENAI_PROJECT_ID`, `ANTHROPIC_API_KEY`, `ANTHROPIC_AUTH_TOKEN`, `ANTHROPIC_BASE_URL`, `GEMINI_API_KEY`, `GOOGLE_API_KEY`, `OPENCODE_API_KEY`, and OpenCode server username/password variables by name. Restart the editor to pick up changes to its inherited environment. Use **Reconnect** after refreshing host credentials. Session history, databases, configuration, and container token refreshes remain independent; no assistant data directories are shared or copied back to the host.

Dev Container failure details are written to the application log at **Warn** level. Its toolbar menu shows a short failure message and retains **Retry**; opening the menu does not repeat diagnostics. Configuration, start/rebuild/reconnect, and stop errors include operation context. Failure records include the Compose exit status and captured stdout/stderr, with long output bounded while retaining its tail. Each diagnostic line gets its own timestamped record. Sensitive values and build-command payloads are redacted while surrounding failure explanations and exit codes remain visible.

The bottom panel has **Terminal** and **Logs** tabs. Switching to Logs leaves the terminal running and processing output, while keyboard input belongs to the log controls. Select **All**, **Editor**, **Git**, **Dev Container**, **Docker Compose**, **Terminal**, or **Addons** to view a component. Use search and severity filters, select/copy text, **Copy visible logs**, **Clear**, and **Follow latest**. Scrolling upward pauses following. **Dev Container → Show logs** opens the viewer with that source selected.

Logs are current-session only: every editor launch starts an empty viewer and replaces `launch.log`; no previous records are loaded and no rotated history is kept. Each running editor's viewer is independent. The file path represents the newest launch; older running instances keep their own output handle. The viewer retains up to 10,000 records or 8 MiB and discards oldest records when full. **Clear** removes records for the selected source (or all sources) from the viewer; it leaves the current-session diagnostic file intact. Viewer filters do not change which levels are captured.

Docker Compose build/start/stop output streams live into its section, including successful progress. Codex and OpenCode process diagnostics appear in their sections; terminal input/output and OpenCode protocol payloads are excluded. At Debug or Trace level, the Addons section also includes incoming and outgoing app-server JSON, including message content, request IDs, methods, and numbered payload chunks. Credentials are redacted before either log sink.

Logging defaults to **Info**. Change it easily for a launch:

```sh
focus-editor --log-level debug .
focus-editor --foreground --log-level=trace .
cargo run --release -- --log-level debug .
```

Supported levels are `trace`, `debug`, `info`, `warn`, `error`, and `off`. Both flag forms work before or after the workspace argument, and background launching forwards the chosen level. Invalid levels fail before the GUI launches.

- **Debug:** discrete editor/file/search/Git actions, terminal lifecycle and resizing, assistant controls, session and request activity. Codex app-server requests, responses, notifications, and approval/user-input replies include their redacted JSON payloads. Long payloads use numbered chunks rather than silently losing their bodies. Rendering frames, editor text contents, terminal input/output, credentials, and OpenCode protocol payloads are excluded.
- **Info:** important application, workspace, assistant, Dev Container, and session lifecycle events; successful commits, deletion, and path changes.
- **Warn:** recoverable operation failures, configuration fallbacks, and connection recovery. **Error** records fatal application failure. **Trace** adds low-level event metadata.

Records contain a timestamp, level, process ID, and component. Foreground launches write to both the terminal's stderr and the current-session file; background launches write to `$XDG_DATA_HOME/focus-editor/logs/launch.log`, normally `~/.local/share/focus-editor/logs/launch.log`.

For component filters, use `RUST_LOG`, for example `RUST_LOG=warn,focus_editor::git_ui=debug focus-editor .`. An explicit `--log-level` takes precedence over `RUST_LOG`. With neither set, application Info and dependency Warn are enabled. Only the current launch is retained; the next launch replaces the file without creating history or rotated backups.

## Layout and appearance

- The left activity bar switches between **Explorer**, **Search**, and **Source Control**.
- The center contains file tabs, the editor, and Git diff previews.
- The bottom of the center column contains an interactive terminal and Logs, with a Restart action.
- **Dev Container** starts a configured Docker Compose service and changes the terminal, Git, and assistants to use it.
- The optional right pane contains Codex and/or OpenCode. Its header keeps status visible alongside horizontally scrollable session controls; **Usage / status** has its own section below.
- Drag the dividers to resize the sidebar, terminal, and assistant pane. Their sizes are saved.

Nord Light follows the Nord concept in `ui-mocks/index.html`: cool gray panels, a light activity rail, muted blue accents, square corners, and compact controls. Source Control keeps semantic status colors and text labels. The accent-colored bottom status bar shows file and save feedback. Bundled addon panels receive the editor’s resolved theme automatically, including custom widget colors and spacing. The terminal uses the widget’s supported light palette; native window decorations follow the operating system. Saved panel sizes are retained. See [UI follow-ups](TODO.md) for additional mockup features and future theme selection.

## Explorer and editing

Choose a workspace from the toolbar. **Single-click** a file in Explorer to open an italic **preview tab**. Clicking another file replaces that preview in the same position. **Double-click** the file in Explorer or its tab to keep it permanently open. Editing a preview also makes it permanent. Clicking a file already open permanently selects its existing tab.

Edit in the center and use **Save**, **Ctrl+S**, or **Edit → Save** to write changes to disk. Source lines stay unwrapped; use horizontal scrolling for long lines. Syntax coloring uses a lightweight built-in tokenizer rather than compiler-backed language services.

Tabs remain in one horizontally scrollable row. Use the **⌄** open-files menu to search and select any open tab. Right-click a tab title for **Close file**, **Close others**, or **Close all**. Close others keeps the right-clicked tab. You can also close a tab with **×** or **Ctrl+W**; closing a modified file offers **Save**, **Discard**, and **Cancel**, including inactive tabs. Bulk closing asks for each modified file in tab order; Cancel stops the remaining closes, and failed saves leave the file and question open. Unsaved contents and Undo histories stay with their file while switching tabs. Search results and Source Control’s **Open file** action open permanent tabs.

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

The Source Control sidebar has one repository/branch header and a Refresh icon. The commit message and **Commit staged** button stay above the scrolling file list. Status refreshes automatically every three seconds while the panel is open; background reads keep controls enabled and preserve focus, selection, group expansion, and scrolling.

| Group | Contents | Actions |
| --- | --- | --- |
| Conflicts | Files requiring resolution | Resolve in the editor, then stage the resolved file |
| Staged Changes | Content included in the next commit | Unstage through the row context menu, Space, or the group **−** button |
| Changes | Tracked changes excluded from the next commit | Stage through the row context menu, Space, or the group **+** button |
| Untracked | New files not yet added to Git | Stage through double-click, the context menu, Space, or the group **+** button |

Only nonempty groups are shown. A clean repository shows **Working tree clean**. Each compact row has four columns: **status letter**, **repository-relative path**, **+added lines**, and **-deleted lines**. Both line counts are green and aligned on the right. Status letters are M (modified), A (added), D (deleted), R (renamed), C (copied), T (type changed), U (conflict), and N (new/untracked). Hover for the full path, status description, rename source, and complete counts. A partially staged file appears in both Staged Changes and Changes with the counts for each side.

New text files show all their lines as additions and zero deletions, including a final line without a newline. Counts are calculated in the background. Binary, unreadable, and unsupported files show **+— -—**, with an explanatory tooltip.

Select a file to view its aligned **Before / After** comparison in the center. The numeric **+ / -** columns are counts; clicking them selects the file. Double-click a row or press **Space** on the focused row to stage/unstage it, or right-click for **Open file**, **Stage file**, or **Unstage file**. Group buttons apply to that group; conflicts have no bulk-stage button. Untracked files and unsupported text comparisons explain which actions are available.

For eligible tracked text modifications, each hunk has a **Stage hunk** or **Unstage hunk** action. Both sides remain visible in place after the action. Staged hunks have a blue background and a **Staged** label; red/green text still identifies removed/added lines. Reversing a staged hunk applies the original patch in reverse and preserves unrelated staged content.

The comparison is a retained review snapshot. **Reload diff** loads current changes. External file/index changes and whole-file operations mark the review stale before further hunk actions. New files, deletions, renames, mode changes, symbolic links, conflicts, and binary changes use whole-file actions. Loading or failing to load a comparison keeps the center pane in review mode; it does not briefly expose the editor. Reloading the same comparison retains its scroll position.

Enter a message at the top and click **Commit staged**. Only staged content is committed; save editor edits before staging. The panel explains when committing is unavailable: missing message, no staged files, conflicts, or a Git operation in progress. Git author configuration is required. Success clears the submitted message, while preserving any new draft typed during the commit. Failure preserves the draft and displays a short notice; **…** opens selectable Git error details.

Returning to Explorer or Search shows the active editor buffer without reloading it. Returning to Source Control can show its retained diff. Selecting a file tab or closing the comparison dismisses the review.

## Terminal

The bottom panel embeds `egui_tty`, backed by Ghostty’s VT engine, with 10,000 lines of scrollback. Drag to select text, double-click for a word, and triple-click for a line. The mouse wheel scrolls history or goes to a full-screen program when it requests mouse reporting. Resize the panel normally; full-screen programs use their own alternate screen and restore the shell on exit.

The terminal uses an installed monospace font with Nerd/symbol fallbacks, independently of other panels. Prompt symbols such as `✗` use the installed font coverage.

Click the terminal surface to give it keyboard focus. **Tab** performs shell completion; **Shift+Tab**, **Escape**, arrows, Home/End, Insert/Delete, Page Up/Down, and F1–F12 go to the terminal. Ctrl and Alt combinations reach the shell, including **Ctrl+C** to interrupt, **Ctrl+R** to search history, and **Ctrl+W** to delete the previous word. Editor shortcuts are suspended while the terminal owns focus. Click the editor or Codex to return keyboard input there.

**Ctrl+C** interrupts even with a selection. **Ctrl+Shift+C** copies the terminal selection; **Ctrl+V** and **Ctrl+Shift+V** paste. On macOS, Command+C/V use the clipboard. Pasted text uses bracketed paste when requested by the shell. Application-cursor mode is respected for navigation. The terminal starts your configured shell (`$SHELL`, then your account shell, with `/bin/sh` as a fallback) interactively and loads its normal startup configuration, prompt, aliases, history, and completion. Dev Container terminals resolve the shell inside the container for the selected user. Shortcuts reserved by your Linux desktop remain controlled by the desktop. Restart is the icon at the right of the Terminal/Logs title row. Logs keeps the shell running without capturing its keys. Shell exit preserves the screen with an exit notice; startup failures stay visible and can be retried with Restart. Restart, workspace/container changes, and application shutdown close the old terminal session without stopping the container.

## Assistant addons: Codex and OpenCode

Use **Addons** in the main toolbar to enable or disable **Codex** and **OpenCode** independently. When both are enabled, use their tabs at the top of the right pane to switch. Each tab shows activity or **Waiting for you** for an approval/question. Conversations, drafts, selections, and scroll positions remain independent; work continues in an inactive tab.

Disabling an assistant stops its connection and running work and hides its tab. With both disabled, the right pane disappears and the editor gets its width back. Enabling an assistant starts its addon and restores the pane width; use its tab to select it. **Stop** inside an enabled panel leaves that panel visible so you can use **Start** again.

Enabled assistants and the selected tab are saved. Initially, Codex is enabled and OpenCode is disabled. Workspace changes stop the old connections and resume the new workspace's sessions; drafts survive switching workspaces during the current editor run. Each assistant maintains its own conversations; nothing is copied between Codex and OpenCode. Both use the same Codex-style header, session controls, usage section, message cards, approval styling, and composer. Provider-specific models, thinking choices, modes/agents, and usage values remain independent.

The composer stays below the scrolling conversation. Its input grows from three rows to at most 120px (less in short panels), then scrolls vertically; Send, Interrupt, and selectors remain outside that scroll area. Drafts and cursor focus survive streaming and resizing.

Assistant replies, proposed plans, and supplied reasoning render as Markdown, including headings, lists, links, tables, inline code, and fenced blocks such as `shell`, `bash`, Rust, and JSON. Code is selectable and has a copy control. Wide content stays inside a horizontal scroll area; unknown code languages retain a monospaced fallback. User messages and raw tool output stay literal.

`--no-addon codex` and `--no-addon opencode` suppress an addon for one launch without changing saved enablement. Repeat the option to suppress multiple addons. Use **Addons** in the toolbar for persistent visibility and lifecycle controls.

## Codex addon

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

Replies appear as message cards. Commands and file activity are expandable. Readable reasoning starts expanded and can be collapsed; your choice remains in place as text streams and completes. New turns request automatic reasoning summaries. The panel prefers a supplied summary and falls back to readable reasoning content supplied by Codex, preserving section boundaries and streamed text in live and restored conversations. An empty running reasoning item shows **Thinking…**; empty completed items are hidden. Reasoning cannot be displayed when Codex supplies neither a summary nor readable content.

Proposed plans are displayed as expanded cards and restored with session history. Completion appears in the conversation as **Plan ready**, **Response complete**, or a failure/interruption message, in addition to the header status.

When the latest plan has completed successfully and Plan mode is selected, **Go** appears beneath that plan. Click it to switch to the available Default mode and send exactly **Go** in the same session. The model, thinking level, and unsent draft are preserved. The button is disabled while busy, disconnected, awaiting your response, or when Default mode is unavailable. A failed submission restores the plan action for an explicit retry; it never resends automatically. The latest completed plan can also offer Go when reopening a saved session. Normal approval prompts still apply.

Approval requests provide Allow, Allow this session, and Decline actions. Input requests provide a response field. Codex waits for your response before continuing where required.

### Usage

Expand **Usage / status** for the session ID, model, account limits, session tokens, and context usage when supplied by Codex. Account limits refresh every minute while connected; **Refresh usage** requests an immediate update. Unsupported or unavailable account usage is shown explicitly. Token/context figures update from session events.

## OpenCode addon

The native panel uses a private local `opencode serve` process and your existing OpenCode configuration and provider authentication. Configure providers using OpenCode's CLI first; Focus Editor does not change those credentials or permission rules. Missing executables, unavailable models, connection failures, and unsupported responses appear in the panel. The transport has been smoke-tested with OpenCode **1.19.5**.

- The compact session dropdown lists conversations in the current workspace. **+** creates a session, **↻** refreshes it, and **…** requests older sessions. The last selected workspace session is remembered; missing sessions fall back to a new session. **Load earlier messages** retrieves earlier history.
- **Send** or **Ctrl+Enter** submits the draft. Select the connected provider/model, supported **Thinking/variant**, and primary **Agent** beside Send. New choices prefer **Plan**, otherwise the first available primary agent. **Tab / Shift+Tab** cycles agents in the idle composer.
- Responses stream into the conversation. Supplied reasoning starts expanded and remains collapsible; tool activity is expandable. **Interrupt** stops active work. Permission and question cards use OpenCode's own responses; approvals are never supplied automatically by the editor.
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
| Ctrl+V / Ctrl+Shift+V | Terminal | Paste |
| Ctrl+Shift+C | Terminal | Copy terminal selection |
| Ctrl+Enter | Codex or OpenCode message box | Send |
| Tab / Shift+Tab | Idle assistant message box | Next / previous Codex mode or OpenCode agent |

## Source organization

The editor and executable addons compile as separate Cargo projects:

- `src/editor/mod.rs` owns application state and coordinates the window. Its sibling modules separate layout/sidebar routing, document lifecycle and preview state, tab rendering, clipboard and close-dialog actions, document path changes, text rendering, Explorer, inline creation, confirmed deletion, search/Find, shell rendering, and persistence.
- `crates/addon-support/src/assistant_ui/` shares assistant headers, cards, Markdown rendering/cache, and bounded scrolling composers; provider modules adapt their own catalogs and protocol actions to these controls.
- `addons/codex/src/codex_ui/mod.rs` owns Codex panel state and connection lifecycle. Its sibling modules separate the header, transcript, composer, catalog helpers, usage, outgoing requests, incoming responses/events, message updates, indexed reasoning sections, and completed-plan/Go state.
- `src/git_ui/mod.rs` owns Source Control state. Its sibling modules separate repository access/status parsing, numeric diff statistics and new-file line counting (`stats`), independent status/operation/review requests (`actions`), shared grouping and eligibility rules (`model`), fixed panel controls (`panel`), compact change rows (`file_list`), comparison rendering (`diff`, `diff_rows`), byte-preserving patch parsing (`patch_model`), retained reviews (`review`), file/index validation (`fingerprint`), hunk application (`patch`), and presentation helpers. Git tests live in `src/git_ui/tests.rs` and `src/git_ui/tests/`.
- `addons/opencode/src/opencode_ui/` separates OpenCode lifecycle, catalogs, sessions, events, approvals, composer, conversation, and usage. `addons/opencode/src/opencode_server/` owns background HTTP/SSE workers and server cleanup. `src/addons/` owns the generic docks, addon lifecycle, and UI surface input/rendering.
- Editor tests are grouped by feature under `src/editor/tests/`. Codex tests cover panel layout, selection helpers, history, and event routing in `addons/codex/src/codex_ui/tests.rs`.

Other top-level modules provide the shared theme/icons, settings, terminal process lifecycle and system font discovery, syntax coloring, and addon host services. Panel methods preserve their widget scopes and IDs; implementation details remain private to their feature module.

See [PRODUCT.md](PRODUCT.md) for the current product scope and [PLAN.md](PLAN.md) for implementation and validation notes.
