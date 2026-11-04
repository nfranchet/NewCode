//! Background Git actions and applying completed results to panel state.

use super::{
    GitUi,
    repository::{Snapshot, conflict, git, snapshot},
};
use eframe::egui;
use std::{process::Command, sync::mpsc, time::Instant};

pub(super) enum Action {
    Refresh,
    Index { paths: Vec<String>, stage: bool },
    Commit(String),
    Diff(String, bool),
    Patch(String, bool),
}

pub(super) struct Reply {
    pub(super) snapshot: Result<Snapshot, String>,
    pub(super) message: Option<Result<String, String>>,
    pub(super) committed: bool,
    pub(super) diff: Option<String>,
}

impl GitUi {
    pub(super) fn start(&mut self, action: Action, ctx: &egui::Context) {
        if let Action::Diff(path, staged) = &action {
            self.diff_hidden = false;
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
    pub(super) fn poll_reply(&mut self) {
        if let Some(reply) = self.pending.as_ref().and_then(|rx| rx.try_recv().ok()) {
            self.pending = None;
            self.refreshed = Some(Instant::now());
            match reply.snapshot {
                Ok(s) => self.snapshot = Some(s),
                Err(e) => {
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
            if let (Some(snapshot), Some((path, staged))) = (&self.snapshot, &mut self.diff_target)
            {
                if let Some(change) = snapshot.files.iter().find(|c| &c.path == path) {
                    let has_staged =
                        change.index != ' ' && change.index != '?' && !conflict(change);
                    let has_unstaged =
                        change.work != ' ' || change.index == '?' || conflict(change);
                    if *staged && !has_staged && has_unstaged {
                        *staged = false;
                    } else if !*staged && !has_unstaged && has_staged {
                        *staged = true;
                    }
                }
            }
            if reply.diff.is_some() {
                self.diff = reply.diff;
            }
        }
    }
}
