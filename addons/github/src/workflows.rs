use crate::{
    App, Cleanup, Detail,
    api::{Host, git, read, segment, string},
};
use focus_addon_sdk::{Action, GitOperation, Service};
use serde_json::json;

fn check_push_destination(
    host: &mut impl Host,
    generation: u64,
    repo: &crate::Repo,
) -> Result<(), String> {
    let url = read(
        host,
        generation,
        &["remote", "get-url", "--push", &repo.remote],
    )?;
    let actual = crate::parse_remote(&repo.remote, url.trim())
        .ok_or("The selected remote pushes outside the selected GitHub repository")?;
    if !actual.slug().eq_ignore_ascii_case(&repo.slug()) {
        return Err("This remote has a different push repository. Select or configure a remote with matching fetch/push repositories.".into());
    }
    Ok(())
}

impl App {
    pub(crate) fn prepare_pr(&mut self, host: &mut impl Host) -> Result<(), String> {
        let head = read(
            host,
            self.context.generation,
            &["rev-parse", "--abbrev-ref", "HEAD"],
        )?
        .trim()
        .to_owned();
        if head == "HEAD" || head.is_empty() {
            return Err("Checkout a branch before creating a pull request".into());
        }
        let commit = read(
            host,
            self.context.generation,
            &["log", "-1", "--format=%s%n%b"],
        )?;
        let (title, body) = commit
            .trim()
            .split_once('\n')
            .unwrap_or((commit.trim(), ""));
        let repo = self.repo()?;
        let base = string(&self.metadata, "_base");
        let range = format!("{}/{base}..HEAD", repo.remote);
        let body = read(
            host,
            self.context.generation,
            &["log", "--max-count=50", "--format=%s%n%b", &range, "--"],
        )
        .unwrap_or_else(|_| body.into());
        self.detail = Detail::CreatePr {
            head,
            title: title.into(),
            body: body.trim().into(),
        };
        Ok(())
    }
    pub(crate) fn create_pr(
        &mut self,
        host: &mut impl Host,
        action: &Action,
    ) -> Result<(), String> {
        let repo = self.repo()?;
        let Detail::CreatePr { head, .. } = &self.detail else {
            return Err("Open the create-PR form first".into());
        };
        let head = head.clone();
        let base = action.text("base");
        let title = action.text("title");
        let mut body = action.text("body");
        if title.trim().is_empty() || base.trim().is_empty() {
            return Err("Title and base branch are required".into());
        }
        if head == base {
            return Err("Head and base must be different branches".into());
        }
        check_push_destination(host, self.context.generation, &repo)?;
        if read(
            host,
            self.context.generation,
            &["rev-parse", "--abbrev-ref", "HEAD"],
        )?
        .trim()
            != head
        {
            return Err("Current branch changed. Open the create-PR form again.".into());
        }
        self.api.request(
            host,
            self.context.generation,
            "GET",
            &format!("{}/branches/{}", repo.endpoint(), segment(&base)),
            None,
        )?;
        git(
            host,
            self.context.generation,
            GitOperation::Push {
                remote: repo.remote.clone(),
                branch: head.clone(),
            },
        )?;
        let existing = self.api.request(
            host,
            self.context.generation,
            "GET",
            &format!(
                "{}/pulls?state=open&head={}",
                repo.endpoint(),
                segment(&format!("{}:{head}", repo.owner))
            ),
            None,
        )?;
        if let Some(pr) = existing.as_array().and_then(|v| v.first()) {
            let number = pr["number"].as_u64().ok_or("Invalid PR response")?;
            let value = self.api.request(
                host,
                self.context.generation,
                "GET",
                &format!("{}/pulls/{number}", repo.endpoint()),
                None,
            )?;
            self.detail = Detail::Item { value, pull: true };
            self.notice = "Branch pushed; its existing pull request is open.".into();
            return Ok(());
        }
        if let Some(number) = self
            .stored
            .associations
            .get(&self.association(&repo, &head))
        {
            let closes = format!("Closes #{number}");
            if !body.contains(&closes) {
                body = format!("{}\n\n{closes}", body.trim()).trim().into();
            }
        }
        let value = self.api.request(
            host,
            self.context.generation,
            "POST",
            &format!("{}/pulls", repo.endpoint()),
            Some(json!({"title":title,"body":body,"head":head,"base":base})),
        )?;
        self.detail = Detail::Item { value, pull: true };
        self.notice = "Pull request created.".into();
        Ok(())
    }
    pub(crate) fn issue_branch(
        &mut self,
        host: &mut impl Host,
        action: &Action,
    ) -> Result<(), String> {
        let repo = self.repo()?;
        let Detail::Item { value, pull: false } = &self.detail else {
            return Err("Select an issue first".into());
        };
        let number = value["number"].as_u64().ok_or("Invalid issue")?;
        let issue_id = string(value, "node_id");
        let name = action.text("branch");
        let base = action.text("base");
        // Use Git's own branch-name validator before making a remote change.
        read(
            host,
            self.context.generation,
            &["check-ref-format", "--branch", &name],
        )?;
        read(
            host,
            self.context.generation,
            &["check-ref-format", "--branch", &base],
        )?;
        if name == base || name == string(&self.metadata, "default_branch") {
            return Err("Choose a new branch name different from the base/default branch".into());
        }
        let branch_url = format!("{}/branches/{}", repo.endpoint(), segment(&name));
        match self
            .api
            .request(host, self.context.generation, "GET", &branch_url, None)
        {
            Ok(_) if !action.checked("reuse") => {
                return Err(
                    "This branch already exists. Select ‘Reuse existing branch’ to checkout it."
                        .into(),
                );
            }
            Ok(_) => (),
            Err(error) if error.contains("404") => {
                let base = self.api.request(
                    host,
                    self.context.generation,
                    "GET",
                    &format!("{}/branches/{}", repo.endpoint(), segment(&base)),
                    None,
                )?;
                self.api.graphql(host, self.context.generation,
                    "mutation($issue:ID!,$oid:GitObjectID!,$name:String!){createLinkedBranch(input:{issueId:$issue,oid:$oid,name:$name}){linkedBranch{ref{name}}}}",
                    json!({"issue":issue_id,"oid":base["commit"]["sha"],"name":name}))?;
            }
            Err(error) => return Err(error),
        }
        // Remember association even if the checkout is cancelled or a fetch fails.
        self.stored
            .associations
            .insert(self.association(&repo, &name), number);
        self.store()?;
        git(
            host,
            self.context.generation,
            GitOperation::Fetch {
                remote: repo.remote.clone(),
                branch: name.clone(),
            },
        )?;
        git(
            host,
            self.context.generation,
            GitOperation::Checkout {
                branch: name.clone(),
                remote: repo.remote,
            },
        )?;
        self.branch = name;
        self.notice = "Issue branch checked out.".into();
        Ok(())
    }
    pub(crate) fn merge(&mut self, host: &mut impl Host, action: &Action) -> Result<(), String> {
        let repo = self.repo()?;
        let Detail::Item { value, pull: true } = &self.detail else {
            return Err("Select a pull request first".into());
        };
        let number = value["number"].as_u64().ok_or("Invalid pull request")?;
        let expected = string(&value["head"], "sha");
        let value = self.api.request(
            host,
            self.context.generation,
            "GET",
            &format!("{}/pulls/{number}", repo.endpoint()),
            None,
        )?;
        if value["head"]["sha"].as_str() != Some(&expected) {
            self.detail = Detail::Item { value, pull: true };
            return Err(
                "The PR head changed. Review the new head before requesting merge again.".into(),
            );
        }
        if value["merged"].as_bool() == Some(true) || !value["merged_at"].is_null() {
            return Err("This pull request is already merged".into());
        }
        let branch = string(&value["head"], "ref");
        let base = string(&value["base"], "ref");
        let cleanup = action.checked("delete");
        if cleanup
            && (value["head"]["repo"]["full_name"].as_str() != Some(&repo.slug())
                || branch == string(&self.metadata, "default_branch")
                || branch == base)
        {
            return Err("Automatic branch cleanup is available only for a non-default branch in this repository".into());
        }
        let mut merged_with_cli = false;
        if !self.api.no_gh {
            let reply = host.service(
                self.context.generation,
                Service::Run {
                    program: "gh".into(),
                    args: vec![
                        "pr".into(),
                        "merge".into(),
                        number.to_string(),
                        "--repo".into(),
                        format!("github.com/{}", repo.slug()),
                        "--auto".into(),
                        "--squash".into(),
                        "--match-head-commit".into(),
                        expected.clone(),
                    ],
                    input: None,
                    credential: true,
                },
            )?;
            if reply.code == "missing_tool" {
                self.api.no_gh = true;
            } else {
                reply.result()?;
                merged_with_cli = true;
            }
        }
        if !merged_with_cli {
            let id = string(&value, "node_id");
            let state = self.api.graphql(host, self.context.generation,
                "query($id:ID!){node(id:$id){... on PullRequest{isMergeQueueEnabled isInMergeQueue mergeStateStatus}}}", json!({"id":id}))?;
            let queued = state["node"]["isInMergeQueue"].as_bool() == Some(true);
            let queue = state["node"]["isMergeQueueEnabled"].as_bool() == Some(true);
            let clean = state["node"]["mergeStateStatus"].as_str() == Some("CLEAN");
            if !queued {
                let mutation = if queue && clean {
                    "mutation($id:ID!,$sha:GitObjectID!){enqueuePullRequest(input:{pullRequestId:$id,expectedHeadOid:$sha}){mergeQueueEntry{id}}}"
                } else if !queue && clean {
                    "mutation($id:ID!,$sha:GitObjectID!){mergePullRequest(input:{pullRequestId:$id,expectedHeadOid:$sha,mergeMethod:SQUASH}){pullRequest{merged}}}"
                } else {
                    "mutation($id:ID!,$sha:GitObjectID!){enablePullRequestAutoMerge(input:{pullRequestId:$id,expectedHeadOid:$sha,mergeMethod:SQUASH}){pullRequest{id}}}"
                };
                self.api.graphql(
                    host,
                    self.context.generation,
                    mutation,
                    json!({"id":id,"sha":expected}),
                )?;
            }
        }
        if cleanup {
            let record = Cleanup {
                repo: repo.clone(),
                workspace: self.context.workspace.clone().ok_or("Open a workspace")?,
                number,
                branch,
                sha: expected,
                base,
                local_done: false,
                remote_done: false,
            };
            self.stored.cleanup.retain(|item| {
                !(item.repo == repo && item.number == number && item.workspace == record.workspace)
            });
            self.stored.cleanup.push(record);
            self.store()?;
        }
        self.notice = "Merge requested. GitHub will apply its checks and protection rules.".into();
        self.check_cleanup(host)?;
        let value = self.api.request(
            host,
            self.context.generation,
            "GET",
            &format!("{}/pulls/{number}", repo.endpoint()),
            None,
        )?;
        self.detail = Detail::Item { value, pull: true };
        Ok(())
    }
    pub(crate) fn check_cleanup(&mut self, host: &mut impl Host) -> Result<(), String> {
        if self.stored.cleanup.is_empty() {
            return Ok(());
        }
        let mut pending = self.stored.cleanup.clone();
        for item in &mut pending {
            if Some(&item.workspace) != self.context.workspace.as_ref()
                || !self.repos.contains(&item.repo)
            {
                continue;
            }
            let value = self.api.request(
                host,
                self.context.generation,
                "GET",
                &format!("{}/pulls/{}", item.repo.endpoint(), item.number),
                None,
            )?;
            if value["merged_at"].is_null() {
                continue;
            }
            if value["head"]["sha"].as_str() != Some(&item.sha) {
                self.notice =
                    "Cleanup pending: PR head changed after the merge request; branches were kept."
                        .into();
                continue;
            }
            let metadata = self.api.request(
                host,
                self.context.generation,
                "GET",
                &item.repo.endpoint(),
                None,
            )?;
            if string(&metadata, "default_branch") == item.branch || item.branch == item.base {
                self.notice = "Cleanup skipped: branch is now the default/base branch.".into();
                continue;
            }
            if !item.remote_done {
                match self.api.request(
                    host,
                    self.context.generation,
                    "GET",
                    &format!(
                        "{}/branches/{}",
                        item.repo.endpoint(),
                        segment(&item.branch)
                    ),
                    None,
                ) {
                    Err(error) if error.contains("404") => item.remote_done = true,
                    Err(error) => return Err(error),
                    Ok(branch) if branch["commit"]["sha"].as_str() != Some(&item.sha) => {
                        self.notice =
                            "Cleanup pending: remote branch has new commits; kept it.".into();
                        continue;
                    }
                    Ok(_) => {
                        if let Err(error) =
                            check_push_destination(host, self.context.generation, &item.repo)
                        {
                            self.notice = error;
                            continue;
                        }
                        match git(
                            host,
                            self.context.generation,
                            GitOperation::DeleteRemote {
                                remote: item.repo.remote.clone(),
                                branch: item.branch.clone(),
                                expected: item.sha.clone(),
                            },
                        ) {
                            Ok(_) => item.remote_done = true,
                            Err(error) => {
                                self.notice = format!("Remote cleanup pending: {error}");
                                continue;
                            }
                        }
                    }
                }
            }
            if !item.local_done {
                let refs = read(host, self.context.generation, &["show-ref", "--heads"])?;
                if !refs.lines().any(|line| {
                    line.split_whitespace().nth(1) == Some(&format!("refs/heads/{}", item.branch))
                }) {
                    item.local_done = true;
                } else {
                    match git(
                        host,
                        self.context.generation,
                        GitOperation::DeleteLocal {
                            branch: item.branch.clone(),
                            expected: item.sha.clone(),
                            base: item.base.clone(),
                        },
                    ) {
                        Ok(_) => item.local_done = true,
                        Err(error) => self.notice = format!("Local cleanup pending: {error}"),
                    }
                }
            }
        }
        self.stored.cleanup = pending
            .into_iter()
            .filter(|i| !i.local_done || !i.remote_done)
            .collect();
        self.store()
    }
}
