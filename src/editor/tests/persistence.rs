use crate::{editor::persistence::restore_open_files, settings::Settings};

#[test]
fn restores_order_active_file_and_skips_missing_files() {
    let root = std::env::temp_dir().join(format!("focus-session-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let first = root.join("first.txt");
    let second = root.join("second.txt");
    std::fs::write(&first, "first").unwrap();
    std::fs::write(&second, "second").unwrap();
    let mut settings = Settings {
        open_files: vec![first.clone(), root.join("missing.txt"), second.clone()],
        active_file: Some(second.clone()),
        ..Settings::default()
    };
    assert_eq!(
        restore_open_files(&settings),
        (
            vec![first.clone(), second.clone()],
            Some(second),
            "second".into()
        )
    );
    settings.active_file = Some(root.join("missing.txt"));
    assert_eq!(restore_open_files(&settings).1, Some(first));
    settings.workspace = Some(root.clone());
    assert!(restore_open_files(&settings).0.is_empty());
    let old: Settings = toml::from_str("").unwrap();
    assert!(restore_open_files(&old).0.is_empty());
    std::fs::remove_dir_all(root).unwrap();
}
