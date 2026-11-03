use eframe::egui::{self, Color32};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    process::Command,
    sync::mpsc,
    time::{Duration, Instant},
};

fn git(root: &Path, args: &[&str]) -> Result<Vec<u8>, String> {
    let output = Command::new("git")
        .current_dir(root)
        .env("GIT_LITERAL_PATHSPECS", "1")
        .args(args)
        .output()
        .map_err(|e| e.to_string())?;
    if output.status.success() {
        Ok(output.stdout)
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_owned())
    }
}

#[derive(Clone, Debug)]
struct Change {
    path: String,
    old: Option<String>,
    index: char,
    work: char,
}
fn changes(bytes: &[u8]) -> Vec<Change> {
    let mut fields = bytes.split(|b| *b == 0).filter(|s| !s.is_empty());
    let mut result = Vec::new();
    while let Some(field) = fields.next() {
        if field.len() < 4 {
            continue;
        }
        let index = field[0] as char;
        let work = field[1] as char;
        let old = if matches!(index, 'R' | 'C') || matches!(work, 'R' | 'C') {
            fields
                .next()
                .map(|s| String::from_utf8_lossy(s).into_owned())
        } else {
            None
        };
        result.push(Change {
            path: String::from_utf8_lossy(&field[3..]).into_owned(),
            old,
            index,
            work,
        });
    }
    result
}
struct Snapshot {
    root: PathBuf,
    branch: String,
    files: Vec<Change>,
    staged: String,
    unstaged: String,
    counts: [HashMap<String, String>; 2],
}
fn snapshot(workspace: &Path) -> Result<Snapshot, String> {
    let root = git(workspace, &["rev-parse", "--show-toplevel"])?;
    let root = PathBuf::from(String::from_utf8_lossy(&root).trim_end());
    let branch = String::from_utf8_lossy(&git(&root, &["status", "--short", "--branch"])?)
        .lines()
        .next()
        .unwrap_or("")
        .trim_start_matches("## ")
        .to_owned();
    let files = changes(&git(
        &root,
        &["status", "--porcelain=v1", "-z", "--untracked-files=all"],
    )?);
    let stats = |cached: bool| -> Result<String, String> {
        let args = if cached {
            vec!["diff", "--cached", "--shortstat"]
        } else {
            vec!["diff", "--shortstat"]
        };
        Ok(String::from_utf8_lossy(&git(&root, &args)?)
            .trim()
            .to_owned())
    };
    let counts = |cached: bool| -> Result<HashMap<String, String>, String> {
        let mut args = vec!["diff", "--numstat", "-z", "--no-renames"];
        if cached {
            args.push("--cached");
        }
        let bytes = git(&root, &args)?;
        Ok(bytes
            .split(|b| *b == 0)
            .filter_map(|entry| {
                let mut parts = entry.splitn(3, |b| *b == b'\t');
                let added = String::from_utf8_lossy(parts.next()?);
                let removed = String::from_utf8_lossy(parts.next()?);
                let path = String::from_utf8_lossy(parts.next()?).into_owned();
                Some((
                    path,
                    if added == "-" {
                        "Binary".into()
                    } else {
                        format!("+{added} −{removed}")
                    },
                ))
            })
            .collect())
    };
    Ok(Snapshot {
        counts: [counts(true)?, counts(false)?],
        staged: stats(true)?,
        unstaged: stats(false)?,
        root,
        branch,
        files,
    })
}
fn label(code: char) -> &'static str {
    match code {
        'M' => "Modified",
        'A' => "Added",
        'D' => "Deleted",
        'R' => "Renamed",
        'C' => "Copied",
        'U' => "Conflict",
        '?' => "Untracked",
        'T' => "Type changed",
        _ => "Changed",
    }
}
fn conflict(c: &Change) -> bool {
    c.index == 'U'
        || c.work == 'U'
        || (c.index == 'A' && c.work == 'A')
        || (c.index == 'D' && c.work == 'D')
}

enum Action {
    Refresh,
    Index { paths: Vec<String>, stage: bool },
    Commit(String),
    Diff(String, bool),
    Patch(String, bool),
}
struct Reply {
    snapshot: Result<Snapshot, String>,
    message: Option<Result<String, String>>,
    committed: bool,
    diff: Option<String>,
}
#[derive(Default)]
pub(crate) struct GitUi {
    workspace: Option<PathBuf>,
    snapshot: Option<Snapshot>,
    pending: Option<mpsc::Receiver<Reply>>,
    refreshed: Option<Instant>,
    message: String,
    commit: String,
    diff: Option<String>,
    diff_target: Option<(String, bool)>,
}
impl GitUi {
    fn start(&mut self, action: Action, ctx: &egui::Context) {
        if let Action::Diff(path, staged) = &action {
            self.diff_target = Some((path.clone(), *staged));
        }
        let Some(workspace) = self.workspace.clone() else {
            return;
        };
        let root = self
            .snapshot
            .as_ref()
            .map(|s| s.root.clone())
            .unwrap_or(workspace.clone());
        let (tx, rx) = mpsc::channel();
        self.pending = Some(rx);
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let mut diff = None;
            let mut committed = false;
            let message = match action {
                Action::Refresh => None,
                Action::Index { paths, stage } => {
                    let mut args = if stage {
                        vec!["add", "--"]
                    } else {
                        vec!["reset", "-q", "--"]
                    };
                    args.extend(paths.iter().map(String::as_str));
                    Some(git(&root, &args).map(|_| "Index updated".into()))
                }
                Action::Commit(message) => {
                    let result = git(&root, &["commit", "-m", &message]);
                    committed = result.is_ok();
                    Some(result.map(|v| String::from_utf8_lossy(&v).trim().into()))
                }
                Action::Patch(patch, reverse) => {
                    use std::io::Write;
                    use std::process::Stdio;
                    let result = (|| -> Result<String, String> {
                        let mut command = Command::new("git");
                        command.current_dir(&root).args(["apply", "--cached"]);
                        if reverse {
                            command.arg("--reverse");
                        }
                        let mut child = command
                            .stdin(Stdio::piped())
                            .stderr(Stdio::piped())
                            .stdout(Stdio::piped())
                            .spawn()
                            .map_err(|e| e.to_string())?;
                        child
                            .stdin
                            .take()
                            .unwrap()
                            .write_all(patch.as_bytes())
                            .map_err(|e| e.to_string())?;
                        let output = child.wait_with_output().map_err(|e| e.to_string())?;
                        if output.status.success() {
                            Ok("Hunk applied. Select the file to refresh its diff.".into())
                        } else {
                            Err(String::from_utf8_lossy(&output.stderr).into())
                        }
                    })();
                    Some(result)
                }
                Action::Diff(path, staged) => {
                    let mut args = vec!["diff", "--no-ext-diff", "--no-textconv"];
                    if staged {
                        args.push("--cached");
                    }
                    args.extend(["--", &path]);
                    match git(&root, &args) {
                        Ok(v) => {
                            diff = Some(String::from_utf8_lossy(&v).into());
                            None
                        }
                        Err(e) => Some(Err(e)),
                    }
                }
            };
            let _ = tx.send(Reply {
                committed,
                snapshot: snapshot(&workspace),
                message,
                diff,
            });
            ctx.request_repaint();
        });
    }
    pub(crate) fn show_diff(&mut self, ui: &mut egui::Ui) -> bool {
        let Some(diff) = &self.diff else {
            return false;
        };
        let mut close = false;
        let mut patch = None;
        let staged = self.diff_target.as_ref().is_some_and(|(_, staged)| *staged);
        ui.horizontal(|ui| {
            ui.strong(
                self.diff_target
                    .as_ref()
                    .map(|(path, _)| path.as_str())
                    .unwrap_or("Git diff"),
            );
            ui.label(if staged {
                "HEAD → Index"
            } else {
                "Index → Working tree"
            });
            close = ui.button("Close diff").clicked();
        });
        let lines: Vec<_> = diff.split_inclusive('\n').collect();
        let first = lines.iter().position(|line| line.starts_with("@@"));
        egui::ScrollArea::both().show(ui, |ui| {
            if diff.is_empty() {
                ui.label("No text changes available.");
            }
            for (index, line) in lines.iter().enumerate() {
                if line.starts_with("@@")
                    && !diff.contains("new file mode")
                    && !diff.contains("deleted file mode")
                    && !diff.contains("rename from")
                {
                    if ui
                        .add_enabled(
                            self.pending.is_none(),
                            egui::Button::new(if staged { "Unstage hunk" } else { "Stage hunk" }),
                        )
                        .clicked()
                    {
                        let end = lines
                            .iter()
                            .enumerate()
                            .skip(index + 1)
                            .find(|(_, l)| l.starts_with("@@"))
                            .map(|(i, _)| i)
                            .unwrap_or(lines.len());
                        patch = Some(format!(
                            "{}{}",
                            lines[..first.unwrap_or(0)].concat(),
                            lines[index..end].concat()
                        ));
                    }
                }
                let color = if line.starts_with('+') {
                    Color32::DARK_GREEN
                } else if line.starts_with('-') {
                    Color32::DARK_RED
                } else {
                    Color32::DARK_GRAY
                };
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(line.trim_end_matches('\n'))
                            .monospace()
                            .color(color),
                    )
                    .selectable(true),
                );
            }
        });
        if let Some(patch) = patch {
            self.start(Action::Patch(patch, staged), ui.ctx());
            self.diff = None;
        }
        if close {
            self.diff = None;
        }
        true
    }

    pub(crate) fn ui(&mut self, ui: &mut egui::Ui, workspace: Option<&Path>) -> Option<PathBuf> {
        if self.workspace.as_deref() != workspace {
            *self = Self {
                workspace: workspace.map(Path::to_path_buf),
                ..Self::default()
            };
        }
        if let Some(reply) = self.pending.as_ref().and_then(|rx| rx.try_recv().ok()) {
            self.pending = None;
            self.refreshed = Some(Instant::now());
            match reply.snapshot {
                Ok(s) => self.snapshot = Some(s),
                Err(e) => {
                    self.snapshot = None;
                    self.message = e;
                }
            }
            if let Some(message) = reply.message {
                match message {
                    Ok(m) => {
                        if reply.committed {
                            self.commit.clear();
                        }
                        self.message = m;
                    }
                    Err(e) => self.message = e,
                }
            }
            if reply.diff.is_some() {
                self.diff = reply.diff;
            }
        }
        if workspace.is_none() {
            ui.label("Choose a workspace folder to view Git changes.");
            return None;
        }
        let busy = self.pending.is_some();
        let mut action = None;
        if !busy
            && self
                .refreshed
                .is_none_or(|t| t.elapsed() >= Duration::from_secs(3))
        {
            action = Some(Action::Refresh);
        }
        ui.ctx().request_repaint_after(Duration::from_secs(3));
        if ui
            .add_enabled(!busy, egui::Button::new("Refresh"))
            .clicked()
        {
            action = Some(Action::Refresh);
        }
        if busy {
            ui.spinner();
        }
        let mut open = None;
        if let Some(s) = &self.snapshot {
            ui.label(egui::RichText::new(&s.branch).strong());
            ui.small(s.root.display().to_string());
            let staged = s
                .files
                .iter()
                .any(|c| c.index != ' ' && c.index != '?' && !conflict(c));
            let conflicts = s.files.iter().any(conflict);
            if conflicts {
                ui.colored_label(
                    Color32::DARK_RED,
                    "Resolve conflicts and stage the resolved files before committing.",
                );
            }
            if s.files.is_empty() {
                ui.label("Working tree clean");
            }
            egui::ScrollArea::vertical()
                .id_salt("git_changes")
                .max_height((ui.available_height() - 210.0).max(80.0))
                .show(ui, |ui| {
                    for (group, staged) in [(0, false), (1, false), (2, true), (3, false)] {
                        ui.separator();
                        ui.strong(match group {
                            0 => "Unstaged",
                            1 => "Untracked",
                            2 => "Staged",
                            _ => "Conflicts",
                        })
                        .on_hover_text(if staged {
                            "Included in the next commit"
                        } else {
                            "Not included in the next commit"
                        });
                        let files: Vec<_> = s
                            .files
                            .iter()
                            .filter(|c| match group {
                                0 => c.work != ' ' && c.index != '?' && !conflict(c),
                                1 => c.index == '?',
                                2 => c.index != ' ' && c.index != '?' && !conflict(c),
                                _ => conflict(c),
                            })
                            .collect();
                        if !files.is_empty()
                            && ui
                                .add_enabled(
                                    !busy,
                                    egui::Button::new(if staged {
                                        "Unstage all"
                                    } else {
                                        "Stage all"
                                    }),
                                )
                                .clicked()
                        {
                            let mut paths = Vec::new();
                            for c in &files {
                                paths.push(c.path.clone());
                                if let Some(old) = &c.old {
                                    paths.push(old.clone());
                                }
                            }
                            action = Some(if staged {
                                Action::Index {
                                    paths,
                                    stage: false,
                                }
                            } else {
                                Action::Index { paths, stage: true }
                            });
                        }
                        for c in files {
                            ui.push_id((&c.path, staged), |ui| {
                                let code = if staged { c.index } else { c.work };
                                let title = if conflict(c) { "Conflict" } else { label(code) };
                                if ui
                                    .selectable_label(
                                        self.diff_target.as_ref().is_some_and(|(p, cached)| {
                                            p == &c.path && *cached == staged
                                        }),
                                        format!(
                                            "{title}: {}  {}",
                                            c.path,
                                            s.counts[usize::from(!staged)]
                                                .get(&c.path)
                                                .map(String::as_str)
                                                .unwrap_or("")
                                        ),
                                    )
                                    .clicked()
                                {
                                    action = Some(Action::Diff(c.path.clone(), staged));
                                }
                                if let Some(old) = &c.old {
                                    ui.small(format!("From: {old}"));
                                }
                            });
                        }
                    }
                });
            ui.separator();
            if let Some((path, cached)) = &self.diff_target {
                ui.small(format!("Selected: {path}"));
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(
                            !busy,
                            egui::Button::new(if *cached {
                                "Unstage file"
                            } else {
                                "Stage file"
                            }),
                        )
                        .clicked()
                    {
                        let mut paths = vec![path.clone()];
                        if let Some(old) = s
                            .files
                            .iter()
                            .find(|c| &c.path == path)
                            .and_then(|c| c.old.clone())
                        {
                            paths.push(old);
                        }
                        action = Some(Action::Index {
                            paths,
                            stage: !cached,
                        });
                    }
                    if ui
                        .add_enabled(s.root.join(path).is_file(), egui::Button::new("Open file"))
                        .clicked()
                    {
                        open = Some(s.root.join(path));
                        self.diff = None;
                    }
                });
            }
            ui.separator();
            ui.add(
                egui::TextEdit::multiline(&mut self.commit)
                    .hint_text("Commit message…")
                    .desired_rows(2)
                    .desired_width(f32::INFINITY),
            );
            if ui
                .add_enabled(
                    !busy && staged && !conflicts && !self.commit.trim().is_empty(),
                    egui::Button::new("Commit staged changes"),
                )
                .clicked()
            {
                action = Some(Action::Commit(self.commit.clone()));
            }
            ui.small("Git uses saved files. Save editor changes before staging.");
        }
        if !self.message.is_empty() {
            ui.separator();
            ui.label(&self.message);
        }
        if let Some(action) = action {
            self.start(action, ui.ctx());
        }
        open
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_partial_changes_renames_and_unusual_names() {
        let files =
            changes(b"MM partial.rs\0R  new name\0old name\0?? tab\tline\n.txt\0UU conflict\0");
        assert_eq!(files.len(), 4);
        assert_eq!((files[0].index, files[0].work), ('M', 'M'));
        assert_eq!(files[1].old.as_deref(), Some("old name"));
        assert_eq!(files[2].path, "tab\tline\n.txt");
        assert!(conflict(&files[3]));
    }

    #[test]
    fn repository_staging_diff_commit_and_unstage() {
        let root = std::env::temp_dir().join(format!(
            "focus-git-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&root).unwrap();
        let run = || {
            git(&root, &["init", "-q"])?;
            git(&root, &["config", "user.name", "Test"])?;
            git(&root, &["config", "user.email", "test@example.invalid"])?;
            git(&root, &["config", "commit.gpgsign", "false"])?;
            std::fs::write(root.join("file [1].txt"), "one\n").unwrap();
            assert_eq!(snapshot(&root)?.files[0].index, '?');
            git(&root, &["add", "--", "file [1].txt"])?;
            assert_eq!(snapshot(&root)?.files[0].index, 'A');
            git(&root, &["reset", "-q", "--", "file [1].txt"])?;
            assert_eq!(snapshot(&root)?.files[0].index, '?');
            git(&root, &["add", "--", "file [1].txt"])?;
            git(
                &root,
                &["-c", "core.hooksPath=/dev/null", "commit", "-qm", "initial"],
            )?;
            std::fs::write(root.join("file [1].txt"), "one\ntwo\n").unwrap();
            git(&root, &["add", "--", "file [1].txt"])?;
            std::fs::write(root.join("file [1].txt"), "one\ntwo\nthree\n").unwrap();
            let state = snapshot(&root)?;
            assert_eq!((state.files[0].index, state.files[0].work), ('M', 'M'));
            assert_eq!(state.counts[0]["file [1].txt"], "+1 −0");
            assert_eq!(state.counts[1]["file [1].txt"], "+1 −0");
            git(&root, &["reset", "-q", "--", "file [1].txt"])?;
            assert_eq!(snapshot(&root)?.files[0].index, ' ');
            Ok::<_, String>(())
        };
        let result = run();
        std::fs::remove_dir_all(&root).unwrap();
        result.unwrap();
    }
}
