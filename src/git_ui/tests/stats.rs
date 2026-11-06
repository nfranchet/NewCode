use super::{
    super::{
        model::Side,
        repository::{Change, snapshot as read_snapshot},
        stats::{self, LineStats},
    },
    review::Repo,
};

#[test]
fn numstat_keeps_numeric_columns_binary_files_and_rename_destinations() {
    let counts = stats::numstat(
        b"12\t3\tfile.rs\0-\t-\tbinary\0\
          1\t2\t\0old\tname\n.rs\0new\tname\n.rs\0\
          0\t0\tempty\0bad\t2\tunknown\0",
    );
    assert_eq!(counts.len(), 5);
    assert_eq!(
        counts["file.rs"],
        LineStats::Text {
            added: 12,
            deleted: 3
        }
    );
    assert_eq!(counts["binary"], LineStats::Binary);
    assert_eq!(
        counts["new\tname\n.rs"],
        LineStats::Text {
            added: 1,
            deleted: 2
        }
    );
    assert!(!counts.contains_key("old\tname\n.rs"));
    assert_eq!(
        counts["empty"],
        LineStats::Text {
            added: 0,
            deleted: 0
        }
    );
    assert_eq!(counts["unknown"], LineStats::Unavailable);
}

#[test]
fn untracked_counts_handle_line_endings_empty_files_and_stream_boundaries() {
    let repo = Repo::new();
    for (path, content, lines) in [
        ("empty", "", 0),
        ("lf", "first\nsecond\n", 2),
        ("crlf", "first\r\nsecond\r\n", 2),
        ("unterminated", "first\nsecond", 2),
        ("one", "one line", 1),
        ("blank", "\n\n", 2),
        ("unicode", "é\n日本語", 2),
    ] {
        repo.write(path, content);
        assert_eq!(
            stats::untracked(&repo.0.join(path)),
            LineStats::Text {
                added: lines,
                deleted: 0
            }
        );
    }
    let mut long = vec![b'a'; 65_539];
    long[32_767] = b'\n';
    long[65_535] = b'\n';
    std::fs::write(repo.0.join("long"), &long).unwrap();
    std::fs::write(repo.0.join("not-utf8"), b"\xff\n").unwrap();
    long[65_536] = 0;
    std::fs::write(repo.0.join("binary"), long).unwrap();
    let snapshot = read_snapshot(&repo.0).unwrap();
    assert_eq!(
        snapshot.counts[1]["long"],
        LineStats::Text {
            added: 3,
            deleted: 0
        }
    );
    assert_eq!(
        snapshot.counts[1]["not-utf8"],
        LineStats::Text {
            added: 1,
            deleted: 0
        }
    );
    assert_eq!(snapshot.counts[1]["binary"], LineStats::Binary);
    assert_eq!(
        snapshot.counts[1]["empty"],
        LineStats::Text {
            added: 0,
            deleted: 0
        }
    );
    assert_eq!(
        stats::untracked(&repo.0.join("missing")),
        LineStats::Unavailable
    );
    assert_eq!(stats::untracked(&repo.0), LineStats::Unavailable);
    std::os::unix::fs::symlink("missing", repo.0.join("broken-link")).unwrap();
    let snapshot = read_snapshot(&repo.0).unwrap();
    assert!(snapshot.files.iter().any(|file| file.path == "broken-link"));
    assert_eq!(snapshot.counts[1]["broken-link"], LineStats::Unavailable);
}

#[test]
fn snapshot_counts_follow_staged_working_renamed_deleted_and_binary_changes() {
    let repo = Repo::new();
    repo.write("partial", "first\nsecond\nthird\n");
    repo.write("old name", "unique rename contents\n");
    repo.write("deleted", "one\ntwo\n");
    std::fs::write(repo.0.join("binary"), b"old\0bytes").unwrap();
    repo.run(&["add", "."]);
    repo.run(&["-c", "core.hooksPath=/dev/null", "commit", "-qm", "fixture"]);
    repo.write("partial", "staged\nsecond\nthird\n");
    repo.run(&["add", "partial"]);
    repo.write("partial", "staged\nworking\nthird\n");
    repo.run(&["mv", "old name", "new\tname\n.rs"]);
    std::fs::remove_file(repo.0.join("deleted")).unwrap();
    std::fs::write(repo.0.join("binary"), b"new\0bytes").unwrap();
    repo.write("new file", "first\nsecond");
    let before = read_snapshot(&repo.0).unwrap();
    assert_eq!(
        before.counts[0]["partial"],
        LineStats::Text {
            added: 1,
            deleted: 1
        }
    );
    assert_eq!(
        before.counts[1]["partial"],
        LineStats::Text {
            added: 1,
            deleted: 1
        }
    );
    assert_eq!(
        before.counts[0]["new\tname\n.rs"],
        LineStats::Text {
            added: 0,
            deleted: 0
        }
    );
    assert!(!before.counts[0].contains_key("old name"));
    assert_eq!(
        before.counts[1]["deleted"],
        LineStats::Text {
            added: 0,
            deleted: 2
        }
    );
    assert_eq!(before.counts[1]["binary"], LineStats::Binary);
    repo.run(&["add", "new file"]);
    let after = read_snapshot(&repo.0).unwrap();
    assert_eq!(before.counts[1]["new file"], after.counts[0]["new file"]);
    assert_eq!(
        after.counts[0]["new file"],
        LineStats::Text {
            added: 2,
            deleted: 0
        }
    );
}

#[test]
fn status_letters_are_capitals_and_follow_the_selected_side() {
    for code in ['M', 'A', 'D', 'R', 'C', 'T'] {
        let file = Change {
            path: "file".into(),
            old: None,
            index: code,
            work: 'M',
        };
        assert_eq!(file.status_letter(Side::Staged), code);
        assert_eq!(file.status_letter(Side::Working), 'M');
    }
    for (index, work, letter) in [
        ('?', '?', 'N'),
        ('U', 'U', 'U'),
        ('A', 'A', 'U'),
        ('D', 'D', 'U'),
    ] {
        let file = Change {
            path: "file".into(),
            old: None,
            index,
            work,
        };
        assert_eq!(file.status_letter(Side::Working), letter);
    }
}
