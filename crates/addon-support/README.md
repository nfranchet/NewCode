# Addon panels and UI surfaces

This optional helper provides shared UI controls, container utilities, and a
background event loop for executable addons. Provider clients live entirely in
their own addon projects.

Any addon can select its dock and rendering mode:

```toml
panel = "right"             # default: "left"
ui = "surface"              # default: "widgets"
enabled_by_default = false
```

Enabled right-dock addons share the existing resizable right pane, with tabs
when several are enabled. Hidden addons keep processing events. Disabling stops
the process; disabling all releases the width. Declarative right panels render
`View.sidebar` through the SDK's existing widget API.

For rich interfaces, implement `runner::Panel` and call `runner::run(panel)`.
`context` receives workspace/container metadata, private addon storage, and
migrated non-secret state. `tick` runs even when hidden, `show` renders visible
content, and `state` supplies the tab status. The addon owns per-workspace drafts,
session restore, provider connections, approvals, and cleanup.

Surface version 1 uses JSON-RPC `surface.input` and `surface.frame` notifications
with context generation and frame sequence. Initialization acknowledges SDK
protocol 1 and surface version 1. The host routes keyboard, pointer, clipboard,
and IME input to the focused surface. Frames contain clipped meshes and bounded,
compressed premultiplied RGBA texture updates. Texture IDs are isolated per
addon. Cursor/IME output, text copying, and HTTP(S) links are forwarded to the OS.
Obsolete workspace content is discarded. V1 uses egui 0.36.2's serialized input
and mesh types; incompatible changes require a new surface version. No Rust ABI
crosses the boundary. Addon accessibility trees are not merged in v1.

Diagnostics go to stderr and appear under Logs → Addons. Stdout is reserved for
protocol messages. The host forwards only manifest-declared secret environment
names (`environment = ["EXAMPLE_API_KEY"]`). Addons redact diagnostic content
before emitting it and never persist credentials in state files.

Optional `runtime`/`authentication` helpers support container commands and atomic
private JSON credential transfer. Each addon chooses its own tool, environment,
and auth source/destination. Disabled or absent addons do not transfer credentials.

`[migration]` declares old settings keys: `enabled` (boolean-key array),
`selection`, `sessions`, and `autostart`. Explicit new preferences win. The host
passes sessions/autostart via `Context.state`; unconsumed legacy settings remain
available until the corresponding addon is installed.

```sh
python3 scripts/addons.py --addon codex --install
python3 scripts/addons.py --addon opencode --install
python3 scripts/addons.py --test
focus-editor --no-addon codex --no-addon opencode .
```

Build/release tooling discovers bundled manifests. Packaged executables support
`--version`, printing their executable name followed by the version.
