//! Session-scoped asynchronous operations, prompt admission, and Go.
use super::{OpenCodeUi, Operation, Pending, string};
use serde_json::{Value, json};
impl OpenCodeUi {
    pub(super) fn request(
        &mut self,
        operation: Operation,
        method: &str,
        path: String,
        body: Option<Value>,
    ) -> bool {
        log::debug!("OpenCode request method={method}");
        match self
            .server
            .as_mut()
            .ok_or("OpenCode is offline".to_string())
            .and_then(|s| s.request(method, path, body))
        {
            Ok(id) => {
                self.requests.insert(
                    id,
                    Pending {
                        operation,
                        epoch: self.epoch,
                    },
                );
                true
            }
            Err(error) => {
                self.error = Some(error.clone());
                self.status = error;
                false
            }
        }
    }
    pub(super) fn list_sessions(&mut self, resume: bool) {
        let mut url = reqwest::Url::parse("http://localhost/session").unwrap();
        url.query_pairs_mut()
            .append_pair("roots", "true")
            .append_pair("limit", &self.session_limit.to_string())
            .append_pair("path", &self.workspace.to_string_lossy());
        self.request(
            Operation::Sessions(resume),
            "GET",
            format!("/session?{}", url.query().unwrap_or("")),
            None,
        );
    }

    pub(super) fn select_session(&mut self, id: Option<String>) {
        log::info!("OpenCode session selection existing={}", id.is_some());
        if self.busy || !self.permissions.is_empty() || !self.questions.is_empty() {
            return;
        }
        self.epoch += 1;
        self.turn_cancelled = false;
        self.seen_events.clear();
        self.next_stream = false;
        self.session = id;
        self.messages.clear();
        self.session_info = Value::Null;
        self.history_before = None;
        self.permissions.clear();
        self.questions.clear();
        self.answers.clear();
        if self.session.is_some() {
            self.refresh();
        } else {
            self.request(
                Operation::Create,
                "POST",
                "/session".into(),
                Some(json!({})),
            );
        }
    }
    pub(super) fn refresh(&mut self) {
        self.last_refresh = Some(std::time::Instant::now());
        let Some(id) = self.session.clone() else {
            return;
        };
        for (operation, path) in [
            (Operation::Session, format!("/session/{id}")),
            (
                Operation::History(false),
                format!("/session/{id}/message?limit=100"),
            ),
            (Operation::Status, "/session/status".into()),
            (Operation::Permissions, "/permission".into()),
            (Operation::Questions, "/question".into()),
            (
                Operation::PermissionsV2,
                format!("/api/session/{id}/permission"),
            ),
            (
                Operation::QuestionsV2,
                format!("/api/session/{id}/question"),
            ),
        ] {
            if !self.requests.values().any(|p| {
                p.epoch == self.epoch
                    && std::mem::discriminant(&p.operation) == std::mem::discriminant(&operation)
            }) {
                self.request(operation, "GET", path, None);
            }
        }
    }
    pub(super) fn older_messages(&mut self) {
        if let (Some(id), Some(before)) = (self.session.clone(), self.history_before.clone()) {
            let mut path =
                reqwest::Url::parse(&format!("http://localhost/session/{id}/message")).unwrap();
            path.query_pairs_mut()
                .append_pair("limit", "100")
                .append_pair("before", &before);
            self.request(
                Operation::History(true),
                "GET",
                format!("{}?{}", path.path(), path.query().unwrap_or("")),
                None,
            );
        }
    }
    pub(super) fn can_send(&self) -> bool {
        self.online
            && self.stream_online
            && self.session.is_some()
            && self.model.is_some()
            && !self.agent.is_empty()
            && !self.locked()
    }
    pub(super) fn submit(&mut self, go: bool) {
        log::debug!("OpenCode prompt submission requested go={go}");
        if !self.can_send() || (go && !self.go_ready()) {
            return;
        }
        let text = if go {
            "Go".to_owned()
        } else {
            self.input.trim().to_string()
        };
        if text.is_empty() {
            return;
        }
        let previous_agent = go.then(|| std::mem::replace(&mut self.agent, "build".into()));
        let (provider, model) = self.model.as_ref().unwrap();
        let mut body = json!({"parts":[{"type":"text","text":text}],"model":{"providerID":provider,"modelID":model},"agent":self.agent});
        if !self.variant.is_empty() {
            body["variant"] = json!(self.variant);
        }
        self.error = None;
        self.turn_cancelled = false;
        self.busy = true;
        self.status = "OpenCode is running…".into();
        let admitted = self.request(
            Operation::Send {
                text,
                draft: !go,
                previous_agent: previous_agent.clone(),
            },
            "POST",
            format!("/session/{}/prompt_async", self.session.as_deref().unwrap()),
            Some(body),
        );
        if !admitted {
            self.busy = false;
            if let Some(agent) = previous_agent {
                self.agent = agent;
            }
        }
    }
    pub(super) fn go_ready(&self) -> bool {
        if self.turn_cancelled
            || self.agent != "plan"
            || !self.agents.iter().any(|a| a == "build")
            || self.busy
            || !self.permissions.is_empty()
            || !self.questions.is_empty()
        {
            return false;
        }
        self.messages.last().is_some_and(|m| {
            string(&m.info, "role") == "assistant"
                && string(&m.info, "agent") == "plan"
                && m.info["error"].is_null()
                && m.info.pointer("/time/completed").is_some()
                && m.parts
                    .iter()
                    .any(|p| string(p, "type") == "text" && !string(p, "text").trim().is_empty())
        })
    }
}
