use super::*;
use focus_addon_sdk::{GitOperation, Widget};
use std::fs;
use std::{
    process::Command,
    sync::atomic::AtomicUsize,
    time::{Instant, SystemTime},
};

static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "focus-addon-test-{}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
    fn manifest(&self) -> Manifest {
        fs::write(self.0.join("program"), b"fixture").unwrap();
        fs::write(self.0.join("icon.png"), b"fixture").unwrap();
        fs::write(self.0.join("addon.toml"), "id='fixture'\nname='Fixture'\nversion='1'\nprotocol=1\nexecutable='program'\nicon='icon.png'\n").unwrap();
        load_manifest(&self.0.join("addon.toml")).unwrap()
    }
    fn git(&self, args: &[&str]) -> String {
        let output = Command::new("git")
            .current_dir(&self.0)
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).trim().into()
    }
    fn repository(&self) {
        self.git(&["init", "-b", "main"]);
        self.git(&["config", "user.email", "fixture@example.invalid"]);
        self.git(&["config", "user.name", "Fixture"]);
        fs::write(self.0.join("file"), "original").unwrap();
        self.git(&["add", "."]);
        self.git(&["commit", "-m", "Initial"]);
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn addon_directory_is_sibling_of_logs() {
    assert_eq!(
        crate::app_paths::addons().unwrap().parent(),
        crate::logging::session::log_path()
            .unwrap()
            .parent()
            .unwrap()
            .parent()
    );
}
#[test]
fn manifest_rejects_escapes_and_incompatible_protocols() {
    let fixture = Fixture::new();
    fixture.manifest();
    let path = fixture.0.join("addon.toml");
    let original = fs::read_to_string(&path).unwrap();
    fs::write(&path, original.replace("protocol=1", "protocol=99")).unwrap();
    assert!(load_manifest(&path).unwrap_err().contains("protocol"));
    fs::write(
        &path,
        original.replace("executable='program'", "executable='../program'"),
    )
    .unwrap();
    assert!(load_manifest(&path).unwrap_err().contains("relative"));
    #[cfg(unix)]
    {
        fs::write(&path, &original).unwrap();
        fs::remove_file(fixture.0.join("program")).unwrap();
        std::os::unix::fs::symlink("/bin/sh", fixture.0.join("program")).unwrap();
        assert!(load_manifest(&path).unwrap_err().contains("escapes"));
    }
}
#[test]
fn rejects_stale_addon_views_and_keeps_host_responsive() {
    use std::os::unix::fs::PermissionsExt;
    let fixture = Fixture::new();
    let manifest = fixture.manifest();
    fs::write(
        fixture.0.join("program"),
        "#!/bin/sh\nIFS= read -r init\ncat messages.jsonl\nwhile IFS= read -r event; do cat selected.jsonl; done\n",
    )
    .unwrap();
    fs::set_permissions(fixture.0.join("program"), fs::Permissions::from_mode(0o755)).unwrap();
    let mut wire = Vec::new();
    focus_addon_sdk::write_message(
        &mut wire,
        &Message::response(0, serde_json::json!({"protocol":1})),
    )
    .unwrap();
    for (generation, title) in [(1, "current"), (0, "stale")] {
        let view = View {
            generation,
            title: title.into(),
            sidebar: vec![Widget::text(title)],
            detail: vec![Widget::text("Selected item")],
            reveal_detail: true,
            ..Default::default()
        };
        focus_addon_sdk::write_message(&mut wire, &Message::notification("view.update", view))
            .unwrap();
    }
    fs::write(fixture.0.join("messages.jsonl"), wire).unwrap();
    let selected = View {
        generation: 1,
        title: "current".into(),
        detail: vec![Widget::text("Selected item")],
        reveal_detail: true,
        ..Default::default()
    };
    let mut wire = Vec::new();
    focus_addon_sdk::write_message(&mut wire, &Message::notification("view.update", selected))
        .unwrap();
    fs::write(fixture.0.join("selected.jsonl"), wire).unwrap();
    let mut host = Addons::default();
    host.context.generation = 1;
    host.entries.push(Addon {
        manifest,
        directory: fixture.0.clone(),
        storage: fixture.0.join("data"),
        enabled: true,
        process: None,
        error: None,
        view: View::default(),
        values: BTreeMap::new(),
        icon: None,
        working: false,
        incarnation: 0,
        surface: Default::default(),
        seed: Default::default(),
        status: String::new(),
    });
    let ctx = egui::Context::default();
    host.activate("fixture".into(), &ctx);
    let start = Instant::now();
    while host.entries[0].view.title != "current" && start.elapsed() < Duration::from_secs(2) {
        host.tick(&ctx, false, true);
        std::thread::sleep(Duration::from_millis(10));
    }
    host.tick(&ctx, false, true);
    assert_eq!(host.entries[0].view.title, "current");
    assert!(host.entries[0].process.as_ref().unwrap().ready);
    host.detail = false;
    host.action("fixture", "reopen".into());
    let start = Instant::now();
    while !host.detail && start.elapsed() < Duration::from_secs(2) {
        host.tick(&ctx, false, true);
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        host.detail,
        "Selecting the same item should reopen its dismissed details"
    );
}
#[test]
fn checkout_preserves_dirty_worktree_and_mutations_share_a_lease() {
    let fixture = Fixture::new();
    let manifest = fixture.manifest();
    fixture.repository();
    fixture.git(&["branch", "issue-1"]);
    fixture.git(&["remote", "add", "origin", "."]);
    fs::write(fixture.0.join("file"), "unsaved worktree change").unwrap();
    let checkout = || Service::Git {
        operation: GitOperation::Checkout {
            branch: "issue-1".into(),
            remote: "origin".into(),
        },
    };
    let reply = services::execute(&manifest, Some(&fixture.0), None, checkout());
    assert!(!reply.ok);
    assert_eq!(fixture.git(&["branch", "--show-current"]), "main");
    assert_eq!(
        fs::read_to_string(fixture.0.join("file")).unwrap(),
        "unsaved worktree change"
    );
    let _lease = crate::repository_jobs::acquire(&fixture.0).unwrap();
    let reply = services::execute(&manifest, Some(&fixture.0), None, checkout());
    assert!(!reply.ok);
    assert!(reply.output.contains("Another Git operation"));
}
#[test]
fn local_cleanup_refuses_new_commits_and_other_worktrees() {
    let fixture = Fixture::new();
    let manifest = fixture.manifest();
    fixture.repository();
    let initial = fixture.git(&["rev-parse", "HEAD"]);
    fixture.git(&["switch", "-c", "issue-1"]);
    fs::write(fixture.0.join("file"), "new commit").unwrap();
    fixture.git(&["commit", "-am", "New work"]);
    let operation = |expected: String| Service::Git {
        operation: GitOperation::DeleteLocal {
            branch: "issue-1".into(),
            expected,
            base: "main".into(),
        },
    };
    let reply = services::execute(&manifest, Some(&fixture.0), None, operation(initial));
    assert!(!reply.ok);
    assert_eq!(fixture.git(&["branch", "--show-current"]), "issue-1");
    let expected = fixture.git(&["rev-parse", "HEAD"]);
    fixture.git(&["switch", "main"]);
    let worktree = Fixture::new();
    fixture.git(&["worktree", "add", worktree.0.to_str().unwrap(), "issue-1"]);
    let reply = services::execute(
        &manifest,
        Some(&fixture.0),
        None,
        operation(expected.clone()),
    );
    assert!(!reply.ok);
    assert!(reply.output.contains("another worktree"));
    fixture.git(&["worktree", "remove", worktree.0.to_str().unwrap()]);
    assert!(services::execute(&manifest, Some(&fixture.0), None, operation(expected)).ok);
    assert!(
        !fixture
            .git(&["show-ref", "--heads"])
            .contains("refs/heads/issue-1")
    );
}
