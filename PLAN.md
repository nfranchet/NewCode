# Implementation plan

1. **Bootstrap the desktop app** — create a Rust/eframe project, Linux build notes, and persistent user settings.
2. **Project explorer and editor** — select a workspace, browse directories, implement file/folder create, rename, move, delete, and open/edit/save text files. Add extension-aware syntax coloring for common languages.
3. **Interactive terminals** — embed two PTY-backed terminal emulators with keyboard input and resizing; put the general shell at the bottom and default the right command to `codex --resume`.
4. **Resizable layout and polish** — persist panel sizes and settings, add keyboard shortcuts and clear error reporting, and document build/run steps.

## Acceptance checklist
- [ ] Linux app launches into an empty state or restores the last workspace.
- [ ] Explorer operations update the filesystem and tree.
- [ ] Double-click opens a text file; edits can be saved.
- [ ] Common extensions receive language-aware colors.
- [ ] Both terminal panels accept interactive keyboard input and show live output; right command is configurable.
- [ ] Passing a directory argument at launch opens it in the explorer.
- [ ] Layout and workspace survive restarting the app.
