# Focus Editor

A small Linux-first Rust editor for project browsing, editing source files, and working in two shell panels.

## Build and run

Install a recent Rust toolchain and the system libraries required by eframe/winit for your Linux distribution, then run:

```sh
cargo run --release
```

Open a directory directly in the explorer by passing it as an argument:

```sh
cargo run --release -- ~/projects/my-app
```

The first build downloads crates from crates.io. The app stores settings in the platform configuration directory (`~/.config/focus-editor/settings.toml` on most Linux desktops).

## Use

- Choose a workspace folder from the toolbar.
- Double-click a file in Explorer to open it. Edit in the center pane and save with **Ctrl+S** or the toolbar button.
- Use **New file** and **New folder**, or right-click entries to rename, move, or delete them.
- The bottom pane is an interactive Bash terminal. The right pane starts with `codex --resume`; edit its command and press **Run** to use another command.
- Drag the dividers to resize panes. The last workspace, command, and pane dimensions are saved with app settings.

Syntax colors are selected from common extensions including Rust, Python, JavaScript/TypeScript, JSON, TOML, HTML, CSS, shell, SQL, and Markdown. This initial implementation uses a lightweight built-in tokenizer rather than full compiler-grade parsing.

See [PRODUCT.md](PRODUCT.md) for the product brief and [PLAN.md](PLAN.md) for the implementation stages.
