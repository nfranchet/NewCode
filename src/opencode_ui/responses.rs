//! Apply only responses belonging to the current session generation.
use super::{OpenCodeUi, Operation, string};
use serde_json::Value;
impl OpenCodeUi {
    pub(super) fn reply(&mut self, id: u64, result: Result<Value, String>) {
        let Some(pending) = self.requests.remove(&id) else {
            return;
        };
        if pending.epoch != self.epoch
            && !matches!(
                pending.operation,
                Operation::Providers | Operation::Agents | Operation::Sessions(_)
            )
        {
            return;
        }
        let value = match result {
            Ok(value) => value,
            Err(error) => {
                if matches!(
                    pending.operation,
                    Operation::PermissionsV2 | Operation::QuestionsV2
                ) && error.contains("404")
                {
                    return;
                }
                self.error = Some(error.clone());
                if let Operation::Send { previous_agent, .. } = &pending.operation {
                    self.busy = false;
                    if let Some(agent) = previous_agent {
                        self.agent = agent.clone();
                    }
                    self.refresh();
                }
                if matches!(pending.operation, Operation::Session) && error.contains("404") {
                    self.session = None;
                    self.select_session(None);
                }
                self.status = error;
                return;
            }
        };
        match pending.operation {
            Operation::Providers => self.load_models(value),
            Operation::Agents => self.load_agents(value),
            Operation::Sessions(resume) => {
                self.sessions = value
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|s| {
                        s["parentID"].is_null() && {
                            let path = std::path::Path::new(string(s, "directory"));
                            path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
                                == self.workspace
                        }
                    })
                    .cloned()
                    .collect();
                self.sessions.sort_by_key(|s| {
                    std::cmp::Reverse(
                        s.pointer("/time/updated")
                            .and_then(Value::as_u64)
                            .unwrap_or(0),
                    )
                });
                if resume {
                    let chosen = self
                        .session
                        .clone()
                        .or_else(|| self.sessions.first().map(|s| string(s, "id").to_owned()));
                    self.select_session(chosen);
                }
            }
            Operation::Create => {
                self.session = value["id"].as_str().map(str::to_owned);
                self.session_info = value;
                self.refresh();
                self.list_sessions(false);
            }
            Operation::Session => self.session_info = value,
            Operation::History(older) => {
                let rows = value.as_array().cloned().unwrap_or_default();
                if older || self.history_before.is_none() {
                    self.history_before = (rows.len() >= 100)
                        .then(|| {
                            rows.first()
                                .and_then(|v| v.pointer("/info/id"))
                                .and_then(Value::as_str)
                                .unwrap_or("")
                                .to_owned()
                        })
                        .filter(|s| !s.is_empty());
                }
                for row in rows {
                    self.merge_message(
                        row["info"].clone(),
                        row["parts"].as_array().cloned().unwrap_or_default(),
                    );
                }
            }
            Operation::Status => {
                if let Some(id) = &self.session {
                    self.busy = value[id]["type"].as_str().is_some_and(|s| s != "idle");
                }
            }
            Operation::Permissions => {
                let mut requests = self.session_requests(value);
                requests.extend(
                    self.permissions
                        .iter()
                        .filter(|p| p["_v2"] == true)
                        .cloned(),
                );
                self.permissions = requests;
            }
            Operation::Questions => {
                let mut requests = self.session_requests(value);
                requests.extend(self.questions.iter().filter(|p| p["_v2"] == true).cloned());
                self.questions = requests;
            }
            Operation::PermissionsV2 | Operation::QuestionsV2 => {
                let question = matches!(pending.operation, Operation::QuestionsV2);
                let mut rows = value.as_array().cloned().unwrap_or_default();
                for row in &mut rows {
                    row["_v2"] = Value::Bool(true);
                    row["sessionID"] = serde_json::json!(self.session);
                }
                let target = if question {
                    &mut self.questions
                } else {
                    &mut self.permissions
                };
                target.retain(|r| r["_v2"] != true);
                for row in rows {
                    if let Some(old) = target.iter_mut().find(|r| r["id"] == row["id"]) {
                        *old = row;
                    } else {
                        target.push(row);
                    }
                }
            }
            Operation::Send { text, draft, .. } => {
                if draft && self.input.trim() == text {
                    self.input.clear();
                }
            }
            Operation::Abort => {
                self.turn_cancelled = true;
                self.busy = false;
                self.refresh();
            }
            Operation::Answer { question, id } => {
                if question {
                    self.questions.retain(|q| string(q, "id") != id);
                    self.answers.remove(&id);
                } else {
                    self.permissions.retain(|p| string(p, "id") != id);
                }
            }
        }
    }
    fn session_requests(&self, value: Value) -> Vec<Value> {
        value
            .as_array()
            .into_iter()
            .flatten()
            .filter(|r| r["sessionID"].as_str() == self.session.as_deref())
            .cloned()
            .collect()
    }
}
