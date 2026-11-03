# Implementation plan

1. **Bootstrap the desktop app** — create a Rust/eframe project, Linux build notes, and persistent user settings.
2. **Project explorer and editor** — select a workspace, browse directories, implement file/folder create, rename, move, delete, and open/edit/save text files. Add extension-aware syntax coloring for common languages.
3. **Workspace search and editor navigation** — add Explorer and Search modes to the left sidebar. Search the selected workspace for a literal query, group matching lines by file, and show line numbers with previews. Clicking a result opens the file, scrolls to the match with a few preceding lines visible, and places the caret on the matching line. Skip directories and unreadable or non-text files, and report search status in the UI.
4. **Interactive terminals** — embed two PTY-backed terminal emulators with keyboard input and resizing; put the general shell at the bottom and default the right command to `codex resume`.
5. **Resizable layout and polish** — persist panel sizes and settings, add keyboard shortcuts and clear error reporting, and document build/run steps.

## Acceptance checklist
- [ ] Linux app launches into an empty state or restores the last workspace.
- [ ] Explorer operations update the filesystem and tree.
- [ ] Double-click opens a text file; edits can be saved.
- [ ] Explorer and Search modes switch within the left sidebar; Search mode replaces the tree with a query field and Search button.
- [ ] Searching the workspace shows matching lines grouped under each filename, with line numbers and previews; clicking a line opens the file at that line with preceding context visible and the caret on the matching line.
- [ ] Search skips directories and unreadable or non-text files and gives useful feedback when no matches are found or a search fails.
- [ ] Common extensions receive language-aware colors.
- [ ] Both terminal panels accept interactive keyboard input and show live output; right command is configurable.
- [ ] Passing a directory argument at launch opens it in the explorer.
- [ ] Layout and workspace survive restarting the app.
