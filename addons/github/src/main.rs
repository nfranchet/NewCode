mod api;
#[cfg(test)]
mod tests;
mod view;
mod workflows;

use api::{Api, Host, read};
use focus_addon_sdk::{Action, Connection, Context, Message, PROTOCOL_VERSION};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs, io,
    path::PathBuf,
    time::{Duration, Instant},
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Repo {
    remote: String,
    owner: String,
    name: String,
}
impl Repo {
    fn slug(&self) -> String {
        format!("{}/{}", self.owner, self.name)
    }
    fn endpoint(&self) -> String {
        format!("repos/{}", self.slug())
    }
}
fn parse_remote(remote: &str, url: &str) -> Option<Repo> {
    let path = if let Some(path) = url.strip_prefix("git@github.com:") {
        path.to_owned()
    } else {
        let parsed = url::Url::parse(url).ok()?;
        if parsed.host_str()? != "github.com" || !matches!(parsed.scheme(), "https" | "ssh") {
            return None;
        }
        parsed.path().trim_start_matches('/').to_owned()
    };
    let path = path.trim_end_matches('/').trim_end_matches(".git");
    let (owner, name) = path.split_once('/')?;
    if owner.is_empty()
        || name.is_empty()
        || name.contains('/')
        || !owner
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        || !name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
    {
        return None;
    }
    Some(Repo {
        remote: remote.into(),
        owner: owner.into(),
        name: name.into(),
    })
}

#[derive(Clone, Serialize, Deserialize)]
struct Cleanup {
    repo: Repo,
    workspace: PathBuf,
    number: u64,
    branch: String,
    sha: String,
    base: String,
    #[serde(default)]
    local_done: bool,
    #[serde(default)]
    remote_done: bool,
}
#[derive(Default, Serialize, Deserialize)]
struct Stored {
    #[serde(default)]
    associations: BTreeMap<String, u64>,
    #[serde(default)]
    cleanup: Vec<Cleanup>,
    #[serde(default)]
    remotes: BTreeMap<PathBuf, String>,
}
enum Detail {
    Item {
        value: Value,
        pull: bool,
    },
    CreatePr {
        head: String,
        title: String,
        body: String,
    },
    None,
}
struct App {
    context: Context,
    api: Api,
    repos: Vec<Repo>,
    selected: usize,
    metadata: Value,
    items: Vec<Value>,
    pulls: bool,
    page: u32,
    more: bool,
    query: String,
    detail: Detail,
    notice: String,
    stored: Stored,
    refreshed: Option<Instant>,
    cooldown: Option<Instant>,
    branch: String,
}
impl Default for App {
    fn default() -> Self {
        Self {
            context: Context::default(),
            api: Api::default(),
            repos: vec![],
            selected: 0,
            metadata: Value::Null,
            items: vec![],
            pulls: false,
            page: 1,
            more: false,
            query: String::new(),
            detail: Detail::None,
            notice: String::new(),
            stored: Stored::default(),
            refreshed: None,
            cooldown: None,
            branch: String::new(),
        }
    }
}
impl App {
    fn repo(&self) -> Result<Repo, String> {
        self.repos
            .get(self.selected)
            .cloned()
            .ok_or_else(|| "The workspace has no GitHub.com remote".into())
    }
    fn association(&self, repo: &Repo, branch: &str) -> String {
        format!(
            "{}:{}:{branch}",
            self.context
                .workspace
                .as_deref()
                .unwrap_or(std::path::Path::new(""))
                .display(),
            repo.slug()
        )
    }
    fn store(&self) -> Result<(), String> {
        fs::create_dir_all(&self.context.storage).map_err(|e| e.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&self.context.storage, fs::Permissions::from_mode(0o700))
                .map_err(|e| e.to_string())?;
        }
        let path = self.context.storage.join("state.json");
        let temporary = path.with_extension("tmp");
        fs::write(
            &temporary,
            serde_json::to_vec_pretty(&self.stored).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        fs::rename(temporary, path).map_err(|e| e.to_string())
    }
    fn reset(&mut self, context: Context, host: &mut impl Host) -> Result<(), String> {
        let api = std::mem::take(&mut self.api);
        *self = Self {
            context,
            api,
            ..Self::default()
        };
        self.stored = fs::read(self.context.storage.join("state.json"))
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        if self.context.workspace.is_none() {
            return Ok(());
        }
        let names = read(host, self.context.generation, &["remote"])?;
        for remote in names.lines() {
            let url = read(
                host,
                self.context.generation,
                &["remote", "get-url", remote],
            )?;
            if let Some(repo) = parse_remote(remote, url.trim()) {
                self.repos.push(repo);
            }
        }
        let preferred = self
            .context
            .workspace
            .as_ref()
            .and_then(|w| self.stored.remotes.get(w))
            .map(String::as_str)
            .unwrap_or("origin");
        self.selected = self
            .repos
            .iter()
            .position(|r| r.remote == preferred)
            .unwrap_or(0);
        if self.repos.is_empty() {
            return Ok(());
        }
        self.load_repository(host)
    }
    fn load_repository(&mut self, host: &mut impl Host) -> Result<(), String> {
        let repo = self.repo()?;
        self.items.clear();
        self.page = 1;
        self.detail = Detail::None;
        self.metadata =
            self.api
                .request(host, self.context.generation, "GET", &repo.endpoint(), None)?;
        let default = self.metadata["default_branch"].clone();
        self.metadata["_base"] = default.clone();
        if default.as_str() != Some("main") {
            match self.api.request(
                host,
                self.context.generation,
                "GET",
                &format!("{}/branches/main", repo.endpoint()),
                None,
            ) {
                Ok(_) => self.metadata["_base"] = json!("main"),
                Err(error) if error.contains("404") => (),
                Err(error) => return Err(error),
            }
        }
        self.branch = read(
            host,
            self.context.generation,
            &["rev-parse", "--abbrev-ref", "HEAD"],
        )?
        .trim()
        .to_owned();
        if let Some(workspace) = &self.context.workspace {
            self.stored.remotes.insert(workspace.clone(), repo.remote);
            self.store()?;
        }
        self.refresh(host, false)
    }
    fn refresh(&mut self, host: &mut impl Host, more: bool) -> Result<(), String> {
        if self.cooldown.is_some_and(|until| until > Instant::now()) {
            return Err(
                "GitHub requests are paused until its rate-limit retry time. Try again later."
                    .into(),
            );
        }
        let repo = self.repo()?;
        let page = if more { self.page + 1 } else { 1 };
        let kind = if self.pulls { "pulls" } else { "issues" };
        let value = self.api.request(
            host,
            self.context.generation,
            "GET",
            &format!(
                "{}/{kind}?state=open&sort=updated&direction=desc&per_page=50&page={page}",
                repo.endpoint()
            ),
            None,
        )?;
        let values = value.as_array().ok_or("Invalid GitHub list response")?;
        self.more = values.len() == 50;
        if !more {
            self.items.clear();
        }
        for value in values {
            if !self.pulls && value.get("pull_request").is_some() {
                continue;
            }
            if !self.items.iter().any(|v| v["number"] == value["number"]) {
                self.items.push(value.clone());
            }
        }
        self.page = page;
        self.refreshed = Some(Instant::now());
        self.branch = read(
            host,
            self.context.generation,
            &["rev-parse", "--abbrev-ref", "HEAD"],
        )?
        .trim()
        .to_owned();
        Ok(())
    }
    fn perform(&mut self, host: &mut impl Host, action: Action) -> Result<(), String> {
        if action.generation != self.context.generation {
            return Ok(());
        }
        self.notice.clear();
        match action.id.as_str() {
            "issues" | "pulls" => {
                self.pulls = action.id == "pulls";
                self.detail = Detail::None;
                self.refresh(host, false)?;
            }
            "repository" => {
                let remote = action.text("remote");
                self.selected = self
                    .repos
                    .iter()
                    .position(|r| r.remote == remote)
                    .ok_or("Select a repository")?;
                self.load_repository(host)?;
            }
            "refresh" => {
                self.refresh(host, false)?;
                self.check_cleanup(host)?;
            }
            "more" => self.refresh(host, true)?,
            "filter" => self.query = action.text("query"),
            "new_pr" => self.prepare_pr(host)?,
            "create_pr" => self.create_pr(host, &action)?,
            "branch" => self.issue_branch(host, &action)?,
            "merge" => self.merge(host, &action)?,
            "cleanup" => self.check_cleanup(host)?,
            id if id.starts_with("item:") => {
                let number: u64 = id[5..].parse().map_err(|_| "Invalid issue number")?;
                let repo = self.repo()?;
                let kind = if self.pulls { "pulls" } else { "issues" };
                let value = self.api.request(
                    host,
                    self.context.generation,
                    "GET",
                    &format!("{}/{kind}/{number}", repo.endpoint()),
                    None,
                )?;
                self.detail = Detail::Item {
                    value,
                    pull: self.pulls,
                };
            }
            _ => (),
        }
        Ok(())
    }
    fn report(&mut self, result: Result<(), String>) {
        if let Err(error) = result {
            if error.contains("rate limit") || error.contains("Rate limit") {
                let seconds = error
                    .split("Retry after ")
                    .nth(1)
                    .and_then(|s| s.split_whitespace().next())
                    .and_then(|s| s.parse::<u64>().ok())
                    .unwrap_or(60)
                    .clamp(1, 86400);
                self.cooldown = Some(Instant::now() + Duration::from_secs(seconds));
            }
            self.notice = error;
        }
    }
}

fn run() -> Result<(), String> {
    let mut connection = Connection::new(io::BufReader::new(io::stdin()), io::stdout());
    let mut app = App::default();
    while let Some(message) = connection.receive().map_err(|e| e.to_string())? {
        let mut reveal_detail = false;
        match message.method.as_deref() {
            Some("initialize") => {
                let params = message.params.unwrap_or_default();
                if params["protocol"].as_u64() != Some(PROTOCOL_VERSION as u64) {
                    return Err("Unsupported host protocol".into());
                }
                connection
                    .send(Message::response(
                        message.id.unwrap_or(0),
                        json!({"protocol":PROTOCOL_VERSION}),
                    ))
                    .map_err(|e| e.to_string())?;
                let context =
                    serde_json::from_value(params["context"].clone()).map_err(|e| e.to_string())?;
                let result = app.reset(context, &mut connection);
                app.report(result);
            }
            Some("context.changed") => {
                let context = serde_json::from_value(message.params.unwrap_or_default())
                    .map_err(|e| e.to_string())?;
                let result = app.reset(context, &mut connection);
                app.report(result);
            }
            Some("action") => {
                let action: Action = serde_json::from_value(message.params.unwrap_or_default())
                    .map_err(|e| e.to_string())?;
                reveal_detail = action.id.starts_with("item:")
                    || matches!(action.id.as_str(), "new_pr" | "create_pr");
                connection.publish(&app.view(true))?;
                let result = app.perform(&mut connection, action);
                app.report(result);
            }
            Some("tick") => {
                let visible = message
                    .params
                    .as_ref()
                    .and_then(|v| v["visible"].as_bool())
                    .unwrap_or(false);
                if !app.repos.is_empty() && !app.cooldown.is_some_and(|t| t > Instant::now()) {
                    let result = app.check_cleanup(&mut connection);
                    app.report(result);
                    if visible
                        && app
                            .refreshed
                            .is_some_and(|t| t.elapsed() > Duration::from_secs(60))
                    {
                        let result = app.refresh(&mut connection, false);
                        app.report(result);
                    }
                }
            }
            Some("shutdown") => break,
            _ => (),
        }
        let mut view = app.view(false);
        view.reveal_detail = reveal_detail;
        connection.publish(&view)?;
    }
    Ok(())
}
fn main() {
    if std::env::args().any(|argument| argument == "--version") {
        println!("focus-addon-github {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    if let Err(error) = run() {
        eprintln!("GitHub addon stopped: {error}");
        std::process::exit(1);
    }
}
