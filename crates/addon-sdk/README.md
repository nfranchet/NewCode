# Focus executable addon SDK, protocol 1

An addon is an independently built executable. It communicates with Focus Editor
using JSON-RPC 2.0 objects separated by newlines on stdin/stdout (8 MiB maximum
per message). Send diagnostics to stderr. The host owns native UI rendering;
the addon has no dependency on egui or the editor crate. Other languages can
implement the same protocol. Installed addons are trusted OS programs.

Install each addon under `<application-data>/focus-editor/addons/<id>/`, beside
`logs/`. Use **Addons → Install local addon…** and select its `addon.toml`, or use
the release installer. Repository directories are never searched for addons.
Executable and PNG icon paths must stay inside the installation directory.

```toml
id = "example"
name = "Example"
version = "0.1.0"
protocol = 1
executable = "example-addon"
icon = "icon.png"
```

## Lifecycle and UI

1. Host sends request `initialize` with `{ "protocol": 1, "context": Context }`.
   Respond with `{ "protocol": 1 }` within five seconds.
2. Publish notifications `view.update` containing `View`. One activity-bar icon
   opens the addon's sidebar; a nonempty `detail` opens an optional central view.
   The host supports text, Markdown, buttons, inputs, checkboxes, selectors,
   links, separators, and rows. Widget/action IDs must be stable and unique.
3. Host sends `action` notifications with an action ID, current context
   generation, and all rendered form values. Handle actions in the executable,
   publishing a busy view before long operations and a completed view afterward.
4. Host sends `context.changed` with a new generation when the workspace or
   Dev Container changes. Replace repository state; ignore obsolete actions.
   Every view and service request carries its originating generation.
5. `view.activated` announces selection. `tick` arrives approximately every
   30 seconds with `{ "visible": bool }`, allowing visible refresh and pending
   operation checks. `shutdown` requests termination; the host allows two seconds.

`View.form` identifies a form. Changing it resets host input values and opens
nonempty detail content. Keeping it preserves edits during background refresh.
Set `View.reveal_detail` on selection to reopen the same item after dismissal.
The host namespaces widget state per addon. `host.credentials` is a reserved
button action that opens the native credential dialog without transmitting
token text to the executable. Normal file tabs dismiss addon detail views.

## Host services

Use `Connection::call(generation, Service)` for background tool execution, HTTPS,
or Git. Interleaved notifications are queued until the request completes.

- `Run`: explicit program/arguments and optional stdin; no implicit shell.
  A `missing_tool` reply allows the addon to choose a fallback. Other errors
  should not trigger an identity/backend change or automatic write retry.
- `Http`: HTTPS, optional JSON body, bounded responses, no automatic redirects.
- `Git`: read queries, fetch, checkout, push, and checked branch deletion.
  Mutations share Source Control's lease and use the active Dev Container when
  present. Checkout/cleanup resolves dirty editor buffers through the host.
  Conflicting worktree changes and new branch commits are preserved.

Service replies contain `ok`, `code`, and `output`. HTTP/command failures are
returned as errors; check remote state before retrying uncertain writes.
Unsupported protocol versions, malformed frames, and process failures leave
the editor running and expose Restart in the addon UI.

Optional manifest credentials declare `origin`, `program`, `environment`,
`git_origin`, and user-facing `instructions`. The host reads/stores credentials
in the OS keyring, attaches them only to the matching HTTPS origin or declared
program, and returns sanitized results. It does not expose a token-reading API.
Use `Context.storage` for non-secret persistent addon state; preserve the
generation/workspace association of pending local operations.

The GitHub addon is a complete example. Build it separately with:

```sh
cargo build --locked --manifest-path addons/github/Cargo.toml
python3 scripts/addons.py --install
```
