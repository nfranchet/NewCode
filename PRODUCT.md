# Product brief: Focus Editor

## Goal

A fast, Linux-first Rust desktop editor for project file management, source editing, Git review, an interactive shell, and Codex. Keep the working layout compact and make common actions easy to identify.

## Window and visual design

The left activity bar selects Explorer, Search, or Source Control. The center displays open files or a Source Control diff. An interactive shell occupies the bottom and an optional Codex/OpenCode assistant pane occupies the right. Pane sizes are adjustable and saved.

Nord Light uses cool gray surfaces, muted blue accents, compact controls, square corners, and semantic Git colors, based on the Nord concept in `ui-mocks/index.html`. The terminal sits beneath the center editor column, and sidebars extend to the status bar. Bundled addons inherit resolved host appearance through the generic surface theme contract. Focus, selected tabs, status labels, vector icons, colored file markers, and steady input cursors remain visible. A bottom status bar reports file/save activity. New mockup capabilities and other theme presets are tracked in `TODO.md`.

## Explorer and documents

- Single-click Explorer files to open one replaceable preview tab. Double-click an Explorer file or its tab to keep it permanently; editing also pins it. Use italic preview labels with an explanatory tooltip. Selecting a permanent file activates its existing tab. An optional launch directory selects the workspace.
- Create a file or folder through a temporary inline name field beside the displayed file, with the workspace root as the fallback when no file is open. Reveal nested destinations; label destinations outside the workspace above the tree.
- Create on Enter only. Escape, focus loss, sidebar changes, or workspace changes cancel without filesystem writes. Validate names, preserve failed drafts for correction, and never overwrite existing entries.
- Provide right-click Rename, Move, and Delete actions. Deletion requires an in-app confirmation, warns about folder contents and unsaved edits, and removes affected tabs only on success. Deleting a symbolic link preserves its target.
- Edit/save text with lightweight extension-based syntax coloring. Keep source lines unwrapped and horizontally scrollable.
- Keep tabs on one scrollable row with close controls and a searchable open-files menu. Right-click a tab title for Close file, Close others (keeping that tab), or Close all. Closing modified files asks Save, Discard, or Cancel for each file in tab order. Cancel stops the remaining closes; failed saves preserve the file and confirmation.
- Preserve unsaved contents and separate Undo state per open file during tab switches. Modified indicators and close confirmations cover inactive files too; Save targets the file being closed. Keep preview and document state synchronized during rename, move, and deletion.
- Restore all tabs as permanent, including the last preview; restore workspace, tab order, and active file on relaunch, skipping missing/unreadable files. A different launch workspace does not inherit unrelated tabs. Restore saved disk contents, not unsaved buffers or caret/scroll positions.

## Search and clipboard

Workspace search scans readable text files and groups matching lines by filename. Selecting a result opens the file with the matching line and nearby preceding context visible.

Ctrl+F opens case-insensitive Find in the active file, with match highlights, a count, and forward/backward navigation. Provide editor clipboard shortcuts and Edit-menu actions for Save, Copy, Paste, Find, and Select All.

## Source Control

Use one repository/branch header and Refresh icon. Anchor the commit composer above a scrolling list of nonempty Conflicts, Staged Changes, Changes, and Untracked groups. Untracked files appear only in their own group. Each file occupies one compact row with four columns: a capital status letter, the repository-relative path, +added lines, and -deleted lines. Both count columns are green and aligned on the right; truncate paths first and expose full paths, descriptions, rename sources, and complete counts on hover. Use N for new/untracked files and U for conflicts. New text files show their line count as additions and zero deletions, calculated in the background; binary, unreadable, and unsupported files have unavailable-count placeholders. Partially staged files appear on both applicable sides with their respective counts. Stage/Unstage is available through double-click, Space, the row context menu, and group buttons; Open file is in the row context menu. Conflicts have no bulk staging. A clean repository has a single clean-state message.

Keep periodic three-second status reads independent from mutations and diff loads. Unchanged reads must not disable controls, display automatic busy indicators, clear content, move the commit form, or disturb focus, expansion, selection, or scrolling. Preserve snapshots after read failures, coalesce refresh requests, and reject obsolete replies after operations or repository/context changes. Show concise feedback with expandable error details. Explain disabled commit conditions, commit staged content only, and preserve drafts on failure or when edited while a commit is pending.

Selecting a file displays an aligned Before/After comparison in the center, including explicit loading and error states that avoid flashing the editor. Preserve comparison scroll positions across same-target reloads. Keep existing hunk safeguards: retain both sides and scroll position after hunk actions, tint staged hunks blue with a Staged label, retain red/green change indicators, apply original patch bytes, and require Reload diff after external or whole-file index changes. Returning to Explorer/Search restores the active buffer; selecting an editor tab dismisses the review.

## Shell and Codex

The bottom pane is a real PTY-backed interactive shell, displayed by the existing egui_tty/Ghostty terminal component, with a Restart icon in the Terminal/Logs title row. Provide 10,000 lines of scrollback, selection/copy, system monospace and symbol fonts, mouse reporting, and alternate-screen restoration for full-screen applications. Preserve the screen on exit, report startup failures, and clean up old PTY sessions on Restart, workspace/container replacement, and shutdown. Start the configured shell interactively with its normal startup files; resolve the selected user’s shell inside Dev Containers. While focused, it owns keyboard input before panel navigation or editor shortcuts: Tab/Shift+Tab, Escape, navigation, F1–F12, and Ctrl/Alt combinations go to the shell. Clicking another panel transfers focus normally. Ctrl+C interrupts and Ctrl+V pastes. Respect application-cursor and bracketed-paste modes for shells such as zsh. The optional Codex addon contributes a right-dock App Server client over JSONL stdio, with Start/Stop, visible connection/activity status, streaming turns, Interrupt, approval responses, and user-input responses.

Resume the latest workspace session at startup and allow choosing older sessions or creating a new one. Session labels are compact when closed and longer in the dropdown. Provide model, thinking-level, and mode selectors near Send, populated from Codex's catalog. Prefer Plan initially, with the first available mode as fallback. Support Ctrl+Enter to send and Tab/Shift+Tab to cycle modes in the idle composer.

Show proposed plans as expanded message cards and retain them in restored history. Request automatic reasoning summaries, preserve streamed sections, and show supplied reasoning content when no summary is available. Show supplied reasoning expanded initially, preserve manual collapse state during streaming/completion, and keep command details expandable; show Thinking while an empty reasoning item is running, hide empty completed items, and report turn completion in the conversation. Show available account limits and token/context usage; refresh account limits every minute and on request.

After a successful completed plan, provide a Go action in Plan mode that selects Default and sends Go in the same session, preserving the selected model, thinking level, and unsent draft. Disable it during work, pending user responses, disconnected sessions, or unavailable Default mode; prevent duplicate submissions and allow explicit retry after submission failure.

## Assistant switching and OpenCode

Provide an always-accessible Addons toolbar menu with independent Codex and OpenCode enable switches. Use status-bearing tabs in one right pane when both are enabled. Inactive tabs continue processing events and retain their drafts, selections, conversation, and scroll state. Disabling stops and hides that assistant; disabling both releases the entire right-pane width to the editor. Save visibility, selected tab, and pane width. Initially enable Codex only. Keep Start/Stop separate from visibility.

Use one shared Codex-style presentation for both providers: header, status badges, session controls, usage section, message/approval cards, and composer. Reserve the composer in the same frame before laying out the transcript. Input grows from three rows to a bounded 120px maximum (reduced for short panels), then scrolls; Send, Interrupt, and the three selectors remain visible below it. Render assistant replies, plans, and reasoning through an existing Markdown viewer, with selectable/copyable fenced code and bounded wide content; retain literal user messages and tool output. Provider catalogs, sessions, permissions, and protocol behavior remain independent.

OpenCode uses an owned localhost server and existing CLI configuration/authentication. Provide current-workspace session restore/selection/creation, streaming messages, reasoning/tool details, catalog-driven provider/model/variant/primary-agent choices, Plan-first fallback, Ctrl+Enter send, Tab/Shift+Tab agent cycling, interrupt, native permission replies, and user questions. A completed successful Plan response can offer Go to select Build and send Go while preserving the draft. Display reported tokens/cost/context information and explicitly unavailable account quotas. Poll enabled clients independently of rendering; recover stream state without resending prompts. Keep workspace sessions and drafts isolated; stop old workspace connections on switching. Clean up owned processes on disable/exit.

## Platform and boundaries

- Linux and Apple Silicon macOS desktop application using Rust and eframe/egui, with native filesystem operations and a PTY.
- Support Compose-based Dev Containers directly through Docker and the Docker Compose plugin. The native editor keeps the bind-mounted files on the host while the shell, Git, Codex, and OpenCode execute with `docker exec` in the selected service. Do not require, install, or invoke Node.js or the Dev Container CLI. Containers remain running on editor exit; start, reconnect, stop, and rebuild are explicit toolbar actions.
- Interpret only the Compose selection fields needed to connect (`dockerComposeFile`, `service`, `workspaceFolder`, and `remoteUser`). Dockerfiles and Compose define provisioning; Dev Container Features and lifecycle commands are not interpreted.
- Git is required for Source Control; an installed, authenticated Codex CLI is required for the Codex panel. Both capabilities depend on their local tools and configuration.
- Automatic Codex startup can be disabled in saved settings or with `--no-addon codex`; `--no-addon opencode` skips OpenCode startup. Visibility is independently controlled by the Addons menu.
- No extension marketplace, debugger, compiler-backed language services, Git history/branch-management UI, individual-line staging, or unsaved-buffer recovery is currently provided.
- Renaming and moving use existing dialogs; new-entry naming and delete confirmation are handled inside the application.

## Launch behavior

On Linux, launch in the background by default, preserving the calling directory, environment, workspace argument, and `--no-addon codex`. Provide `--foreground` for terminal diagnostics. Write current-session background output/errors to the local data directory under `focus-editor/logs/launch.log`; report failures to spawn or open the log in the terminal. Each launch opens an independent editor instance.

Use the native window title `Focus Editor — workspace-name`, updating it when a workspace is restored, supplied at launch, or changed in the UI. Use the full path for filesystem roots and `Focus Editor` without a workspace. Keep the application identity stable for Linux desktop grouping.

## Diagnostics

Default application logging to Info, reserving it for important lifecycle paths. Use Debug for discrete application actions, Warn for recovered failures/configuration issues, Error for fatal failures, and Trace for low-level metadata. Provide `--log-level LEVEL` and `--log-level=LEVEL`, preserve them across background launching, and keep their values out of workspace selection. Explicit levels override `RUST_LOG`; otherwise honor component filters and default dependencies to Warn. Include timestamp, level, PID, and component. Write stderr in the foreground and the existing launch log in the background. Exclude document/terminal contents, credentials, and per-frame rendering noise. Normal Info logging excludes prompts, answers, and protocol payloads; Debug/Trace may include redacted Codex protocol content as described below.

Dev Container failures use compact menu feedback with Retry; detailed bounded diagnostics go to the application log once per configuration or lifecycle failure, with credentials and build-command payloads redacted. Preserve stdout, stderr, and process status; emit separate timestamped records for diagnostic lines, keeping surrounding failure explanations and exit codes. Successful connection and shutdown use Info.

Dev Container assistant authentication: install both native CLIs with official curl installers in the bundled image. Each enabled assistant addon copies only its own host authentication file before starting its container tool; copy authentication files atomically with private permissions as the configured remote user; preserve package installations. Forward supported provider/server authentication environment variables by name. Keep history, databases, configuration, and refreshed container credentials independent. Missing host files retain container credentials with a warning; failed transfers fail connection without logging secrets.

## Release distribution

Published GitHub releases and prereleases build the exact release tag for Linux amd64 and Linux arm64 on native hosted runners. Package Linux executables with installation notes and the Apache license in tar.gz archives. Publish both verified archives and SHA-256 checksums only after both builds pass; macOS builds and DMGs are skipped for automatic releases. Retain macOS source compatibility and manual app/DMG packaging. Use Cargo metadata for executable/bundle versions, tag-based download names, least-privilege GitHub tokens, and rerunnable uploads. Do not bundle external Git, Docker, or assistant installations. macOS starts its GUI and discovers common user/Homebrew tool paths when opened from Finder; Developer ID signing/notarization is outside this workflow. Linux background launching is unchanged.

## Terminal and Logs panel

Provide Terminal/Logs tabs in the shared resizable bottom panel, defaulting to Terminal. Keep the terminal alive and drain hidden output, but release its keyboard focus in Logs. Show selectable timestamped/severity-colored log records, a component selector (All, Editor, Git, Codex, OpenCode, Dev Container, Docker Compose, Terminal), search/severity filters, Copy visible logs, source-specific Clear, and Follow latest with manual scroll pause. The Dev Container menu opens its logs directly. Stream Compose diagnostics and sanitized assistant stderr. Keep terminal contents and OpenCode protocol content out of logs; Codex protocol content is included only when debug logging is enabled. Maintain an empty process-local viewer per launch with a 10,000-record/8-MiB retention bound. Replace launch.log each startup with no rotation/history; keep foreground stderr too. Clearing viewer records does not erase the diagnostic file. Concurrent instances retain independent viewers/output handles; the file pathname points to the newest launch.

Codex protocol diagnostics are enabled only at runtime Debug/Trace levels or matching component filters. Record incoming/outgoing JSON with direction, sequence, RPC IDs, methods, and numbered long-payload chunks in the existing Codex Logs source and current-session file. Redact credentials before the logging facade and preserve the actual wire payload. Normal Info logging excludes protocol content.

## Executable addons and dock placement

Codex and OpenCode live in separate addon crates with their own transports, authentication handling, lifecycle, session state, and tests. The editor has no provider-specific clients or assistant enum. Install either or both addons beside logs, and use the generic Addons menu to enable, disable, or restart them. Right-dock tab selection, width, process supervision, and input routing belong to the editor; disabled or absent addons do not reserve space or launch processes.

Any addon can declare `panel = "left"` (the default) or `panel = "right"`. Right-dock addons can use the existing declarative widgets or the optional versioned UI surface protocol. The shared helper relays typed input, meshes, compressed textures, text clipboard, IME, and safe external links while preserving existing native controls. Codex and OpenCode use this helper and retain Markdown, collapsible reasoning, bounded scrolling composers, sessions, approvals, Plan/Go, and usage controls. Hidden enabled addons continue processing backend events.

Build and release tooling discovers addon manifests. Push/release checks run the SDK, shared helpers, and every bundled addon independently. The installer can select addon IDs and preserves each addon's non-secret data. Manifest-declared migration metadata imports old enablement, selection, and session settings without adding provider-specific logic to the editor.

## Addon management

Provide a built-in Addons activity directly below Source Control. Its searchable
Installed and Available lists open details in the center with install/update,
enable/disable, restart, credential, and uninstall controls. Use the official
release catalog and trusted local manifests/archives. Updates are explicit,
validate compatibility and SHA-256, run in the background, and replace only the
affected addon with rollback and cross-instance process locks. Keep persistent
data beside logs under addon-data/<id>, outside executable installations; keep
data and credentials on uninstall unless data removal is explicitly selected.
Release builds select their own tag; source builds allow latest or an explicit
tag. Publish standalone packages for every manifest-discovered addon on both
Linux architectures, an aggregate addons.json catalog, and verified checksums.
