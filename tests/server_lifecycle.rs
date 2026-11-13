//! Real binaries, isolated data, and no desktop or Chromium in the editor environment.
use reqwest::blocking::{Client, Response};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::mpsc,
    time::{Duration, Instant},
};

const EDITOR: &str = env!("CARGO_BIN_EXE_focus-editor");
const SERVER: &str = env!("CARGO_BIN_EXE_focus-editor-server");
fn wait<T>(mut check: impl FnMut() -> Option<T>) -> T {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if let Some(value) = check() {
            return value;
        }
        assert!(
            Instant::now() < deadline,
            "Timed out waiting for editor/server lifecycle"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}
fn command(binary: &str, root: &Path) -> Command {
    let mut command = Command::new(binary);
    command
        .env("XDG_DATA_HOME", root.join("data"))
        .env("XDG_CONFIG_HOME", root.join("config"))
        .env("SHELL", "/bin/sh")
        .env("PATH", "/no-browser-installed")
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .env_remove("FOCUS_EDITOR_SERVER_USERNAME")
        .env_remove("FOCUS_EDITOR_SERVER_PASSWORD")
        .stdin(Stdio::null());
    command
}
struct Gateway {
    child: Child,
    origin: String,
    client: Client,
}
impl Gateway {
    fn start(root: &Path) -> Self {
        let mut child = command(SERVER, root)
            .args(["--bind", "127.0.0.1", "--port", "0", "--config"])
            .arg(root.join("server.toml"))
            .stdout(Stdio::piped())
            .stderr(fs::File::create(root.join("server.log")).unwrap())
            .spawn()
            .unwrap();
        let stdout = child.stdout.take().unwrap();
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let mut line = String::new();
            let _ = BufReader::new(stdout).read_line(&mut line);
            let _ = sender.send(line);
        });
        let line = receiver.recv_timeout(Duration::from_secs(15)).unwrap();
        assert!(
            line.starts_with("Focus Editor server listening on http://127.0.0.1:"),
            "Server startup failed: {line}"
        );
        Self {
            child,
            origin: line
                .trim()
                .strip_prefix("Focus Editor server listening on ")
                .unwrap()
                .into(),
            client: Client::builder()
                .no_proxy()
                .timeout(Duration::from_secs(10))
                .build()
                .unwrap(),
        }
    }
    fn get(&self, path: &str) -> Value {
        self.client
            .get(format!("{}{path}", self.origin))
            .basic_auth("smoke", Some("fixture-password"))
            .send()
            .unwrap()
            .error_for_status()
            .unwrap()
            .json()
            .unwrap()
    }
    fn post(&self, path: &str, body: Value) -> Response {
        self.client
            .post(format!("{}{path}", self.origin))
            .basic_auth("smoke", Some("fixture-password"))
            .header("Origin", &self.origin)
            .header("X-Focus-Client", "lifecycle-test")
            .json(&body)
            .send()
            .unwrap()
    }
    fn stop(&mut self) {
        if self.child.try_wait().unwrap().is_none() {
            self.child.kill().unwrap();
            self.child.wait().unwrap();
        }
    }
}
impl Drop for Gateway {
    fn drop(&mut self) {
        self.stop();
    }
}
struct Cleanup {
    root: PathBuf,
    direct: Option<Child>,
}
impl Drop for Cleanup {
    fn drop(&mut self) {
        let client = Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(2))
            .build()
            .unwrap();
        for record in records(&self.root) {
            let _ = client
                .post(format!(
                    "{}/api/manage/stop",
                    record["endpoint"].as_str().unwrap()
                ))
                .header("Origin", record["endpoint"].as_str().unwrap())
                .header("X-Focus-Token", record["token"].as_str().unwrap())
                .json(&json!({"choice":"discard"}))
                .send();
        }
        if let Some(child) = &mut self.direct {
            if child.try_wait().ok().flatten().is_none() {
                let _ = child.kill();
            }
            let _ = child.wait();
        }
    }
}
fn records(root: &Path) -> Vec<Value> {
    fs::read_dir(root.join("data/focus-editor/runtime/instances"))
        .ok()
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            fs::read(entry.path())
                .ok()
                .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        })
        .collect()
}
fn key(name: &str, ctrl: bool) -> Vec<Value> {
    let modifiers = json!({"alt":false,"ctrl":ctrl,"shift":false,"mac_cmd":false,"command":ctrl});
    vec![
        json!({"ModifiersChanged":modifiers}),
        json!({"Key":{"key":name,"physical_key":null,"pressed":true,"repeat":false,"modifiers":modifiers}}),
        json!({"Key":{"key":name,"physical_key":null,"pressed":false,"repeat":false,"modifiers":modifiers}}),
    ]
}
fn pointer(x: f32, y: f32) -> Vec<Value> {
    let modifiers = json!({"alt":false,"ctrl":false,"shift":false,"mac_cmd":false,"command":false});
    vec![
        json!({"PointerMoved":{"x":x,"y":y}}),
        json!({"PointerButton":{"pos":{"x":x,"y":y},"button":"Primary","pressed":true,"modifiers":modifiers}}),
        json!({"PointerButton":{"pos":{"x":x,"y":y},"button":"Primary","pressed":false,"modifiers":modifiers}}),
    ]
}
fn frame(server: &Gateway, id: &str, epoch: u64, events: Vec<Value>) -> Option<Value> {
    let response = server.post(&format!("/api/editors/{id}/frame"), json!({"size":[1200,800],"pixels_per_point":1,"focused":true,"events":events,"epoch":epoch}));
    assert!(
        response.status().is_success(),
        "Frame failed: {}",
        response.status()
    );
    if response.status().as_u16() == 204 {
        None
    } else {
        Some(response.json().unwrap())
    }
}
fn connect(server: &Gateway, id: &str) -> u64 {
    assert!(
        server
            .post(&format!("/api/editors/{id}/connect"), json!({}))
            .status()
            .is_success()
    );
    server
        .post(&format!("/api/editors/{id}/claim"), json!({}))
        .json::<Value>()
        .unwrap()["epoch"]
        .as_u64()
        .unwrap()
}
fn edit(server: &Gateway, id: &str, epoch: u64, text: &str) {
    let mut events = pointer(150.0, 127.0);
    events.extend(pointer(600.0, 102.0));
    events.extend(key("A", true));
    events.push(json!({"Text":text}));
    assert_eq!(frame(server, id, epoch, events).unwrap()["dirty"], true);
}

#[test]
fn separate_server_launches_and_reattaches_persistent_editors() {
    let root = tempfile::tempdir().unwrap();
    let root = root.path();
    fs::write(
        root.join("server.toml"),
        "username='smoke'\npassword='fixture-password'\n",
    )
    .unwrap();
    assert!(
        Command::new(SERVER)
            .arg("--version")
            .output()
            .unwrap()
            .stdout
            .starts_with(b"focus-editor-server ")
    );
    let old = Command::new(EDITOR).arg("--server").output().unwrap();
    assert!(
        !old.status.success()
            && String::from_utf8_lossy(&old.stderr).contains("focus-editor-server")
    );
    let a = root.join("already open");
    let b = root.join("known project");
    for path in [&a, &b] {
        fs::create_dir(path).unwrap();
        fs::write(path.join("example.txt"), "original\n").unwrap();
    }
    // Import a workspace recorded by a version predating the workspace catalog.
    let b_id: String = Sha256::digest(b.as_os_str().as_encoded_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    let saved = root.join("data/focus-editor/workspaces").join(&b_id);
    fs::create_dir_all(&saved).unwrap();
    fs::write(
        saved.join("settings.toml"),
        toml::to_string(&json!({"workspace":b})).unwrap(),
    )
    .unwrap();
    let direct = command(EDITOR, root)
        .args(["--headless", "--foreground"])
        .arg(&a)
        .stdout(Stdio::null())
        .stderr(fs::File::create(root.join("direct.log")).unwrap())
        .spawn()
        .unwrap();
    let _cleanup = Cleanup {
        root: root.into(),
        direct: Some(direct),
    };
    let a_record = wait(|| {
        records(root)
            .into_iter()
            .find(|record| record["workspace"] == a.to_string_lossy().as_ref())
    });
    let a_id = a_record["id"].as_str().unwrap().to_owned();
    // Older running editors have a live registry entry but no durable workspace metadata.
    let a_workspace_id: String = Sha256::digest(a.as_os_str().as_encoded_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    fs::remove_file(
        root.join("data/focus-editor/workspaces")
            .join(&a_workspace_id)
            .join("workspace.json"),
    )
    .unwrap();
    let mut server = Gateway::start(root);
    assert_eq!(server.get("/api/workspaces").as_array().unwrap().len(), 2);
    assert_eq!(
        server
            .client
            .get(format!("{}/api/workspaces", server.origin))
            .send()
            .unwrap()
            .status(),
        401
    );
    wait(|| {
        fs::read_to_string(root.join("direct.log"))
            .ok()
            .filter(|text| text.contains("Registered editor with running server"))
    });
    let duplicate = command(SERVER, root)
        .args(["--bind", "127.0.0.1", "--port", "0", "--config"])
        .arg(root.join("server.toml"))
        .output()
        .unwrap();
    assert!(
        !duplicate.status.success()
            && String::from_utf8_lossy(&duplicate.stderr).contains("already running")
    );
    assert_eq!(
        server
            .client
            .post(format!("{}/api/workspaces", server.origin))
            .basic_auth("smoke", Some("fixture-password"))
            .header("Origin", "https://other.example")
            .json(&json!({"path":b}))
            .send()
            .unwrap()
            .status(),
        403
    );
    let known = server.get("/api/workspaces");
    assert_eq!(known.as_array().unwrap().len(), 2);
    assert!(
        known
            .as_array()
            .unwrap()
            .iter()
            .any(|workspace| workspace["id"] == b_id && workspace["state"] == "stopped")
    );
    assert!(!known.to_string().contains("token"));
    assert_eq!(
        server
            .post("/api/workspaces", json!({"path":"relative"}))
            .status(),
        400
    );
    assert_eq!(
        server
            .post("/api/workspaces", json!({"path":root.join("missing")}))
            .status(),
        400
    );
    let alias = root.join("alias");
    std::os::unix::fs::symlink(&b, &alias).unwrap();
    assert_eq!(
        server
            .post("/api/workspaces", json!({"path":alias}))
            .json::<Value>()
            .unwrap()["id"],
        b_id
    );
    let a_epoch = connect(&server, &a_id);
    edit(&server, &a_id, a_epoch, "unsaved before restart");
    // Concurrent starts must launch one process, including paths containing spaces.
    let mut requests = Vec::new();
    for _ in 0..4 {
        let client = server.client.clone();
        let origin = server.origin.clone();
        let b_id = b_id.clone();
        requests.push(std::thread::spawn(move || {
            client
                .post(format!("{origin}/api/workspaces/{b_id}/start"))
                .basic_auth("smoke", Some("fixture-password"))
                .header("Origin", &origin)
                .json(&json!({}))
                .send()
                .unwrap()
                .status()
        }));
    }
    for request in requests {
        assert!(request.join().unwrap().is_success());
    }
    let b_record = wait(|| {
        records(root)
            .into_iter()
            .find(|record| record["workspace"] == b.to_string_lossy().as_ref())
    });
    let b_editor = b_record["id"].as_str().unwrap().to_owned();
    assert_eq!(
        records(root)
            .iter()
            .filter(|record| record["workspace"] == b.to_string_lossy().as_ref())
            .count(),
        1
    );
    assert_eq!(
        server
            .post(&format!("/api/workspaces/{b_id}/start"), json!({}))
            .json::<Value>()
            .unwrap()["editor_id"],
        b_editor
    );
    let b_epoch = connect(&server, &b_editor);
    edit(&server, &b_editor, b_epoch, "saved after restart ✓");
    let pid_file = root.join("shell.pid");
    let mut events = pointer(600.0, 720.0);
    events.push(json!({"Text":format!("printf $$ > '{}'; /bin/sleep 120",pid_file.display())}));
    events.extend(key("Enter", false));
    frame(&server, &b_editor, b_epoch, events);
    let shell_pid: i32 = wait(|| {
        fs::read_to_string(&pid_file)
            .ok()
            .and_then(|text| text.parse().ok())
    });
    let old_generation: Value = serde_json::from_slice(
        &fs::read(root.join("data/focus-editor/runtime/server.json")).unwrap(),
    )
    .unwrap();
    server.stop();
    assert_eq!(unsafe { libc::kill(shell_pid, 0) }, 0);
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(records(root).len(), 2);
    let server = Gateway::start(root);
    let generation: Value = serde_json::from_slice(
        &fs::read(root.join("data/focus-editor/runtime/server.json")).unwrap(),
    )
    .unwrap();
    assert_ne!(generation["generation"], old_generation["generation"]);
    let live = server.get("/api/editors");
    for record in [&a_record, &b_record] {
        assert!(
            live.as_array()
                .unwrap()
                .iter()
                .any(|live| live["id"] == record["id"])
        );
        assert_eq!(
            unsafe { libc::kill(record["pid"].as_i64().unwrap() as i32, 0) },
            0
        );
    }
    wait(|| {
        fs::read_to_string(root.join("direct.log"))
            .ok()
            .filter(|text| {
                text.matches("Registered editor with running server")
                    .count()
                    >= 2
            })
    });
    let a_epoch = connect(&server, &a_id);
    assert_eq!(
        frame(&server, &a_id, a_epoch, vec![]).unwrap()["dirty"],
        true
    );
    assert_eq!(
        fs::read_to_string(a.join("example.txt")).unwrap(),
        "original\n"
    );
    let b_epoch = connect(&server, &b_editor);
    assert_eq!(
        frame(&server, &b_editor, b_epoch, vec![]).unwrap()["dirty"],
        true
    );
    assert_eq!(unsafe { libc::kill(shell_pid, 0) }, 0);
    assert_eq!(
        server
            .post(&format!("/api/editors/{b_editor}/stop"), json!({}))
            .status(),
        409
    );
    fs::rename(b.join("example.txt"), b.join("backup.txt")).unwrap();
    fs::create_dir(b.join("example.txt")).unwrap();
    assert_eq!(
        server
            .post(
                &format!("/api/editors/{b_editor}/stop"),
                json!({"choice":"save"})
            )
            .status(),
        422
    );
    assert_eq!(
        unsafe { libc::kill(b_record["pid"].as_i64().unwrap() as i32, 0) },
        0
    );
    fs::remove_dir(b.join("example.txt")).unwrap();
    fs::rename(b.join("backup.txt"), b.join("example.txt")).unwrap();
    assert!(
        server
            .post(
                &format!("/api/editors/{b_editor}/stop"),
                json!({"choice":"save"})
            )
            .status()
            .is_success()
    );
    wait(|| {
        records(root)
            .into_iter()
            .all(|record| record["id"] != b_editor)
            .then_some(())
    });
    assert_eq!(
        fs::read_to_string(b.join("example.txt")).unwrap(),
        "saved after restart ✓"
    );
    wait(|| (unsafe { libc::kill(shell_pid, 0) } != 0).then_some(()));
    assert!(
        server
            .post(
                &format!("/api/editors/{a_id}/stop"),
                json!({"choice":"discard"})
            )
            .status()
            .is_success()
    );
    wait(|| records(root).is_empty().then_some(()));
    assert_eq!(
        fs::read_to_string(a.join("example.txt")).unwrap(),
        "original\n"
    );
    assert_eq!(server.get("/api/workspaces").as_array().unwrap().len(), 2);
    fs::remove_dir_all(&b).unwrap();
    assert!(
        server
            .get("/api/workspaces")
            .as_array()
            .unwrap()
            .iter()
            .any(|workspace| workspace["id"] == b_id && workspace["state"] == "unavailable")
    );
    // Unreadable settings must fail startup instead of creating an empty duplicate session.
    let broken = root.join("failed startup");
    fs::create_dir(&broken).unwrap();
    let added: Value = server
        .post("/api/workspaces", json!({"path":broken}))
        .json()
        .unwrap();
    let broken_id = added["id"].as_str().unwrap();
    let settings = root
        .join("data/focus-editor/workspaces")
        .join(broken_id)
        .join("settings.toml");
    fs::create_dir(&settings).unwrap();
    assert!(
        server
            .post(&format!("/api/workspaces/{broken_id}/start"), json!({}))
            .status()
            .is_success()
    );
    wait(|| {
        server
            .get("/api/workspaces")
            .as_array()
            .unwrap()
            .iter()
            .any(|workspace| workspace["id"] == broken_id && workspace["state"] == "failed")
            .then_some(())
    });
    assert!(records(root).is_empty());
    fs::remove_dir(&settings).unwrap();
    fs::write(settings, "").unwrap();
    assert!(
        server
            .post(&format!("/api/workspaces/{broken_id}/start"), json!({}))
            .status()
            .is_success()
    );
    let retried = wait(|| {
        records(root)
            .into_iter()
            .find(|record| record["workspace"] == broken.to_string_lossy().as_ref())
    });
    assert!(
        server
            .post(
                &format!("/api/editors/{}/stop", retried["id"].as_str().unwrap()),
                json!({})
            )
            .status()
            .is_success()
    );
    wait(|| records(root).is_empty().then_some(()));
}
