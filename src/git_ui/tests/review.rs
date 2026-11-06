use super::super::{
    GitUi, patch, patch_model,
    repository::git,
    review::{DiffView, ReviewUpdate},
};

struct Repo(std::path::PathBuf);
impl Repo {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "focus-review-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&path).unwrap();
        for args in [
            vec!["init", "-q"],
            vec!["config", "user.name", "Test"],
            vec!["config", "user.email", "test@example.invalid"],
            vec!["config", "commit.gpgsign", "false"],
        ] {
            git(&path, &args).unwrap();
        }
        Self(path)
    }
    fn write(&self, name: &str, text: &str) {
        std::fs::write(self.0.join(name), text).unwrap();
    }
    fn run(&self, args: &[&str]) -> Vec<u8> {
        git(&self.0, args).unwrap()
    }
}
impl Drop for Repo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn aligned_rows_preserve_patch_bytes_and_newline_markers() {
    let raw = b"diff --git a/f b/f\n--- a/f\n+++ b/f\n@@ -1,3 +1,2 @@\n context\r\n-old\xff\n-extra\n+new\n\\ No newline at end of file\n";
    let parsed = patch_model::parse(raw, false);
    assert!(parsed.hunk_actions);
    let hunk = &parsed.hunks[0];
    assert_eq!(hunk.patch, raw);
    assert_eq!(hunk.rows.len(), 3);
    assert_eq!(hunk.rows[0].before.as_ref().unwrap().text, "context");
    assert_eq!(hunk.rows[1].after.as_ref().unwrap().number, 2);
    assert!(hunk.rows[1].after.as_ref().unwrap().no_newline);
    assert!(hunk.rows[2].after.is_none());
    for prefix in [
        "new file mode 100644\n",
        "Binary files a/f and b/f differ\n",
        "diff --cc f\n",
    ] {
        assert!(
            !patch_model::parse(
                format!("{prefix}{}", String::from_utf8_lossy(raw)).as_bytes(),
                false
            )
            .hunk_actions
        );
    }
}

#[test]
fn hunk_toggles_retain_review_and_preserve_other_staging() {
    let repo = Repo::new();
    let original: String = (1..=50).map(|i| format!("line {i}\n")).collect();
    repo.write("file [1]", &original);
    repo.write("other", "original\n");
    repo.run(&["add", "."]);
    repo.run(&["-c", "core.hooksPath=/dev/null", "commit", "-qm", "initial"]);
    let changed = original
        .replace("line 3\n", "changed 3\n")
        .replace("line 40\n", "changed 40\n");
    repo.write("file [1]", &changed);
    repo.write("other", "staged elsewhere\n");
    repo.run(&["add", "other"]);
    let other_index = repo.run(&["show", ":other"]);
    let view = DiffView::load(&repo.0, "file [1]", false, 7, None, None).unwrap();
    assert_eq!(view.hunks.len(), 2);
    let raw = view.hunks[0].patch.clone();
    let rows = format!("{:?}", view.hunks[0].rows);
    let mut ui = GitUi {
        diff: Some(view),
        review_generation: 7,
        ..Default::default()
    };
    for index in [0, 1, 0, 0, 1, 0] {
        let task = ui.diff.as_ref().unwrap().patch_task(index).unwrap();
        let expected = task.staged_after;
        ui.accept_review(ReviewUpdate::Applied(
            patch::apply(&repo.0, &task, None).unwrap(),
        ));
        let view = ui.diff.as_ref().unwrap();
        assert_eq!(view.id, 7);
        assert_eq!(view.hunks[index].staged, expected);
        assert_eq!(view.hunks[0].patch, raw);
        assert_eq!(format!("{:?}", view.hunks[0].rows), rows);
        assert_eq!(repo.run(&["show", ":other"]), other_index);
        assert_eq!(
            std::fs::read_to_string(repo.0.join("file [1]")).unwrap(),
            changed
        );
    }
    assert_eq!(repo.run(&["show", ":file [1]"]), original.as_bytes());
    // An old background load must not replace a newer review.
    ui.accept_review(ReviewUpdate::Loaded(Box::new(
        DiffView::load(&repo.0, "file [1]", false, 6, None, None).unwrap(),
    )));
    assert_eq!(ui.diff.as_ref().unwrap().id, 7);
    // Detect external edits even when the number of changed lines stays identical.
    repo.write("file [1]", &changed.replace("changed 3", "another 3"));
    assert!(
        patch::apply(
            &repo.0,
            &ui.diff.as_ref().unwrap().patch_task(0).unwrap(),
            None
        )
        .is_err()
    );
    assert_eq!(repo.run(&["show", ":file [1]"]), original.as_bytes());
}

#[test]
fn staged_review_reverses_only_selected_hunk_and_rejects_bad_patch() {
    let repo = Repo::new();
    repo.write("f", "one\ntwo\nthree\n");
    repo.run(&["add", "."]);
    repo.run(&["-c", "core.hooksPath=/dev/null", "commit", "-qm", "initial"]);
    repo.write("f", "ONE\ntwo\nthree\n");
    repo.run(&["add", "f"]);
    repo.write("f", "ONE\ntwo\nTHREE\n");
    let mut view = DiffView::load(&repo.0, "f", true, 1, None, None).unwrap();
    let mut bad = view.patch_task(0).unwrap();
    bad.patch = b"invalid patch\n".to_vec();
    assert!(patch::apply(&repo.0, &bad, None).is_err());
    assert!(view.hunks[0].staged);
    let applied = patch::apply(&repo.0, &view.patch_task(0).unwrap(), None).unwrap();
    view.version = applied.version.unwrap();
    view.hunks[0].staged = applied.staged;
    assert_eq!(repo.run(&["show", ":f"]), b"one\ntwo\nthree\n");
    patch::apply(&repo.0, &view.patch_task(0).unwrap(), None).unwrap();
    assert_eq!(repo.run(&["show", ":f"]), b"ONE\ntwo\nthree\n");
    assert_eq!(
        std::fs::read_to_string(repo.0.join("f")).unwrap(),
        "ONE\ntwo\nTHREE\n"
    );
    repo.write("f", "ONE\ntwo\nthree\n");
    repo.run(&["reset", "-q", "--", "f"]);
    assert!(patch::apply(&repo.0, &view.patch_task(0).unwrap(), None).is_err());
}

#[test]
fn comparison_layout_stays_stable_when_hunk_is_staged() {
    use eframe::egui;
    let repo = Repo::new();
    repo.write("f", "old\n");
    repo.run(&["add", "."]);
    repo.run(&["-c", "core.hooksPath=/dev/null", "commit", "-qm", "initial"]);
    repo.write("f", &format!("{}\n", "a long changed line ".repeat(30)));
    for width in [360.0, 1000.0] {
        let view = DiffView::load(&repo.0, "f", false, 1, None, None).unwrap();
        let mut state = GitUi {
            diff: Some(view),
            ..Default::default()
        };
        let ctx = egui::Context::default();
        let mut sizes = Vec::new();
        for staged in [false, false, true, true] {
            state.diff.as_mut().unwrap().hunks[0].staged = staged;
            let _ = crate::test_frame(
                &ctx,
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(width, 700.0),
                    )),
                    ..Default::default()
                },
                |root| {
                    egui::CentralPanel::default().show(root, |ui| {
                        assert!(state.show_diff(ui));
                        sizes.push(ui.min_rect().size());
                    });
                },
            );
        }
        assert!(
            sizes[3].x <= width,
            "comparison exceeded panel: {:?}",
            sizes[3]
        );
        assert_eq!(sizes[1], sizes[3], "staging moved the comparison layout");
    }
}
