use super::*;
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::TcpListener,
    path::Path,
    time::Duration,
};

#[test]
fn http_requests_include_directory_and_body_and_report_server_errors() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let (captured_tx, captured) = mpsc::channel();
    let mock = std::thread::spawn(move || {
        for (status, body) in [
            ("200 OK", "{\"id\":\"session1\"}"),
            ("403 Forbidden", "{\"message\":\"Permission denied\"}"),
        ] {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut first = String::new();
            reader.read_line(&mut first).unwrap();
            let mut len = 0;
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                if line == "\r\n" {
                    break;
                }
                if line.to_lowercase().starts_with("content-length:") {
                    len = line.split(':').nth(1).unwrap().trim().parse().unwrap();
                }
            }
            let mut bytes = vec![0; len];
            reader.read_exact(&mut bytes).unwrap();
            captured_tx.send((first, bytes)).unwrap();
            write!(stream,"HTTP/1.1 {status}\r\nContent-Length: {}\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
        }
    });
    let client = reqwest::blocking::Client::builder()
        .no_proxy()
        .build()
        .unwrap();
    let result = worker::execute(
        &client,
        &base,
        Path::new("/project with spaces"),
        "POST",
        "/session",
        Some(&serde_json::json!({"title":"Session"})),
    )
    .unwrap();
    assert_eq!(result["id"], "session1");
    let (line, body) = captured.recv().unwrap();
    assert!(line.contains("directory=%2Fproject+with+spaces"));
    assert_eq!(
        serde_json::from_slice::<Value>(&body).unwrap()["title"],
        "Session"
    );
    let error = worker::execute(
        &client,
        &base,
        Path::new("/project"),
        "GET",
        "/permission",
        None,
    )
    .unwrap_err();
    assert!(error.contains("403") && error.contains("Permission denied"));
    mock.join().unwrap();
}

#[test]
fn request_ids_are_monotonic_and_shutdown_disconnects_the_queue() {
    let (mut server, rx, _) = Server::capture();
    let one = server.request("GET", "/provider".into(), None).unwrap();
    let two = server.request("GET", "/agent".into(), None).unwrap();
    assert!(two > one);
    assert_eq!(rx.recv().unwrap().id, one);
    assert_eq!(rx.recv().unwrap().id, two);
    drop(server);
    assert!(rx.recv_timeout(Duration::from_millis(50)).is_err());
}

#[test]
#[ignore = "requires installed OpenCode and permission to start a local server; never sends model prompts"]
fn installed_server_catalog_smoke() {
    let root = std::env::temp_dir().join(format!(
        "focus-opencode-native-smoke-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let mut server = Server::spawn(&root, None, egui::Context::default());
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    let mut ready = false;
    while std::time::Instant::now() < deadline && !ready {
        for event in server.drain() {
            match event {
                Event::Ready(version) => {
                    assert!(!version.is_empty());
                    ready = true;
                }
                Event::Closed(error) => panic!("{error}"),
                _ => {}
            }
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(ready, "OpenCode did not become ready");
    let providers = server.request("GET", "/provider".into(), None).unwrap();
    let agents = server.request("GET", "/agent".into(), None).unwrap();
    let mut received = 0;
    while std::time::Instant::now() < deadline && received < 2 {
        for event in server.drain() {
            if let Event::Reply(id, result) = event {
                let value = result.unwrap();
                if id == providers {
                    assert!(value["all"].is_array());
                    received += 1;
                }
                if id == agents {
                    assert!(value.is_array());
                    received += 1;
                }
            }
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(received, 2);
    let pid = server.child.lock().unwrap().as_ref().unwrap().id();
    drop(server);
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    while unsafe { libc::kill(pid as libc::pid_t, 0) } == 0 && std::time::Instant::now() < deadline
    {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_ne!(
        unsafe { libc::kill(pid as libc::pid_t, 0) },
        0,
        "server process leaked after drop"
    );
    std::fs::remove_dir_all(root).unwrap();
}
