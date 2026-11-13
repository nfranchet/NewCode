//! Shared editor instances and their private loopback bridges.
mod launcher;
mod native;
mod protocol;
pub(crate) mod registry;
mod renderer;
mod runtime;

use crate::logging::Arguments;
use flate2::{Compression, write::GzEncoder};
use std::{
    io::{Read, Write},
    time::{Duration, Instant},
};
use tiny_http::{Header, Method, Request, Response, Server};

const MAX_INPUT: usize = 1024 * 1024;
const RECONNECT_GRACE: Duration = Duration::from_secs(60);

pub(crate) fn validate_runtime() -> std::io::Result<()> {
    launcher::browser_executable().map(|_| ())
}
pub(crate) fn run_native(arguments: &Arguments) -> Result<(), Box<dyn std::error::Error>> {
    native::run(arguments)
}

pub(crate) fn external_url(value: &str) -> Result<reqwest::Url, &'static str> {
    let url = reqwest::Url::parse(value).map_err(|_| "Invalid external URL")?;
    if !matches!(url.scheme(), "https" | "http" | "mailto") {
        return Err("Unsupported external URL scheme");
    }
    Ok(url)
}

pub(crate) fn header<'a>(request: &'a Request, name: &'static str) -> Option<&'a str> {
    let mut matching = request.headers().iter().filter(|h| h.field.equiv(name));
    let value = matching.next()?.value.as_str();
    matching.next().is_none().then_some(value)
}

fn authorized(request: &Request, host: &str, origin: &str, token: &str) -> bool {
    header(request, "Host") == Some(host)
        && header(request, "Origin") == Some(origin)
        && header(request, "X-Focus-Token") == Some(token)
}

pub(crate) fn respond(
    request: Request,
    status: u16,
    mime: &str,
    body: impl Into<Vec<u8>>,
    gzip: bool,
) {
    let mut body = body.into();
    let mut headers = vec![
        Header::from_bytes("Content-Type", mime).unwrap(),
        Header::from_bytes("Cache-Control", "no-store").unwrap(),
        Header::from_bytes("X-Content-Type-Options", "nosniff").unwrap(),
        Header::from_bytes("Referrer-Policy", "no-referrer").unwrap(),
        Header::from_bytes("Content-Security-Policy", "default-src 'none'; script-src 'self'; style-src 'self'; connect-src 'self'; img-src 'self' blob: data:; frame-src 'self'; frame-ancestors 'self'; base-uri 'none'; form-action 'none'").unwrap(),
    ];
    if gzip
        && body.len() > 1024
        && header(&request, "Accept-Encoding").is_some_and(|v| v.contains("gzip"))
    {
        let mut encoder = GzEncoder::new(Vec::new(), Compression::fast());
        if encoder.write_all(&body).is_ok() {
            if let Ok(compressed) = encoder.finish() {
                body = compressed;
                headers.push(Header::from_bytes("Content-Encoding", "gzip").unwrap());
            }
        }
    }
    let mut response = Response::from_data(body).with_status_code(status);
    for header in headers {
        response.add_header(header);
    }
    let _ = request.respond(response);
}

pub(crate) fn read_body(request: &mut Request, limit: usize) -> Result<Vec<u8>, &'static str> {
    if request.body_length().is_some_and(|size| size > limit) {
        return Err("Invalid request size");
    }
    let mut body = Vec::new();
    request
        .as_reader()
        .take((limit + 1) as u64)
        .read_to_end(&mut body)
        .map_err(|_| "Cannot read request")?;
    if body.len() > limit {
        return Err("Request too large");
    }
    Ok(body)
}

pub(crate) fn asset(path: &str) -> Option<(&'static str, &'static str)> {
    match path {
        "/" => Some(("text/html; charset=utf-8", include_str!("index.html"))),
        "/client.js" => Some(("text/javascript; charset=utf-8", include_str!("client.js"))),
        "/browser.css" => Some(("text/css; charset=utf-8", include_str!("browser.css"))),
        _ => None,
    }
}

struct Host {
    runtime: runtime::Runtime,
    registration: registry::Registration,
    host: String,
    origin: String,
    token: String,
    last_seen: Instant,
    last_save: Instant,
    close_prompt: bool,
    stop: bool,
}

impl Host {
    fn new(
        arguments: &Arguments,
        endpoint: &str,
        token: &str,
        mode: &str,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let runtime = runtime::Runtime::new(arguments);
        let registration = registry::Registration::new(
            endpoint.into(),
            token.into(),
            mode,
            runtime.renderer.workspace(),
        )?;
        Ok(Self {
            runtime,
            registration,
            host: endpoint.trim_start_matches("http://").into(),
            origin: endpoint.into(),
            token: token.into(),
            last_seen: Instant::now(),
            last_save: Instant::now(),
            close_prompt: false,
            stop: false,
        })
    }
    fn tick(&mut self) {
        self.runtime.tick();
        self.registration.update(
            self.runtime.renderer.title(),
            self.runtime.renderer.workspace(),
        );
        if self.last_save.elapsed() > Duration::from_secs(10) {
            self.runtime.renderer.save();
            self.last_save = Instant::now();
        }
    }
    fn handle(&mut self, mut request: Request) {
        if header(&request, "Host") != Some(self.host.as_str()) {
            respond(request, 403, "text/plain", "Invalid host", false);
            return;
        }
        if request.method() == &Method::Get && !request.url().starts_with("/api/") {
            if let Some((mime, body)) = asset(request.url()) {
                respond(request, 200, mime, body, true);
            } else {
                respond(request, 404, "text/plain", "Not found", false);
            }
            return;
        }
        if !authorized(&request, &self.host, &self.origin, &self.token) {
            respond(request, 403, "text/plain", "Invalid editor session", false);
            return;
        }
        if request.method() == &Method::Get && request.url() == "/api/info" {
            let body = serde_json::json!({"id": self.registration.record.id, "version": 1, "title": self.runtime.renderer.title()});
            respond(request, 200, "application/json", body.to_string(), false);
            return;
        }
        if request.method() != &Method::Post {
            respond(request, 405, "text/plain", "Method not allowed", false);
            return;
        }
        let Some(client) = header(&request, "X-Focus-Client")
            .filter(|id| !id.is_empty() && id.len() <= 128)
            .map(str::to_owned)
        else {
            respond(request, 400, "text/plain", "Missing viewer identity", false);
            return;
        };
        if request.url() == "/api/connect" {
            let remote = header(&request, "X-Focus-Remote") == Some("1");
            let control = self.runtime.connect(&client, remote);
            self.last_seen = Instant::now();
            self.close_prompt = false;
            respond(
                request,
                200,
                "application/json",
                serde_json::to_vec(&control).unwrap(),
                false,
            );
            return;
        }
        if !self.runtime.known(&client) {
            respond(
                request,
                409,
                "text/plain",
                "Viewer session expired. Reconnect to continue.",
                false,
            );
            return;
        }
        match request.url() {
            "/api/claim" => {
                let control = self.runtime.claim(&client);
                respond(
                    request,
                    200,
                    "application/json",
                    serde_json::to_vec(&control).unwrap(),
                    false,
                );
            }
            "/api/frame" => {
                let result = read_body(&mut request, MAX_INPUT)
                    .ok()
                    .and_then(|body| serde_json::from_slice::<protocol::Input>(&body).ok())
                    .ok_or_else(|| "Invalid editor input".to_string())
                    .and_then(|input| self.runtime.frame(&client, input));
                match result {
                    Ok(Some(frame)) => respond(
                        request,
                        200,
                        "application/json",
                        serde_json::to_vec(&frame).unwrap(),
                        true,
                    ),
                    Ok(None) => respond(request, 204, "application/json", "", false),
                    Err(_) => {
                        respond(request, 400, "text/plain", "Invalid editor input", false);
                        return;
                    }
                }
                self.last_seen = Instant::now();
                self.close_prompt = false;
            }
            "/api/disconnect" => {
                self.runtime.disconnect(&client);
                respond(request, 200, "application/json", "{}", false);
            }
            "/api/close-prompt" => {
                if !self.runtime.remote(&client) {
                    self.close_prompt = true;
                    self.last_seen = Instant::now();
                }
                respond(request, 200, "application/json", "{}", false);
            }
            "/api/open-url" => {
                let url = read_body(&mut request, 16384)
                    .ok()
                    .and_then(|body| serde_json::from_slice::<serde_json::Value>(&body).ok())
                    .and_then(|value| value["url"].as_str().and_then(|url| external_url(url).ok()));
                let Some(url) = url else {
                    respond(request, 400, "text/plain", "Invalid external URL", false);
                    return;
                };
                if self.runtime.remote(&client) {
                    respond(
                        request,
                        403,
                        "text/plain",
                        "Open remote links in your own browser",
                        false,
                    );
                    return;
                }
                if webbrowser::open(url.as_str()).is_ok() {
                    respond(request, 200, "application/json", "{}", false);
                } else {
                    respond(request, 500, "text/plain", "Could not open the link", false);
                }
            }
            "/api/shutdown" if !self.runtime.remote(&client) => {
                self.stop = true;
                respond(request, 200, "application/json", "{}", false);
            }
            _ => respond(request, 404, "text/plain", "Not found", false),
        }
        self.registration.update(
            self.runtime.renderer.title(),
            self.runtime.renderer.workspace(),
        );
    }
}
impl Drop for Host {
    fn drop(&mut self) {
        self.runtime.renderer.save();
    }
}

pub(crate) fn run(arguments: &Arguments) -> Result<(), Box<dyn std::error::Error>> {
    let server = Server::http("127.0.0.1:0").map_err(std::io::Error::other)?;
    let origin = format!(
        "http://{}",
        server.server_addr().to_ip().ok_or("Invalid listener")?
    );
    let token = registry::random_id()?;
    let url = format!("{origin}/#{token}");
    if !std::env::args_os().any(|arg| arg == "--focus-editor-detached") {
        println!("Focus Editor: {url}");
    }
    log::info!("Browser host listening at {origin}");
    let mut window = launcher::AppWindow::start(&url)?;
    serve(
        server,
        arguments,
        origin.trim_start_matches("http://"),
        &origin,
        &token,
        RECONNECT_GRACE,
        Some(&mut window),
    )
}

fn serve(
    server: Server,
    arguments: &Arguments,
    _host: &str,
    origin: &str,
    token: &str,
    grace: Duration,
    mut window: Option<&mut launcher::AppWindow>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut host = Host::new(arguments, origin, token, "app")?;
    let mut failure = None;
    while !host.stop && (window.is_some() || host.last_seen.elapsed() < grace || host.close_prompt)
    {
        if let Some(window) = window.as_deref_mut() {
            match window.poll() {
                Ok(Some(status)) => {
                    if !status.success() {
                        failure = Some(window.exit_error(status));
                    }
                    break;
                }
                Err(error) => {
                    failure = Some(error);
                    break;
                }
                Ok(None) => {}
            }
        }
        if let Some(request) = server.recv_timeout(Duration::from_millis(30))? {
            host.handle(request);
        }
        host.tick();
    }
    log::info!("Editor session stopped");
    if let Some(error) = failure {
        return Err(error.into());
    }
    Ok(())
}

#[cfg(test)]
mod tests;
