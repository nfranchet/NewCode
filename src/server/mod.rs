//! An authenticated gateway to private, independently owned editor sessions.
mod config;
mod discovery;
mod workspaces;
use crate::browser::{self, registry};
use std::{sync::Arc, time::Duration};
use tiny_http::{Header, Method, Request, Response, Server};

pub(crate) fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args = config::Arguments::parse(std::env::args_os().skip(1))?;
    if args.version {
        println!("focus-editor-server {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    let config = Arc::new(config::Config::load(&args)?);
    let server = Arc::new(Server::http(config.address).map_err(std::io::Error::other)?);
    let _discovery = discovery::Discovery::start()?;
    let state = workspaces::State::new()?;
    crate::logging::init_server(args.level)?;
    // Import already-running editors as well as saved paths before the first page request.
    for record in registry::records()? {
        if let Some(path) = record.workspace {
            let _ = crate::workspaces::remember(&path);
        }
    }
    let _ = crate::workspaces::list()?;
    let address = server
        .server_addr()
        .to_ip()
        .ok_or("Invalid server address")?;
    println!("Focus Editor server listening on http://{address}");
    log::info!("Shared editor server listening on {address}");
    let mut workers = Vec::new();
    for _ in 0..8 {
        let server = server.clone();
        let config = config.clone();
        let state = state.clone();
        workers.push(std::thread::spawn(move || {
            let client = reqwest::blocking::Client::builder()
                .no_proxy()
                .timeout(Duration::from_secs(10))
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .unwrap();
            while let Ok(request) = server.recv() {
                handle(request, &config, &client, &state);
            }
        }));
    }
    for worker in workers {
        let _ = worker.join();
    }
    Ok(())
}

fn handle(
    mut request: Request,
    config: &config::Config,
    client: &reqwest::blocking::Client,
    state: &workspaces::State,
) {
    if !browser::header(&request, "Authorization")
        .is_some_and(|authorization| config.authorized(authorization))
    {
        let response = Response::from_string("Authentication required")
            .with_status_code(401)
            .with_header(
                Header::from_bytes(
                    "WWW-Authenticate",
                    "Basic realm=\"Focus Editor\", charset=\"UTF-8\"",
                )
                .unwrap(),
            )
            .with_header(Header::from_bytes("Cache-Control", "no-store").unwrap());
        let _ = request.respond(response);
        return;
    }
    if request.method() == &Method::Post {
        let same_origin = browser::header(&request, "Origin")
            .and_then(|value| reqwest::Url::parse(value).ok())
            .zip(browser::header(&request, "Host"))
            .is_some_and(|(origin, host)| {
                matches!(origin.scheme(), "http" | "https")
                    && reqwest::Url::parse(&format!("{}://{host}", origin.scheme()))
                        .is_ok_and(|url| url.origin() == origin.origin())
            });
        if !same_origin {
            browser::respond(request, 403, "text/plain", "Invalid request origin", false);
            return;
        }
    }
    if request.url() == "/api/workspaces" && request.method() == &Method::Get {
        let result = registry::records().and_then(|records| state.catalog(&records));
        match result {
            Ok(catalog) => browser::respond(
                request,
                200,
                "application/json",
                serde_json::to_vec(&catalog).unwrap(),
                true,
            ),
            Err(_) => browser::respond(
                request,
                503,
                "application/json",
                r#"{"error":"Cannot load workspaces"}"#,
                false,
            ),
        }
        return;
    }
    if request.url() == "/api/workspaces" && request.method() == &Method::Post {
        #[derive(serde::Deserialize)]
        struct Add {
            path: std::path::PathBuf,
        }
        let result = browser::read_body(&mut request, 16384)
            .ok()
            .and_then(|body| serde_json::from_slice::<Add>(&body).ok())
            .ok_or_else(|| "Enter a workspace folder path".to_owned())
            .and_then(|body| {
                if !body.path.is_absolute() {
                    return Err("Enter an absolute path on the editor host".into());
                }
                crate::workspaces::remember(&body.path)
                    .map_err(|error| format!("Cannot add workspace: {error}"))
            });
        match result {
            Ok(workspace) => browser::respond(
                request,
                201,
                "application/json",
                serde_json::to_vec(&workspace).unwrap(),
                false,
            ),
            Err(error) => browser::respond(
                request,
                400,
                "application/json",
                serde_json::json!({"error":error}).to_string(),
                false,
            ),
        }
        return;
    }
    if request.method() == &Method::Post && request.url().starts_with("/api/workspaces/") {
        let id = request
            .url()
            .strip_prefix("/api/workspaces/")
            .and_then(|path| path.strip_suffix("/start"));
        let workspace = crate::workspaces::list().ok().and_then(|workspaces| {
            workspaces
                .into_iter()
                .find(|workspace| Some(workspace.id.as_str()) == id)
        });
        let Some(workspace) = workspace else {
            browser::respond(
                request,
                404,
                "application/json",
                r#"{"error":"Unknown workspace"}"#,
                false,
            );
            return;
        };
        // Drain any request body with a small bound, including chunked requests.
        if browser::read_body(&mut request, 1024).is_err() {
            browser::respond(request, 413, "text/plain", "Invalid request size", false);
            return;
        }
        let result = registry::records()
            .map_err(|_| "Cannot read editor registrations".to_owned())
            .and_then(|records| state.start(workspace, &records));
        match result {
            Ok((status, body)) => {
                browser::respond(request, status, "application/json", body.to_string(), false)
            }
            Err(error) => browser::respond(
                request,
                503,
                "application/json",
                serde_json::json!({"error":error}).to_string(),
                false,
            ),
        }
        return;
    }
    if request.method() == &Method::Get {
        let asset = match request.url() {
            "/" => Some(("text/html; charset=utf-8", include_str!("hub.html"))),
            "/hub.js" => Some(("text/javascript; charset=utf-8", include_str!("hub.js"))),
            "/hub.css" => Some(("text/css; charset=utf-8", include_str!("hub.css"))),
            "/client.js" | "/browser.css" => browser::asset(request.url()),
            path if editor_path(path).is_some_and(|(_, action)| action.is_empty()) => {
                browser::asset("/")
            }
            _ => None,
        };
        if let Some((mime, body)) = asset {
            browser::respond(request, 200, mime, body, true);
            return;
        }
        if request.url() == "/api/editors" {
            match registry::records() {
                Ok(records) => {
                    let catalog: Vec<_> = records.iter().map(|record| serde_json::json!({"id": record.id, "title": record.title, "workspace": record.workspace, "mode": record.mode})).collect();
                    browser::respond(
                        request,
                        200,
                        "application/json",
                        serde_json::to_vec(&catalog).unwrap(),
                        true,
                    );
                }
                Err(_) => browser::respond(
                    request,
                    503,
                    "text/plain",
                    "Cannot read editor registrations",
                    false,
                ),
            }
            return;
        }
    }
    let Some((id, action)) =
        editor_path(request.url()).map(|(id, action)| (id.to_owned(), action.to_owned()))
    else {
        browser::respond(request, 404, "text/plain", "Not found", false);
        return;
    };
    if request.method() != &Method::Post
        || !matches!(
            action.as_str(),
            "connect" | "frame" | "claim" | "disconnect" | "stop"
        )
    {
        browser::respond(request, 404, "text/plain", "Not found", false);
        return;
    }
    let viewer = if action == "stop" {
        String::new()
    } else {
        let Some(viewer) = browser::header(&request, "X-Focus-Client")
            .filter(|id| !id.is_empty() && id.len() <= 100)
            .map(str::to_owned)
        else {
            browser::respond(request, 400, "text/plain", "Missing viewer identity", false);
            return;
        };
        viewer
    };
    let body = match browser::read_body(&mut request, 1024 * 1024) {
        Ok(body) => body,
        Err(_) => {
            browser::respond(request, 413, "text/plain", "Invalid request size", false);
            return;
        }
    };
    let Some(record) = registry::records()
        .ok()
        .and_then(|records| records.into_iter().find(|record| record.id == id))
    else {
        browser::respond(request, 410, "text/plain", "This editor has closed", false);
        return;
    };
    if action == "stop" && record.mode != "headless" {
        browser::respond(
            request,
            403,
            "application/json",
            r#"{"error":"Close local editors from their own window"}"#,
            false,
        );
        return;
    }
    let upstream = if action == "stop" {
        "manage/stop"
    } else {
        action.as_str()
    };
    let result = client
        .post(format!(
            "{}/api/{upstream}",
            record.endpoint.trim_end_matches('/')
        ))
        .header("Origin", &record.endpoint)
        .header("X-Focus-Token", &record.token)
        .header("X-Focus-Client", format!("remote-{viewer}"))
        .header("X-Focus-Remote", "1")
        .header("Content-Type", "application/json")
        .body(body)
        .send();
    match result {
        Ok(response) => {
            let status = response.status().as_u16();
            match response.bytes() {
                Ok(body) => {
                    browser::respond(request, status, "application/json", body.to_vec(), true)
                }
                Err(_) => browser::respond(
                    request,
                    502,
                    "text/plain",
                    "Editor connection interrupted",
                    false,
                ),
            }
        }
        Err(_) => browser::respond(
            request,
            502,
            "text/plain",
            "Editor is unavailable or busy. Reconnect to continue.",
            false,
        ),
    }
}

fn editor_path(path: &str) -> Option<(&str, &str)> {
    let path = path
        .strip_prefix("/api/editors/")
        .or_else(|| path.strip_prefix("/editors/"))?;
    let (id, action) = path.split_once('/').unwrap_or((path, ""));
    (id.len() == 48 && id.bytes().all(|byte| byte.is_ascii_hexdigit())).then_some((id, action))
}

#[cfg(test)]
mod tests;
