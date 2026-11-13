//! An authenticated gateway to private, independently owned editor sessions.
mod config;
use crate::{
    browser::{self, registry},
    logging::Arguments,
};
use std::{sync::Arc, time::Duration};
use tiny_http::{Header, Method, Request, Response, Server};

pub(crate) fn run(args: &Arguments) -> Result<(), Box<dyn std::error::Error>> {
    let config = Arc::new(config::Config::load(args)?);
    let server = Arc::new(Server::http(config.address).map_err(std::io::Error::other)?);
    crate::logging::init(args)?;
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
        workers.push(std::thread::spawn(move || {
            let client = reqwest::blocking::Client::builder()
                .no_proxy()
                .timeout(Duration::from_secs(10))
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .unwrap();
            while let Ok(request) = server.recv() {
                handle(request, &config, &client);
            }
        }));
    }
    for worker in workers {
        let _ = worker.join();
    }
    Ok(())
}

fn handle(mut request: Request, config: &config::Config, client: &reqwest::blocking::Client) {
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
            "connect" | "frame" | "claim" | "disconnect"
        )
    {
        browser::respond(request, 404, "text/plain", "Not found", false);
        return;
    }
    let Some(viewer) = browser::header(&request, "X-Focus-Client")
        .filter(|id| !id.is_empty() && id.len() <= 100)
        .map(str::to_owned)
    else {
        browser::respond(request, 400, "text/plain", "Missing viewer identity", false);
        return;
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
    let result = client
        .post(format!(
            "{}/api/{action}",
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
