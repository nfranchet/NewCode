# UI follow-ups

Nord Light integrates the appearance of `ui-mocks/index.html` with existing editor
features. The following capabilities remain separate work:

- [ ] Add Glacier and Linen presets using the shared theme contract, then a persisted theme selector. Addons should continue consuming host-supplied appearance without knowing preset names.
- [ ] Add file breadcrumbs and symbol navigation above the editor.
- [ ] Show Git decorations such as modified/untracked markers in Explorer.
- [ ] Expand the status bar with branch/sync state, diagnostics, cursor position, indentation, encoding, and language information, backed by real application state.
- [ ] Highlight the cursor's current line without obscuring selections or find matches.
- [ ] Add a Settings activity entry and settings UI.
- [ ] Support exact terminal colors from the active theme. `egui_tty` 0.2 exposes only Light/Dark emulator palettes; Nord Light currently uses its readable Light palette with themed panel chrome and cursor.

Native window decorations remain managed by the operating system; the mock's
traffic lights and browser showcase controls are not application controls.
