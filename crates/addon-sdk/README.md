# Focus executable addon SDK, protocol 1

An addon is an independently built executable. It communicates with Focus Editor
using JSON-RPC 2.0 objects separated by newlines on stdin/stdout (8 MiB maximum
per message). Send diagnostics to stderr. The host owns native UI rendering;
the addon has no dependency on egui or the editor crate. Other languages can
implement the same protocol. Installed addons are trusted OS programs.

Install each addon under `<application-data>/focus-editor/addons/<id>/`, beside
`logs/`. Open the **Addons** activity below Source Control, choose **Install local
addon…** and select its `addon.toml` or standalone `.tar.gz`, or use the release
installer. Repository directories are never searched for addons.
Executable and PNG icon paths must stay inside the installation directory.
Any addon may declare `panel = "right"` to use the shared right-hand dock.
The default `ui = "widgets"` uses declarative controls; rich interfaces can use
`ui = "surface"` through the optional [UI helper](../addon-support/README.md).

```toml
id = "example"
name = "Example"
description = "What this addon provides"
publisher = "Your name or organization"
version = "0.1.0"
protocol = 1
executable = "example-addon"
icon = "icon.png"
```

`description` and `publisher` are optional display metadata. Release staging adds
an optional top-level `target`, such as `x86_64-unknown-linux-gnu`; the host rejects
packages for another target. Keep the manifest version equal to the addon Cargo
package version. The ID must match its directory and contain only letters,
digits, `_`, and `-`.

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
generation/workspace association of pending local operations. This path is
`<application-data>/focus-editor/workspaces/<workspace-id>/addon-data/<id>`,
outside the shared executable folder. Updates and uninstall preserve workspace
data and OS-keyring credentials. Old global state is not imported. Do not write
runtime state into the installation directory.

The GitHub addon is a complete example. Build it separately with:

```sh
cargo build --locked --manifest-path addons/github/Cargo.toml
python3 scripts/addons.py --addon github --install
```

## Release packages and catalog

Place a crate and `addon.toml` under `addons/<id>/`. The build helper and release
workflow discover all such manifests; the editor crate does not compile them.
Use `python3 scripts/addons.py --release --target <triple>` to build and stage all
addons under `target/<triple>/release/addons/`. `--addon <id>` selects one and can
be repeated. `--test` checks formatting and tests the SDK and selected crates.
Release executables must support `--version`, printing their executable basename
followed by the version without starting the addon protocol. The release job
checks architecture, shared libraries, and that version response. Addon-specific
license/notices files take precedence over the repository defaults.

`scripts/release/package.py <triple> <tag>` emits a separate
`focus-addon-<id>-<tag>-<platform>.tar.gz` for every staged addon, as well as the
editor archive and `addons-<platform>.json`. Each addon archive contains files
directly at its root: `addon.toml`, its declared executable and PNG icon,
`LICENSE`, and `THIRD_PARTY_NOTICES.txt`. Relative asset subdirectories are
supported. Runtime state, credentials, extra files, symlinks, and hardlinks are
excluded. The editor accepts only those declared assets and optional notices;
compressed and expanded size limits are 256 MiB and 512 MiB.

The final release job checks the complete platform matrix and emits `addons.json`
and `SHA256SUMS`. `Catalog` and `Package` in this SDK define catalog schema 1:

```json
{
  "schema": 1,
  "release": "v0.1.0",
  "editor_version": "0.1.0",
  "packages": [{
    "id": "example", "name": "Example", "description": "An example addon",
    "publisher": "Example project", "version": "0.1.0", "protocol": 1,
    "target": "x86_64-unknown-linux-gnu",
    "archive": "focus-addon-example-v0.1.0-linux-amd64.tar.gz",
    "size": 1234, "sha256": "<64 hexadecimal characters>"
  }]
}
```

The manager fetches the official catalog anonymously, checks editor version,
protocol, target, archive SHA-256/size, and manifest identity before replacement.
Local packages use the same manifest/archive checks without an official
checksum. Installation receipts are separate from addon state. Shared process
leases prevent updates while another editor uses the executable; updates use a
rollback journal and restart only the affected process. No protocol changes or
addon-specific host code are needed to appear in the manager or release catalog.
