# Focus Editor

A small Linux-first Rust editor for project browsing, editing source files, and working with a shell and Codex.

## Build and run

Install a recent Rust toolchain and the system libraries required by eframe/winit for your Linux distribution, then run:

```sh
cargo run --release
```

Open a directory directly in the explorer by passing it as an argument:

```sh
cargo run --release -- ~/projects/my-app
```

Pass `--no-codex` to skip starting the Codex App Server for that launch:

```sh
cargo run --release -- --no-codex ~/projects/my-app
```

The first build downloads crates from crates.io. The app stores settings in the platform configuration directory (`~/.config/focus-editor/settings.toml` on most Linux desktops).

## Use

- Choose a workspace folder from the toolbar.
- Double-click a file in Explorer to open it. Edit in the center pane and save with **Ctrl+S** or **Edit → Save**.
- Editor lines do not wrap; use the horizontal scrollbar to view long lines.
- Close a file with the **×** on its tab or press **Ctrl+W**. If the file has unsaved changes, choose **Save**, **Discard**, or **Cancel**.
- Open file tabs stay on one horizontally scrollable row. Use the **⌄** open-files menu to search and jump directly to any open file; its tab scrolls into view when selected.
- Use **Search** in the left sidebar to search file contents in the current workspace. Results are grouped by file; click a matching line to open the file at that line with nearby context visible.
- Open **Source Control (⑂)** in the left sidebar to see the branch and upstream status, staged and unstaged files, per-file additions/deletions, renames, conflicts, and binary changes. Use **Diff ±** to preview tracked changes or **Open** to edit a file. Status refreshes automatically while this view is open.
- Use **Stage**/**Unstage** per file or for each group, enter a commit message, then click **Commit staged changes**. Save edits before staging; commits include only staged changes. Git must be installed and your Git author configured.
- Press **Ctrl+F** to find text in the active file. Press **Enter** to jump to the first match with nearby preceding lines visible, then use **Enter** or the arrow buttons to move through matches; **Shift+Enter** goes to the previous match. All references are highlighted, and matching is case-insensitive.
- Select editor text and use **Ctrl+C**/**Ctrl+V**, or the **Copy** and **Paste** toolbar buttons, to use the system clipboard.
- Use the file and folder icon buttons beside the Explorer title to create entries, or right-click entries to rename, move, or delete them.
- Use the **Edit** menu for Save, Copy, Paste, Find in File, and Select All; the usual keyboard shortcuts work as well.
- The bottom pane is an interactive Bash terminal. The right pane has **Codex** and **Terminal** tabs. Codex uses `codex app-server` to send messages, stream replies, and show approval or input requests. The server starts by default; uncheck **Start Codex server** or pass `--no-codex` to skip it. The Terminal tab remains a configurable shell command.
- Drag the dividers to resize panes. The last workspace, command, and pane dimensions are saved with app settings.

Syntax colors are selected from common extensions including Rust, Python, JavaScript/TypeScript, JSON, TOML, HTML, CSS, shell, SQL, and Markdown. This initial implementation uses a lightweight built-in tokenizer rather than full compiler-grade parsing.

See [PRODUCT.md](PRODUCT.md) for the product brief and [PLAN.md](PLAN.md) for the implementation stages.
