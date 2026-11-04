//! Workspace search, Unicode-aware matching, Find navigation, and highlighting.

use super::EditorApp;
use crate::ui_theme::{self as theme, Icon};
use eframe::egui::{
    self, Color32,
    text::{LayoutJob, LayoutSection},
};
use std::{
    fs,
    path::{Path, PathBuf},
};

pub(super) struct FileMatches {
    path: PathBuf,
    lines: Vec<(usize, String)>,
}

fn collect_search_files(dir: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_dir() {
            if entry.file_name() != ".git" {
                collect_search_files(&path, files);
            }
        } else if kind.is_file() {
            files.push(path);
        }
    }
}

fn find_ranges(text: &str, query: &str) -> Vec<(usize, usize)> {
    let needle = query.to_lowercase();
    if needle.is_empty() {
        return Vec::new();
    }
    let mut folded = String::new();
    let mut original_char = Vec::new();
    for (index, ch) in text.chars().enumerate() {
        for lower in ch.to_lowercase() {
            folded.push(lower);
            original_char.push(index);
        }
    }
    folded
        .match_indices(&needle)
        .filter_map(|(byte, _)| {
            let start_folded = folded[..byte].chars().count();
            let end_folded = start_folded + needle.chars().count();
            let start = *original_char.get(start_folded)?;
            let end = original_char.get(end_folded.saturating_sub(1))? + 1;
            Some((start, end))
        })
        .collect()
}

pub(super) fn highlight_find_matches(
    job: &mut LayoutJob,
    text: &str,
    query: &str,
    active_match: Option<usize>,
) {
    let matches = find_ranges(text, query);
    if matches.is_empty() {
        return;
    }
    let char_bytes: Vec<usize> = text
        .char_indices()
        .map(|(byte, _)| byte)
        .chain(std::iter::once(text.len()))
        .collect();
    let byte_matches: Vec<(usize, usize)> = matches
        .iter()
        .filter_map(|(start, end)| Some((*char_bytes.get(*start)?, *char_bytes.get(*end)?)))
        .collect();
    let active_bytes = active_match.and_then(|index| byte_matches.get(index).copied());
    let original_sections = std::mem::take(&mut job.sections);
    for section in original_sections {
        let section_start = section.byte_range.start;
        let section_end = section.byte_range.end;
        let mut boundaries = vec![section_start, section_end];
        for (start, end) in &byte_matches {
            if *start < section_end && *end > section_start {
                boundaries.push((*start).max(section_start));
                boundaries.push((*end).min(section_end));
            }
        }
        boundaries.sort_unstable();
        boundaries.dedup();
        for pair in boundaries.windows(2) {
            let start = pair[0];
            let end = pair[1];
            if start == end {
                continue;
            }
            let mut part = section.clone();
            part.byte_range = start..end;
            if let Some((active_start, active_end)) = active_bytes
                && start >= active_start
                && end <= active_end
            {
                part.format.background = Color32::from_rgb(255, 190, 100);
            } else if byte_matches
                .iter()
                .any(|(match_start, match_end)| start >= *match_start && end <= *match_end)
            {
                part.format.background = Color32::from_rgb(255, 239, 166);
            }
            job.sections.push(LayoutSection {
                leading_space: part.leading_space,
                byte_range: part.byte_range,
                format: part.format,
            });
        }
    }
}

impl EditorApp {
    pub(super) fn search_workspace(&mut self) {
        let query = self.search_query.trim().to_lowercase();
        self.search_results.clear();
        if query.is_empty() {
            self.status = "Enter text to search for".into();
            return;
        }
        let Some(root) = self.settings.workspace.clone() else {
            self.status = "Choose a workspace before searching".into();
            return;
        };
        let mut paths = Vec::new();
        collect_search_files(&root, &mut paths);
        paths.sort();
        let mut matched_lines = 0usize;
        for path in paths {
            let Ok(contents) = fs::read_to_string(&path) else {
                continue;
            };
            let lines: Vec<_> = contents
                .lines()
                .enumerate()
                .filter_map(|(index, line)| {
                    line.to_lowercase()
                        .contains(&query)
                        .then(|| (index + 1, line.trim().to_string()))
                })
                .collect();
            if !lines.is_empty() {
                matched_lines += lines.len();
                self.search_results.push(FileMatches { path, lines });
            }
        }
        self.status = if matched_lines == 0 {
            format!("No matches for ‘{}’", self.search_query.trim())
        } else {
            format!(
                "Found {matched_lines} matching lines in {} files",
                self.search_results.len()
            )
        };
    }

    pub(super) fn open_search_result(&mut self, path: PathBuf, line_number: usize) {
        self.open_file(path);
        if self.active_file.is_some() {
            self.pending_editor_cursor = Some(
                self.buffer
                    .split_inclusive('\n')
                    .take(line_number.saturating_sub(1))
                    .map(|line| line.chars().count())
                    .sum(),
            );
        }
    }

    pub(super) fn navigate_find(&mut self, ctx: &egui::Context, reverse: bool) {
        let matches = find_ranges(&self.buffer, &self.find_query);
        if matches.is_empty() {
            self.find_active = None;
            self.status = if self.find_query.is_empty() {
                "Enter text to find in this file".into()
            } else {
                "No matches in this file".into()
            };
            return;
        }
        let index = match self.find_active {
            Some(current) if reverse => (current + matches.len() - 1) % matches.len(),
            Some(current) => (current + 1) % matches.len(),
            None if reverse => matches.len() - 1,
            None => 0,
        };
        self.find_active = Some(index);
        self.pending_editor_cursor = Some(matches[index].0);
        ctx.memory_mut(|memory| memory.request_focus(egui::Id::new("find_in_file_query")));
    }
    pub(super) fn search_panel(&mut self, ui: &mut egui::Ui) {
        let mut run_search = false;
        ui.horizontal(|ui| {
            let response = ui.add(
                egui::TextEdit::singleline(&mut self.search_query)
                    .hint_text("Search in project…")
                    .desired_width((ui.available_width() - 36.0).max(40.0)),
            );
            if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                run_search = true;
            }
            if theme::button(ui, Icon::Search, "", theme::BLUE, false)
                .on_hover_text("Search project · Enter")
                .clicked()
            {
                run_search = true;
            }
        });
        if run_search {
            self.search_workspace();
        }
        let mut open_result = None;
        egui::ScrollArea::vertical().show(ui, |ui| {
            if self.search_results.is_empty() {
                ui.vertical_centered(|ui| {
                    ui.add_space(12.0);
                    theme::empty_art(ui, Icon::Search, theme::BLUE);
                    ui.strong("Find across your project");
                    ui.label("Search saved file contents in the current workspace.");
                });
            }
            for group in &self.search_results {
                let label = group
                    .path
                    .strip_prefix(self.settings.workspace.as_deref().unwrap_or(Path::new("")))
                    .unwrap_or(&group.path)
                    .display()
                    .to_string();
                ui.add(egui::Label::new(theme::file_text(&group.path, &label)).wrap())
                    .on_hover_text(group.path.display().to_string());
                for (line, preview) in &group.lines {
                    let result = ui.selectable_label(
                        false,
                        egui::RichText::new(format!("{line:>4}  {preview}"))
                            .monospace()
                            .size(12.0)
                            .color(Color32::from_rgb(95, 95, 95)),
                    );
                    if result.clicked() {
                        open_result = Some((group.path.clone(), *line));
                    }
                }
                ui.add_space(4.0);
            }
        });
        if let Some((path, line)) = open_result {
            self.open_search_result(path, line);
        }
    }
    pub(super) fn find_bar(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, opened_find: bool) {
        if self.find_open {
            let matches = find_ranges(&self.buffer, &self.find_query);
            let mut query_changed = false;
            let mut navigate = None;
            let mut close_find = false;
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Find").strong());
                let query = ui.add(
                    egui::TextEdit::singleline(&mut self.find_query)
                        .id(egui::Id::new("find_in_file_query"))
                        .hint_text("Search in this file")
                        .desired_width(210.0),
                );
                if opened_find {
                    query.request_focus();
                }
                query_changed = query.changed();
                if query.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)) {
                    navigate = Some(ui.input(|input| input.modifiers.shift));
                }
                if ui.button("↑").on_hover_text("Previous match").clicked() {
                    navigate = Some(true);
                }
                if ui.button("↓").on_hover_text("Next match").clicked() {
                    navigate = Some(false);
                }
                ui.label(format!(
                    "{} / {}",
                    self.find_active.map(|index| index + 1).unwrap_or(0),
                    matches.len()
                ));
                close_find = ui.button("×").on_hover_text("Close find").clicked();
            });
            if query_changed || self.find_query_snapshot != self.find_query {
                self.find_query_snapshot = self.find_query.clone();
                self.find_active = None;
            }
            if let Some(reverse) = navigate {
                self.navigate_find(ctx, reverse);
            }
            if close_find {
                self.find_open = false;
                self.find_query.clear();
                self.find_query_snapshot.clear();
                self.find_active = None;
            }
            if ui.input(|input| input.key_pressed(egui::Key::Escape)) {
                self.find_open = false;
                self.find_active = None;
            }
        }
    }
}
