//! Usage refresh scheduling and account/token/context display.

use super::CodexUi;
use eframe::egui;
use serde_json::{Value, json};
use std::time::{Duration, Instant};

fn show_limits(ui: &mut egui::Ui, usage: &Value) {
    let snapshots: Vec<&Value> = usage
        .get("rateLimitsByLimitId")
        .and_then(Value::as_object)
        .map(|m| m.values().collect())
        .unwrap_or_else(|| usage.get("rateLimits").into_iter().collect());
    for snapshot in snapshots {
        ui.small(
            snapshot
                .get("limitName")
                .and_then(Value::as_str)
                .unwrap_or("Account limits"),
        );
        for key in ["primary", "secondary"] {
            if let Some(window) = snapshot.get(key).filter(|v| !v.is_null()) {
                let used = window
                    .get("usedPercent")
                    .and_then(Value::as_f64)
                    .unwrap_or_default();
                let minutes = window.get("windowDurationMins").and_then(Value::as_i64);
                ui.small(format!(
                    "{}: {:.0}% remaining",
                    minutes
                        .map(|m| format!("{m} min window"))
                        .unwrap_or(key.into()),
                    (100.0 - used).max(0.0)
                ));
                if let Some(reset) = window.get("resetsAt").and_then(Value::as_u64) {
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs();
                    ui.small(format!(
                        "Resets in {} min",
                        reset.saturating_sub(now).div_ceil(60)
                    ));
                }
            }
        }
    }
}

pub(super) fn usage_title(usage: &Value) -> String {
    let snapshot = usage.get("rateLimits");
    if let Some(used) = snapshot
        .and_then(|v| v.pointer("/primary/usedPercent"))
        .and_then(Value::as_f64)
    {
        format!("Usage / status · {:.0}% remaining", (100.0 - used).max(0.0))
    } else {
        "Usage / status".into()
    }
}

impl CodexUi {
    pub(super) fn refresh_usage(&mut self) {
        self.last_usage = Some(Instant::now());
        if let Err(e) = self.send("account/rateLimits/read", json!({})) {
            self.usage_error = e;
        }
    }
    pub(super) fn show_usage(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        if self.server.is_some() && self.initializing_id.is_none() {
            ctx.request_repaint_after(Duration::from_secs(1));
            if self
                .last_usage
                .is_none_or(|t| t.elapsed() >= Duration::from_secs(60))
                && !self
                    .requests
                    .values()
                    .any(|m| m == "account/rateLimits/read")
            {
                self.refresh_usage();
            }
        }
        egui::CollapsingHeader::new(usage_title(&self.usage)).show(ui, |ui| {
            if let Some(id) = &self.thread_id {
                ui.small(format!("Session: {id}"));
            }
            ui.small(format!("Model: {}", self.model));
            if !self.usage_error.is_empty() {
                ui.small(&self.usage_error);
            }
            if self.usage.is_null() {
                ui.small("Account usage unavailable");
            } else {
                show_limits(ui, &self.usage);
            }
            if let Some(total) = self
                .token_usage
                .pointer("/total/totalTokens")
                .and_then(Value::as_i64)
            {
                ui.small(format!("Session tokens: {total}"));
            }
            if let (Some(tokens), Some(window)) = (
                self.token_usage
                    .pointer("/last/totalTokens")
                    .and_then(Value::as_f64),
                self.token_usage
                    .get("modelContextWindow")
                    .and_then(Value::as_f64),
            ) {
                ui.small(format!(
                    "Context: {:.0}% used",
                    tokens / window.max(1.0) * 100.0
                ));
            }
            if ui
                .add_enabled(self.server.is_some(), egui::Button::new("Refresh usage"))
                .clicked()
            {
                self.refresh_usage();
            }
        });
    }
}
