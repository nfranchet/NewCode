use crate::{App, Detail, api::string};
use focus_addon_sdk::{View, Widget as W};

impl App {
    pub(crate) fn view(&self, busy: bool) -> View {
        let mut sidebar = vec![
            W::Row {
                children: vec![
                    W::button("issues", "Issues"),
                    W::button("pulls", "Pull requests"),
                ],
            },
            W::button("host.credentials", "Credentials…"),
        ];
        if self.context.workspace.is_none() {
            sidebar.push(W::text("Open a workspace to see GitHub issues."));
        } else if self.repos.is_empty() {
            sidebar.push(W::text("No GitHub.com remote found in this workspace."));
        }
        if let Ok(repo) = self.repo() {
            sidebar.push(W::Select {
                id: "remote".into(),
                label: "Repository".into(),
                value: repo.remote.clone(),
                options: self
                    .repos
                    .iter()
                    .map(|r| (r.remote.clone(), format!("{} ({})", r.slug(), r.remote)))
                    .collect(),
            });
            sidebar.push(W::button("repository", "Use repository"));
            sidebar.push(W::text(format!("Branch: {}", self.branch)));
            sidebar.push(W::text(if self.api.no_gh {
                "Connection: GitHub HTTPS API / OS keyring"
            } else {
                "Connection: gh / existing login or OS keyring"
            }));
            sidebar.push(W::Row {
                children: vec![
                    W::button("refresh", "Refresh"),
                    W::button("new_pr", "Push & create PR…"),
                ],
            });
            sidebar.push(W::input("query", "Filter loaded items", &self.query));
            sidebar.push(W::button("filter", "Filter"));
            if !self.notice.is_empty() {
                sidebar.push(W::text(&self.notice));
            }
            let needle = self.query.to_lowercase();
            let items: Vec<_> = self
                .items
                .iter()
                .filter(|item| {
                    format!("{} {}", item["number"], string(item, "title"))
                        .to_lowercase()
                        .contains(&needle)
                })
                .collect();
            if items.is_empty() && !busy {
                sidebar.push(W::text(if self.pulls {
                    "No open pull requests in the loaded results."
                } else {
                    "No open issues in the loaded results."
                }));
            }
            for item in items {
                let number = item["number"].as_u64().unwrap_or_default();
                let current = self.pulls && item["head"]["ref"].as_str() == Some(&self.branch);
                sidebar.push(W::button(
                    format!("item:{number}"),
                    format!(
                        "#{number} {}{}",
                        string(item, "title"),
                        if current { " · current branch" } else { "" }
                    ),
                ));
            }
            if self.more {
                sidebar.push(W::button("more", "Load more"));
            }
            let pending = self
                .stored
                .cleanup
                .iter()
                .filter(|i| Some(&i.workspace) == self.context.workspace.as_ref())
                .count();
            if pending > 0 {
                sidebar.push(W::text(format!(
                    "{pending} pending branch cleanup operation(s)"
                )));
                sidebar.push(W::button("cleanup", "Check merge and cleanup"));
            }
        } else if !self.notice.is_empty() {
            sidebar.push(W::text(&self.notice));
        }
        let base = string(&self.metadata, "_base");
        let mut view = View {
            generation: self.context.generation,
            title: "GitHub".into(),
            form: format!("{}:{}:list", self.selected, self.pulls),
            sidebar,
            detail: vec![],
            busy,
            reveal_detail: false,
        };
        match &self.detail {
            Detail::None => (),
            Detail::Item { value, pull } => {
                let number = value["number"].as_u64().unwrap_or_default();
                view.form = format!("{}:{}:{number}", self.selected, pull);
                view.title = format!("#{number} {}", string(value, "title"));
                view.detail = vec![
                    W::Link {
                        label: "Open on GitHub".into(),
                        url: string(value, "html_url"),
                    },
                    W::text(format!(
                        "{} · by {}",
                        string(value, "state"),
                        string(&value["user"], "login")
                    )),
                    W::Markdown {
                        text: string(value, "body"),
                    },
                    W::Separator,
                ];
                if *pull {
                    view.detail.push(W::text(format!(
                        "{} → {} · head {}",
                        string(&value["head"], "ref"),
                        string(&value["base"], "ref"),
                        string(&value["head"], "sha")
                    )));
                    view.detail.push(W::Checkbox {
                        id: "delete".into(),
                        label: "Delete branches after merge".into(),
                        checked: false,
                    });
                    if self.metadata["delete_branch_on_merge"].as_bool() == Some(true) {
                        view.detail.push(W::text("GitHub also has automatic branch deletion enabled for this repository."));
                    }
                    view.detail.push(W::Button {
                        id: "merge".into(),
                        label: "Auto-merge (squash)".into(),
                        enabled: value["state"].as_str() == Some("open")
                            && value["draft"].as_bool() != Some(true),
                    });
                } else {
                    view.detail.push(W::input(
                        "branch",
                        "Issue branch",
                        format!("issue-{number}"),
                    ));
                    view.detail.push(W::input("base", "Base branch", &base));
                    view.detail.push(W::Checkbox {
                        id: "reuse".into(),
                        label: "Reuse existing branch".into(),
                        checked: false,
                    });
                    view.detail
                        .push(W::button("branch", "Create issue branch & checkout"));
                }
            }
            Detail::CreatePr { head, title, body } => {
                view.form = format!("{}:create:{head}", self.selected);
                view.title = "Push & create pull request".into();
                let remote = self
                    .repo()
                    .map(|r| format!("{} ({})", r.remote, r.slug()))
                    .unwrap_or_default();
                view.detail = vec![
                    W::text(format!("Push {head} to {remote}")),
                    W::input("base", "Base branch", base),
                    W::input("title", "Title", title),
                    W::Input {
                        id: "body".into(),
                        label: "Description".into(),
                        value: body.clone(),
                        multiline: true,
                    },
                    W::button("create_pr", "Push & create PR"),
                ];
            }
        }
        view
    }
}
