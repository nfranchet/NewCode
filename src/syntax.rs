use eframe::egui::{Color32, FontId, TextFormat, text::LayoutJob};

pub(crate) fn colored_job(text: &str, ext: &str) -> LayoutJob {
    let mut job = LayoutJob::default();
    let lang = match ext {
        "rs" => "rust",
        "py" => "python",
        "js" | "jsx" | "ts" | "tsx" => "js",
        "json" => "json",
        "toml" => "toml",
        "html" | "htm" => "html",
        "css" | "scss" => "css",
        "sh" | "bash" => "sh",
        "sql" => "sql",
        "md" => "md",
        _ => "plain",
    };
    let keywords = match lang {
        "rust" => {
            "fn let mut pub use mod struct enum impl trait async await return if else match for while loop in as const static type Self self crate"
        }
        "python" => {
            "def class import from as return if elif else for while in try except finally with lambda yield async await True False None self"
        }
        "js" => {
            "const let var function return if else for while class import export from async await new this true false null"
        }
        "sql" => "SELECT FROM WHERE INSERT INTO UPDATE SET DELETE CREATE TABLE JOIN ON AND OR NULL",
        "sh" => "if then else fi for in do done case esac function export",
        _ => "",
    };
    let default = TextFormat {
        font_id: FontId::monospace(14.0),
        color: Color32::from_rgb(48, 49, 51),
        ..Default::default()
    };
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim_start();
        if trimmed.starts_with("//") || trimmed.starts_with('#') || trimmed.starts_with("<!--") {
            job.append(
                line,
                0.0,
                TextFormat {
                    color: Color32::from_rgb(0, 128, 0),
                    ..default.clone()
                },
            );
            continue;
        }
        let mut word = String::new();
        let mut in_string = false;
        let mut quote = '\0';
        for ch in line.chars() {
            if in_string {
                word.push(ch);
                if ch == quote {
                    job.append(
                        &word,
                        0.0,
                        TextFormat {
                            color: Color32::from_rgb(163, 21, 21),
                            ..default.clone()
                        },
                    );
                    word.clear();
                    in_string = false;
                }
            } else if ch == '\'' || ch == '"' || ch == '`' {
                if !word.is_empty() {
                    append_word(&mut job, &word, &default, keywords, lang);
                    word.clear();
                }
                word.push(ch);
                quote = ch;
                in_string = true;
            } else if ch.is_alphanumeric() || ch == '_' {
                word.push(ch);
            } else {
                if !word.is_empty() {
                    append_word(&mut job, &word, &default, keywords, lang);
                    word.clear();
                }
                job.append(&ch.to_string(), 0.0, default.clone());
            }
        }
        if !word.is_empty() {
            append_word(&mut job, &word, &default, keywords, lang);
        }
    }
    job
}
fn append_word(job: &mut LayoutJob, word: &str, fmt: &TextFormat, keywords: &str, lang: &str) {
    let color = if keywords
        .split_whitespace()
        .any(|k| k.eq_ignore_ascii_case(word))
    {
        Color32::from_rgb(175, 0, 219)
    } else if word.parse::<f64>().is_ok() {
        Color32::from_rgb(9, 134, 88)
    } else if lang == "json" && word.ends_with(':') {
        Color32::from_rgb(4, 81, 165)
    } else {
        fmt.color
    };
    job.append(
        word,
        0.0,
        TextFormat {
            color,
            ..fmt.clone()
        },
    );
}
