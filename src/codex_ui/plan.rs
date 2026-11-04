//! Completed-plan readiness and the explicit Go action that continues in Default mode.

use super::CodexUi;
use serde_json::Value;

#[derive(Clone)]
struct ReadyPlan {
    thread_id: String,
    turn_id: Option<String>,
    item_id: String,
}

struct GoSubmission {
    request_id: u64,
    plan: ReadyPlan,
    previous_mode: Option<Value>,
}

#[derive(Default)]
pub(super) struct PlanState {
    candidate: Option<ReadyPlan>,
    ready: Option<ReadyPlan>,
    submission: Option<GoSubmission>,
}

fn plan_item(item: &Value, thread_id: &str, turn_id: Option<&str>) -> Option<ReadyPlan> {
    if item.get("type").and_then(Value::as_str) != Some("plan")
        || item.get("text").and_then(Value::as_str)?.trim().is_empty()
    {
        return None;
    }
    let item_id = item.get("id")?.as_str()?.to_string();
    if item_id.is_empty() {
        return None;
    }
    Some(ReadyPlan {
        thread_id: thread_id.into(),
        turn_id: turn_id.map(str::to_string),
        item_id,
    })
}

impl CodexUi {
    pub(super) fn clear_plan(&mut self) {
        self.plan = PlanState::default();
    }

    pub(super) fn observe_plan_item(&mut self, method: &str, params: &Value) {
        if method != "item/completed" {
            return;
        }
        let Some(item) = params.get("item") else {
            return;
        };
        if item.get("type").and_then(Value::as_str) != Some("plan") {
            return;
        }
        let Some(thread_id) = self.thread_id.as_deref() else {
            return;
        };
        let turn_id = params
            .get("turnId")
            .and_then(Value::as_str)
            .or(self.turn_id.as_deref());
        if let (Some(current), Some(event)) = (self.turn_id.as_deref(), turn_id) {
            if current != event {
                return;
            }
        }
        self.plan.candidate = plan_item(item, thread_id, turn_id);
    }

    pub(super) fn complete_plan_turn(&mut self, params: &Value) {
        // A delayed duplicate completion must not undo a queued Go submission.
        if self.plan.submission.is_some() {
            return;
        }
        let turn_id = params
            .pointer("/turn/id")
            .and_then(Value::as_str)
            .or(self.turn_id.as_deref());
        let successful = params.pointer("/turn/status").and_then(Value::as_str)
            == Some("completed")
            && params.pointer("/turn/error").is_none_or(Value::is_null);
        let candidate = self.plan.candidate.take();
        if successful {
            let previous = self
                .plan
                .ready
                .take()
                .filter(|plan| plan.turn_id.as_deref() == turn_id);
            self.plan.ready = candidate.or(previous).filter(|plan| {
                Some(plan.thread_id.as_str()) == self.thread_id.as_deref()
                    && plan.turn_id.as_deref().is_none_or(|id| Some(id) == turn_id)
            });
        } else {
            self.plan.ready = None;
        }
    }

    pub(super) fn restore_plan(&mut self, thread: &Value) {
        self.clear_plan();
        let Some(thread_id) = self.thread_id.as_deref() else {
            return;
        };
        let Some(turn) = thread
            .get("turns")
            .and_then(Value::as_array)
            .and_then(|turns| turns.last())
        else {
            return;
        };
        if turn.get("status").and_then(Value::as_str) != Some("completed")
            || turn.get("error").is_some_and(|error| !error.is_null())
        {
            return;
        }
        let Some(items) = turn.get("items").and_then(Value::as_array) else {
            return;
        };
        self.plan.ready = items
            .iter()
            .rev()
            .find_map(|item| plan_item(item, thread_id, turn.get("id").and_then(Value::as_str)));
    }

    pub(super) fn go_plan_id(&self) -> Option<&str> {
        if self.mode.as_ref()?.get("mode").and_then(Value::as_str) != Some("plan") {
            return None;
        }
        self.plan
            .ready
            .as_ref()
            .filter(|plan| Some(plan.thread_id.as_str()) == self.thread_id.as_deref())
            .map(|plan| plan.item_id.as_str())
    }

    pub(super) fn go_unavailable_reason(&self) -> Option<&'static str> {
        if self.go_plan_id().is_none() {
            return Some("No completed plan is ready");
        }
        if self.busy || self.plan.submission.is_some() {
            return Some("Codex is running");
        }
        if !self.pending.is_empty() {
            return Some("Respond to Codex first");
        }
        if self.server.is_none() {
            return Some("Start Codex to continue");
        }
        if self.initializing_id.is_some()
            || self.requests.values().any(|method| {
                matches!(
                    method.as_str(),
                    "thread/start" | "thread/resume" | "turn/start" | "collaborationMode/list"
                )
            })
        {
            return Some("Wait for Codex to finish connecting or opening the session");
        }
        if self.model.is_empty() {
            return Some("Wait for a model to be available");
        }
        if !self
            .modes
            .iter()
            .any(|mode| mode.get("mode").and_then(Value::as_str) == Some("default"))
        {
            return Some("Default mode is not available");
        }
        None
    }

    pub(super) fn go(&mut self) -> Result<u64, String> {
        if let Some(reason) = self.go_unavailable_reason() {
            return Err(reason.into());
        }
        let plan = self
            .plan
            .ready
            .clone()
            .ok_or("No completed plan is ready")?;
        let default_mode = self
            .modes
            .iter()
            .find(|mode| mode.get("mode").and_then(Value::as_str) == Some("default"))
            .cloned()
            .ok_or("Default mode is not available")?;
        let previous_mode = self.mode.replace(default_mode);
        match self.submit_text("Go") {
            Ok(request_id) => {
                self.plan.ready = None;
                self.plan.submission = Some(GoSubmission {
                    request_id,
                    plan,
                    previous_mode,
                });
                Ok(request_id)
            }
            Err(error) => {
                self.mode = previous_mode;
                self.status = error.clone();
                Err(error)
            }
        }
    }

    pub(super) fn reject_go(&mut self, id: &Value) {
        if !self
            .plan
            .submission
            .as_ref()
            .is_some_and(|submission| Some(submission.request_id) == id.as_u64())
        {
            return;
        }
        let submission = self.plan.submission.take().unwrap();
        if Some(submission.plan.thread_id.as_str()) == self.thread_id.as_deref() {
            self.mode = submission.previous_mode;
            self.plan.ready = Some(submission.plan);
        }
    }
}
