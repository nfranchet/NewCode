# UI follow-ups

Nord Light integrates the appearance of `ui-mocks/index.html` with existing editor
features. The following capabilities remain separate work:

- [ ] [#22](https://github.com/nfranchet/NewCode/issues/22) — Add Glacier and Linen presets using the shared theme contract, then a persisted theme selector. Addons should continue consuming host-supplied appearance without knowing preset names.
- [ ] [#23](https://github.com/nfranchet/NewCode/issues/23) — Add file breadcrumbs and symbol navigation above the editor.
- [ ] [#24](https://github.com/nfranchet/NewCode/issues/24) — Show Git decorations such as modified/untracked markers in Explorer.
- [ ] [#25](https://github.com/nfranchet/NewCode/issues/25) — Expand the status bar with branch/sync state, diagnostics, cursor position, indentation, encoding, and language information, backed by real application state.
- [ ] [#26](https://github.com/nfranchet/NewCode/issues/26) — Highlight the cursor's current line without obscuring selections or find matches.
- [ ] [#27](https://github.com/nfranchet/NewCode/issues/27) — Add a Settings activity entry and settings UI.
- [ ] [#28](https://github.com/nfranchet/NewCode/issues/28) — Support exact terminal colors from the active theme. `egui_tty` 0.2 exposes only Light/Dark emulator palettes; Nord Light currently uses its readable Light palette with themed panel chrome and cursor.

Native window decorations remain managed by the operating system; the mock's
traffic lights and browser showcase controls are not application controls.
