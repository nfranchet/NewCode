# Product brief: focused Rust editor

## Goal
Build a fast, Linux-first desktop editor that replaces only the VS Code features I use: project file management, opening and editing source files, and two persistent shell work areas.

## Main window
- A left sidebar with **Explorer** and **Search** modes. Explorer shows a clickable project tree; users can open files, create files/folders, rename, move, and delete entries. Search replaces the tree with a text field and a Search button.
- Project search scans files in the current workspace and groups matches by filename. Each matching line appears below its filename; clicking a line opens that file in the center editor, scrolls to the line with nearby preceding context visible, and places the text cursor on that line.
- The editor has an in-file Find bar, opened with **Ctrl+F**, with previous/next match navigation and a match counter. Search is case-insensitive, highlights all references, and pressing Enter jumps to the first reference with preceding context visible, matching workspace search navigation.
- Text selections in the editor can be copied and pasted with **Ctrl+C** and **Ctrl+V**, or with the Copy and Paste toolbar actions.
- The center editor opens files on double-click, supports editing and saving, keeps long lines unwrapped with horizontal scrolling, and colors source text based on common file extensions. Open file tabs have a close button, and **Ctrl+W** closes the active file. Closing a modified file asks whether to save, discard, or cancel.
- Open files use a single horizontally scrollable tab row. A searchable open-files menu provides direct access to any tab, including those outside the visible row, and brings the selected tab into view.
- A shell panel across the bottom of the workspace and a shell panel on the right. Both are interactive terminal emulators connected to PTYs. The right panel defaults to `codex --resume`; its command is configurable.
- An optional directory argument at startup opens that directory in the explorer, for example `focus-editor ~/projects/my-app`.
- All panels can be resized, and their dimensions and positions are restored on the next launch.

## Platform and implementation
- Linux only for the initial release.
- Rust desktop application.
- Prefer native filesystem behavior and a real PTY-backed shell, not a terminal transcript mockup.

## Initial scope
Include common Rust, C/C++, Python, JavaScript/TypeScript, HTML/CSS, JSON, YAML, TOML, Markdown, shell, and SQL extensions. Keep the app focused: no extensions marketplace, debugger, or source-control UI in this first version.

Project search is limited to readable text files under the selected workspace. Search results show one entry per matching line, grouped by file, with line numbers and a short line preview. Directories and files that cannot be read as text are skipped; search reports errors or skipped files through the status area where practical.
