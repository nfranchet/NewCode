use super::*;

#[test]
fn only_one_tab_owns_input_and_reload_can_reconnect() {
    let now = Instant::now();
    let mut session = Session {
        client: None,
        last_seen: now,
        reset_textures: false,
        close_prompt: false,
    };
    assert!(session.connect("first", now));
    session.close_prompt = true;
    assert!(session.reset_textures);
    assert!(!session.connect("second", now));
    assert!(session.active("first"));
    assert!(session.connect("second", now + CLIENT_TIMEOUT));
    assert!(!session.close_prompt);
    assert!(!session.active("first"));
    session.client = None;
    assert!(session.connect("reload", now + CLIENT_TIMEOUT));
}

#[test]
fn api_requires_matching_host_origin_and_secret() {
    let request = |host, origin, token| -> Request {
        tiny_http::TestRequest::new()
            .with_header(Header::from_bytes("Host", host).unwrap())
            .with_header(Header::from_bytes("Origin", origin).unwrap())
            .with_header(Header::from_bytes("X-Focus-Token", token).unwrap())
            .into()
    };
    let host = "127.0.0.1:4321";
    let origin = "http://127.0.0.1:4321";
    assert!(authorized(
        &request(host, origin, "secret"),
        host,
        origin,
        "secret"
    ));
    assert!(!authorized(
        &request("evil.example:4321", origin, "secret"),
        host,
        origin,
        "secret"
    ));
    assert!(!authorized(
        &request(host, "https://evil.example", "secret"),
        host,
        origin,
        "secret"
    ));
    assert!(!authorized(
        &request(host, origin, "wrong"),
        host,
        origin,
        "secret"
    ));
    assert!(!authorized(
        &tiny_http::TestRequest::new().into(),
        host,
        origin,
        "secret"
    ));
}

// Run the actual HTTP host with isolated application directories and a short grace period.
// A subprocess avoids changing HOME/XDG variables under the other parallel tests.
#[test]
fn http_session_reconnect_shutdown_and_timeout() {
    if let Some(root) = std::env::var_os("FOCUS_BROWSER_TEST_ROOT") {
        host_round_trip(std::path::Path::new(&root));
        return;
    }
    let root = tempfile::tempdir().unwrap();
    use std::os::unix::fs::PermissionsExt;
    let opener = root.path().join("open-link");
    std::fs::write(
        &opener,
        "#!/bin/sh\nprintf '%s' \"$1\" > \"$FOCUS_TEST_LINK_OUT\"\n",
    )
    .unwrap();
    std::fs::set_permissions(&opener, std::fs::Permissions::from_mode(0o700)).unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "browser::tests::http_session_reconnect_shutdown_and_timeout",
            "--nocapture",
        ])
        .env("FOCUS_BROWSER_TEST_ROOT", root.path())
        .env("XDG_DATA_HOME", root.path().join("data"))
        .env("XDG_CONFIG_HOME", root.path().join("config"))
        .env("SHELL", "/bin/sh")
        .env("BROWSER", &opener)
        .env("FOCUS_TEST_LINK_OUT", root.path().join("opened-link"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn host_round_trip(root: &std::path::Path) {
    let workspace = root.join("project");
    std::fs::create_dir_all(&workspace).unwrap();
    std::fs::write(workspace.join("example.txt"), "original\n").unwrap();
    let server = Server::http("127.0.0.1:0").unwrap();
    let host = server.server_addr().to_ip().unwrap().to_string();
    let origin = format!("http://{host}");
    let worker_origin = origin.clone();
    let worker_host = host.clone();
    let worker = std::thread::spawn(move || {
        let arguments = Arguments {
            workspace: Some(workspace),
            ..Default::default()
        };
        serve(
            server,
            &arguments,
            &worker_host,
            &worker_origin,
            "test-secret",
            Duration::from_secs(15),
            None,
        )
        .unwrap();
    });
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap();
    let post = |path: &str, tab: &str| {
        client
            .post(format!("{origin}/api/{path}"))
            .header("Origin", &origin)
            .header("X-Focus-Token", "test-secret")
            .header("X-Focus-Client", tab)
    };
    assert_eq!(
        client.get(format!("{origin}/")).send().unwrap().status(),
        200
    );
    assert_eq!(
        client
            .get(format!("{origin}/Cargo.toml"))
            .send()
            .unwrap()
            .status(),
        404
    );
    assert_eq!(
        post("connect", "one")
            .header("Origin", "https://other.example")
            .send()
            .unwrap()
            .status(),
        403
    );
    assert_eq!(
        post("connect", "one")
            .header("X-Focus-Token", "wrong")
            .send()
            .unwrap()
            .status(),
        403
    );
    assert_eq!(post("connect", "one").send().unwrap().status(), 200);
    assert_eq!(post("connect", "two").send().unwrap().status(), 409);
    for url in [
        "file:///etc/passwd",
        "javascript:alert(1)",
        "data:text/html,hello",
        "chrome://settings",
        "not a URL",
    ] {
        assert_eq!(
            post("open-url", "one")
                .json(&serde_json::json!({"url": url}))
                .send()
                .unwrap()
                .status(),
            400
        );
    }
    assert_eq!(
        post("open-url", "one")
            .json(&serde_json::json!({"url": "https://example.invalid/docs"}))
            .send()
            .unwrap()
            .status(),
        200
    );
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        if let Ok(url) = std::fs::read_to_string(root.join("opened-link")) {
            if url == "https://example.invalid/docs" {
                break;
            }
        }
        assert!(
            Instant::now() < deadline,
            "System browser opener was not invoked"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(
        post("frame", "one")
            .body("invalid")
            .send()
            .unwrap()
            .status(),
        400
    );
    let input = serde_json::json!({ "size": [1200, 800], "pixels_per_point": 1, "focused": true, "events": [] });
    let frame: serde_json::Value = post("frame", "one")
        .json(&input)
        .send()
        .unwrap()
        .json()
        .unwrap();
    assert!(!frame["meshes"].as_array().unwrap().is_empty());
    assert!(!frame["textures"].as_array().unwrap().is_empty());
    assert_eq!(frame["title"], "Focus Editor — project");
    assert_eq!(post("disconnect", "one").send().unwrap().status(), 200);
    assert_eq!(post("connect", "two").send().unwrap().status(), 200);
    assert_eq!(post("shutdown", "one").send().unwrap().status(), 409);
    let replay: serde_json::Value = post("frame", "two")
        .json(&input)
        .send()
        .unwrap()
        .json()
        .unwrap();
    assert!(!replay["textures"].as_array().unwrap().is_empty());
    assert_eq!(replay["title"], "Focus Editor — project");
    assert_eq!(post("shutdown", "two").send().unwrap().status(), 200);
    worker.join().unwrap();

    let server = Server::http("127.0.0.1:0").unwrap();
    let host = server.server_addr().to_ip().unwrap().to_string();
    let start = Instant::now();
    serve(
        server,
        &Arguments::default(),
        &host,
        &format!("http://{host}"),
        "unused",
        Duration::from_millis(200),
        None,
    )
    .unwrap();
    assert!(
        start.elapsed() < Duration::from_secs(5),
        "Disconnected host failed to stop"
    );

    // A user may leave an unsaved-change confirmation open longer than the
    // reconnect allowance. The owned browser must keep those buffers alive.
    use std::os::unix::fs::PermissionsExt;
    let browser = root.join("prompt-browser");
    std::fs::write(&browser, "#!/bin/sh\nexec sleep 30\n").unwrap();
    std::fs::set_permissions(&browser, std::fs::Permissions::from_mode(0o700)).unwrap();
    let server = Server::http("127.0.0.1:0").unwrap();
    let host = server.server_addr().to_ip().unwrap().to_string();
    let origin = format!("http://{host}");
    let worker_origin = origin.clone();
    let worker = std::thread::spawn(move || {
        let mut window = launcher::AppWindow::spawn(&browser, &worker_origin).unwrap();
        serve(
            server,
            &Arguments::default(),
            &host,
            &worker_origin,
            "prompt-secret",
            Duration::from_secs(1),
            Some(&mut window),
        )
        .unwrap();
    });
    let post = |path: &str| {
        client
            .post(format!("{origin}/api/{path}"))
            .header("Origin", &origin)
            .header("X-Focus-Token", "prompt-secret")
            .header("X-Focus-Client", "prompt")
    };
    assert_eq!(post("connect").send().unwrap().status(), 200);
    assert_eq!(post("close-prompt").send().unwrap().status(), 200);
    std::thread::sleep(Duration::from_millis(1200));
    assert_eq!(post("frame").json(&input).send().unwrap().status(), 200);
    assert_eq!(post("shutdown").send().unwrap().status(), 200);
    worker.join().unwrap();
}
