use super::*;
use base64::{Engine, engine::general_purpose::STANDARD};
use std::sync::atomic::{AtomicBool, Ordering};

#[test]
fn authenticated_catalog_and_private_proxy() {
    if std::env::var_os("FOCUS_GATEWAY_TEST").is_none() {
        let root = tempfile::tempdir().unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "server::tests::authenticated_catalog_and_private_proxy",
                "--nocapture",
            ])
            .env("FOCUS_GATEWAY_TEST", "1")
            .env("XDG_DATA_HOME", root.path().join("data"))
            .env("XDG_CONFIG_HOME", root.path().join("config"))
            .env_remove("FOCUS_EDITOR_SERVER_USERNAME")
            .env_remove("FOCUS_EDITOR_SERVER_PASSWORD")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let config_path = root.path().join("server.toml");
    std::fs::write(
        &config_path,
        "username = 'smoke'\npassword = 'test-password'\n",
    )
    .unwrap();
    let config = config::Config::load(&Arguments {
        server: true,
        server_config: Some(config_path),
        ..Default::default()
    })
    .unwrap();
    assert_eq!(config.address.to_string(), "0.0.0.0:8999");
    let private = Server::http("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}", private.server_addr().to_ip().unwrap());
    let registration = registry::Registration::new(
        endpoint.clone(),
        "instance-capability".into(),
        "native",
        Some(root.path().into()),
    )
    .unwrap();
    let id = registration.record.id.clone();
    let stop = Arc::new(AtomicBool::new(false));
    let private_stop = stop.clone();
    let worker_endpoint = endpoint.clone();
    let private_worker = std::thread::spawn(move || {
        let mut actions = Vec::new();
        while !private_stop.load(Ordering::Acquire) {
            let Some(mut request) = private.recv_timeout(Duration::from_millis(50)).unwrap() else {
                continue;
            };
            assert_eq!(
                browser::header(&request, "Origin"),
                Some(worker_endpoint.as_str())
            );
            assert_eq!(
                browser::header(&request, "X-Focus-Token"),
                Some("instance-capability")
            );
            assert_eq!(
                browser::header(&request, "X-Focus-Client"),
                Some("remote-browser-test")
            );
            assert_eq!(browser::header(&request, "X-Focus-Remote"), Some("1"));
            actions.push(request.url().to_owned());
            if request.url() == "/api/frame" {
                assert_eq!(
                    browser::read_body(&mut request, 1024).unwrap(),
                    b"{\"epoch\":7}"
                );
            }
            browser::respond(request, 200, "application/json", "{\"ok\":true}", false);
        }
        actions
    });
    let public = Server::http("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", public.server_addr().to_ip().unwrap());
    let public_stop = stop.clone();
    let gateway = std::thread::spawn(move || {
        let client = reqwest::blocking::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(2))
            .build()
            .unwrap();
        while !public_stop.load(Ordering::Acquire) {
            if let Some(request) = public.recv_timeout(Duration::from_millis(50)).unwrap() {
                handle(request, &config, &client);
            }
        }
    });
    let client = reqwest::blocking::Client::builder()
        .no_proxy()
        .build()
        .unwrap();
    let auth = format!("Basic {}", STANDARD.encode("smoke:test-password"));
    let get = |path: &str| {
        client
            .get(format!("{origin}{path}"))
            .header("Authorization", &auth)
    };
    let post = |action: &str| {
        client
            .post(format!("{origin}/api/editors/{id}/{action}"))
            .header("Authorization", &auth)
            .header("Origin", &origin)
            .header("X-Focus-Client", "browser-test")
    };
    for path in [
        "/",
        "/hub.js",
        "/hub.css",
        "/client.js",
        "/browser.css",
        "/api/editors",
        &format!("/editors/{id}/"),
        &format!("/api/editors/{id}/frame"),
    ] {
        let response = client.get(format!("{origin}{path}")).send().unwrap();
        assert_eq!(response.status(), 401, "{path}");
        assert!(
            response.headers()["www-authenticate"]
                .to_str()
                .unwrap()
                .starts_with("Basic ")
        );
        assert_eq!(
            get(path)
                .header("Authorization", "Basic wrong")
                .send()
                .unwrap()
                .status(),
            401
        );
    }
    let catalog = get("/api/editors").send().unwrap().text().unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&catalog).unwrap();
    assert_eq!(parsed[0]["id"], id);
    assert_eq!(parsed[0]["mode"], "native");
    for secret in ["instance-capability", "endpoint", "token", &endpoint] {
        assert!(!catalog.contains(secret));
    }
    assert_eq!(
        get(&format!("/editors/{id}/")).send().unwrap().status(),
        200
    );
    assert_eq!(
        post("connect")
            .header("Origin", "http://other.example")
            .send()
            .unwrap()
            .status(),
        403
    );
    for action in ["connect", "claim", "frame", "disconnect"] {
        assert_eq!(
            post(action)
                .body(if action == "frame" {
                    "{\"epoch\":7}"
                } else {
                    ""
                })
                .send()
                .unwrap()
                .status(),
            200
        );
    }
    for action in ["shutdown", "open-url", "close-prompt"] {
        assert_eq!(post(action).send().unwrap().status(), 404);
    }
    // Public HTTPS origins behind a proxy are accepted using the public Host.
    assert_eq!(
        client
            .post(format!("{origin}/api/editors/{id}/claim"))
            .header("Authorization", &auth)
            .header("X-Focus-Client", "browser-test")
            .header("Host", "editor.example")
            .header("Origin", "https://editor.example")
            .send()
            .unwrap()
            .status(),
        200
    );
    drop(registration);
    assert_eq!(get("/api/editors").send().unwrap().text().unwrap(), "[]");
    assert_eq!(post("frame").body("{}").send().unwrap().status(), 410);
    stop.store(true, Ordering::Release);
    gateway.join().unwrap();
    assert_eq!(
        private_worker.join().unwrap(),
        [
            "/api/connect",
            "/api/claim",
            "/api/frame",
            "/api/disconnect",
            "/api/claim"
        ]
    );
}
