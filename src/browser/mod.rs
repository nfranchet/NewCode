//! Local browser host. Editor state and all filesystem/process access stay in Rust.
mod launcher;
mod protocol;
mod renderer;

use crate::logging::Arguments;
use flate2::{Compression, write::GzEncoder};
use std::{
    io::{Read, Write},
    time::{Duration, Instant},
};
use tiny_http::{Header, Method, Request, Response, Server};

const MAX_INPUT: usize = 1024 * 1024;
const RECONNECT_GRACE: Duration = Duration::from_secs(60);
const CLIENT_TIMEOUT: Duration = Duration::from_secs(5);

pub(crate) fn validate_runtime() -> std::io::Result<()> {
    launcher::browser_executable().map(|_| ())
}

fn external_url(value: &str) -> Result<reqwest::Url, &'static str> {
    let url = reqwest::Url::parse(value).map_err(|_| "Invalid external URL")?;
    if !matches!(url.scheme(), "https" | "http" | "mailto") {
        return Err("Unsupported external URL scheme");
    }
    Ok(url)
}

struct Session {
    client: Option<String>,
    last_seen: Instant,
    reset_textures: bool,
    close_prompt: bool,
}

impl Session {
    fn connect(&mut self, client: &str, now: Instant) -> bool {
        if self
            .client
            .as_deref()
            .is_some_and(|active| active != client)
            && now.duration_since(self.last_seen) < CLIENT_TIMEOUT
        {
            return false;
        }
        self.client = Some(client.to_owned());
        self.last_seen = now;
        self.reset_textures = true;
        self.close_prompt = false;
        true
    }

    fn active(&self, client: &str) -> bool {
        self.client.as_deref() == Some(client)
    }
}

fn header<'a>(request: &'a Request, name: &'static str) -> Option<&'a str> {
    let mut matching = request.headers().iter().filter(|h| h.field.equiv(name));
    let value = matching.next()?.value.as_str();
    matching.next().is_none().then_some(value)
}

fn authorized(request: &Request, host: &str, origin: &str, token: &str) -> bool {
    header(request, "Host") == Some(host)
        && header(request, "Origin") == Some(origin)
        && header(request, "X-Focus-Token") == Some(token)
}

fn respond(request: Request, status: u16, mime: &str, body: impl Into<Vec<u8>>, gzip: bool) {
    let mut body = body.into();
    let mut headers = vec![
        Header::from_bytes("Content-Type", mime).unwrap(),
        Header::from_bytes("Cache-Control", "no-store").unwrap(),
        Header::from_bytes("X-Content-Type-Options", "nosniff").unwrap(),
        Header::from_bytes("Referrer-Policy", "no-referrer").unwrap(),
        Header::from_bytes("Content-Security-Policy", "default-src 'none'; script-src 'self'; style-src 'self'; connect-src 'self'; img-src 'self' blob: data:; frame-ancestors 'none'; base-uri 'none'; form-action 'none'").unwrap(),
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
    if let Err(error) = request.respond(response) {
        log::debug!("Browser response disconnected: {error}");
    }
}

pub(crate) fn run(arguments: &Arguments) -> Result<(), Box<dyn std::error::Error>> {
    let server = Server::http("127.0.0.1:0").map_err(std::io::Error::other)?;
    let host = server
        .server_addr()
        .to_ip()
        .ok_or("Invalid browser listener address")?
        .to_string();
    let origin = format!("http://{host}");
    let mut random = [0; 32];
    getrandom::fill(&mut random).map_err(std::io::Error::other)?;
    let token: String = random.iter().map(|b| format!("{b:02x}")).collect();
    let url = format!("{origin}/#{token}");
    // The fragment never goes to HTTP logs or external sites. Do not log the secret URL.
    if !std::env::args_os().any(|arg| arg == "--focus-editor-detached") {
        println!("Focus Editor: {url}");
    }
    log::info!("Browser host listening at {origin}");
    let mut window = launcher::AppWindow::start(&url)?;
    serve(
        server,
        arguments,
        &host,
        &origin,
        &token,
        RECONNECT_GRACE,
        Some(&mut window),
    )
}

fn serve(
    server: Server,
    arguments: &Arguments,
    host: &str,
    origin: &str,
    token: &str,
    grace: Duration,
    mut window: Option<&mut launcher::AppWindow>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut renderer = renderer::Renderer::new(arguments);
    let start = Instant::now();
    let mut session = Session {
        client: None,
        last_seen: start,
        reset_textures: true,
        close_prompt: false,
    };
    let mut last_save = start;
    let mut failure = None;
    // Leave time for browser startup, reloads, and short connection interruptions.
    while session.last_seen.elapsed() < grace || (session.close_prompt && window.is_some()) {
        if let Some(window) = window.as_deref_mut() {
            match window.poll() {
                Ok(Some(status)) => {
                    if !status.success() {
                        failure = Some(window.exit_error(status));
                    }
                    break;
                }
                Ok(None) => {}
                Err(error) => {
                    failure = Some(error);
                    break;
                }
            }
        }
        let Some(mut request) = server.recv_timeout(Duration::from_millis(100))? else {
            renderer.background();
            continue;
        };
        if header(&request, "Host") != Some(host) {
            respond(request, 403, "text/plain", "Invalid host", false);
            continue;
        }
        if request.method() == &Method::Get {
            let asset = match request.url() {
                "/" => Some(("text/html; charset=utf-8", include_str!("index.html"))),
                "/client.js" => Some(("text/javascript; charset=utf-8", include_str!("client.js"))),
                "/browser.css" => Some(("text/css; charset=utf-8", include_str!("browser.css"))),
                _ => None,
            };
            if let Some((mime, body)) = asset {
                respond(request, 200, mime, body, true);
            } else {
                respond(request, 404, "text/plain", "Not found", false);
            }
            continue;
        }
        if request.method() != &Method::Post || !authorized(&request, host, origin, token) {
            respond(request, 403, "text/plain", "Invalid browser session", false);
            continue;
        }
        let Some(client) = header(&request, "X-Focus-Client")
            .filter(|s| !s.is_empty() && s.len() <= 128)
            .map(str::to_owned)
        else {
            respond(request, 400, "text/plain", "Missing browser client", false);
            continue;
        };
        if request.url() == "/api/connect" {
            if session.connect(&client, Instant::now()) {
                renderer.release_input();
                session.last_seen = Instant::now();
                respond(request, 200, "application/json", "{}", false);
            } else {
                respond(
                    request,
                    409,
                    "text/plain",
                    "This editor is already open in another tab. Close that tab, then reconnect.",
                    false,
                );
            }
            continue;
        }
        if !session.active(&client) {
            respond(
                request,
                409,
                "text/plain",
                "Browser session changed. Reconnect to continue.",
                false,
            );
            continue;
        }
        match request.url() {
            "/api/close-prompt" => {
                // Chromium suspends page timers while an unload confirmation is open.
                // Keep unsaved buffers until the user responds or closes the owned window.
                session.close_prompt = true;
                session.last_seen = Instant::now();
                respond(request, 200, "application/json", "{}", false);
            }
            "/api/open-url" => {
                if !request.body_length().is_some_and(|n| n <= 16384) {
                    respond(
                        request,
                        413,
                        "text/plain",
                        "Link request is too large",
                        false,
                    );
                    continue;
                }
                let mut body = Vec::new();
                let parsed = request
                    .as_reader()
                    .take(16385)
                    .read_to_end(&mut body)
                    .ok()
                    .and_then(|_| serde_json::from_slice::<serde_json::Value>(&body).ok());
                let url = parsed
                    .as_ref()
                    .and_then(|value| value["url"].as_str())
                    .and_then(|url| external_url(url).ok());
                let Some(url) = url else {
                    respond(request, 400, "text/plain", "Invalid external URL", false);
                    continue;
                };
                if webbrowser::open(url.as_str()).is_ok() {
                    respond(request, 200, "application/json", "{}", false);
                } else {
                    respond(
                        request,
                        500,
                        "text/plain",
                        "Could not open the link in your system browser",
                        false,
                    );
                }
            }
            "/api/frame" => {
                if !request.body_length().is_some_and(|n| n <= MAX_INPUT) {
                    respond(
                        request,
                        413,
                        "text/plain",
                        "Browser input is too large",
                        false,
                    );
                    continue;
                }
                let mut body = Vec::new();
                let input = request
                    .as_reader()
                    .take((MAX_INPUT + 1) as u64)
                    .read_to_end(&mut body)
                    .map_err(|e| e.to_string())
                    .and_then(|_| {
                        serde_json::from_slice::<protocol::Input>(&body).map_err(|e| e.to_string())
                    });
                let input = match input {
                    Ok(input) => input,
                    Err(_) => {
                        respond(request, 400, "text/plain", "Invalid browser input", false);
                        continue;
                    }
                };
                let body = match renderer.frame(input, session.reset_textures) {
                    Ok(body) => body,
                    Err(_) => {
                        respond(request, 400, "text/plain", "Invalid browser input", false);
                        continue;
                    }
                };
                session.last_seen = Instant::now();
                session.close_prompt = false;
                session.reset_textures = false;
                if let Some(body) = body {
                    respond(request, 200, "application/json", body, true);
                } else {
                    respond(request, 204, "application/json", "", false);
                }
                if last_save.elapsed() >= Duration::from_secs(10) {
                    renderer.save();
                    last_save = Instant::now();
                }
            }
            "/api/disconnect" => {
                session.client = None;
                session.last_seen = Instant::now();
                renderer.release_input();
                renderer.save();
                respond(request, 200, "application/json", "{}", false);
            }
            "/api/shutdown" => {
                respond(request, 200, "application/json", "{}", false);
                break;
            }
            _ => respond(request, 404, "text/plain", "Not found", false),
        }
    }
    renderer.save();
    // Dropping EditorApp cleans up its PTY and owned addon processes.
    log::info!("Browser session stopped");
    if let Some(error) = failure {
        return Err(error.into());
    }
    Ok(())
}

#[cfg(test)]
mod tests;
