# Product brief: focused Rust editor

## Goal
Build a fast, Linux-first desktop editor that replaces only the VS Code features I use: project file management, opening and editing source files, and two persistent shell work areas.

## Main window
- A clickable project tree on the left. Users can open files, create files/folders, rename, move, and delete entries.
- The center editor opens a file on double-click, supports editing and saving, and colors source text based on common file extensions.
- A shell panel across the bottom of the workspace and a shell panel on the right. Both are interactive terminal emulators connected to PTYs. The right panel defaults to `codex --resume`; its command is configurable.
- An optional directory argument at startup opens that directory in the explorer, for example `focus-editor ~/projects/my-app`.
- All panels can be resized, and their dimensions and positions are restored on the next launch.

## Platform and implementation
- Linux only for the initial release.
- Rust desktop application.
- Prefer native filesystem behavior and a real PTY-backed shell, not a terminal transcript mockup.

## Initial scope
Include common Rust, C/C++, Python, JavaScript/TypeScript, HTML/CSS, JSON, YAML, TOML, Markdown, shell, and SQL extensions. Keep the app focused: no extensions marketplace, debugger, or source-control UI in this first version.
