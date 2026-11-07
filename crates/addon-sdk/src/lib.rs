//! UI-independent, versioned executable-addon contract for Focus Editor.
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, VecDeque},
    io::{self, BufRead, Write},
    path::PathBuf,
};

pub const PROTOCOL_VERSION: u32 = 1;
pub const MAX_MESSAGE: usize = 8 * 1024 * 1024;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Manifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub protocol: u32,
    pub executable: PathBuf,
    pub icon: PathBuf,
    #[serde(default)]
    pub credential: Option<Credential>,
    #[serde(default)]
    pub panel: Panel,
    #[serde(default)]
    pub ui: Presentation,
    #[serde(default = "yes")]
    pub enabled_by_default: bool,
    #[serde(default)]
    pub environment: Vec<String>,
    #[serde(default)]
    pub migration: Migration,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Panel {
    #[default]
    Left,
    Right,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Presentation {
    #[default]
    Widgets,
    Surface,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Migration {
    #[serde(default)]
    pub enabled: Vec<String>,
    pub selection: Option<String>,
    pub sessions: Option<String>,
    pub autostart: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Credential {
    /// Only this HTTPS origin receives a stored credential.
    pub origin: String,
    /// Only this executable may receive the token via `environment`.
    pub program: String,
    pub environment: String,
    #[serde(default)]
    pub git_origin: Option<String>,
    #[serde(default)]
    pub instructions: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Context {
    pub generation: u64,
    pub workspace: Option<PathBuf>,
    pub storage: PathBuf,
    #[serde(default)]
    pub runtime: Option<Runtime>,
    #[serde(default)]
    pub state: Value,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Runtime {
    pub host_workspace: PathBuf,
    pub container_id: String,
    pub workspace_folder: PathBuf,
    pub remote_user: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct View {
    pub generation: u64,
    pub form: String,
    pub title: String,
    #[serde(default)]
    pub busy: bool,
    #[serde(default)]
    pub reveal_detail: bool,
    pub sidebar: Vec<Widget>,
    pub detail: Vec<Widget>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Widget {
    Text {
        text: String,
    },
    Markdown {
        text: String,
    },
    Button {
        id: String,
        label: String,
        #[serde(default = "yes")]
        enabled: bool,
    },
    Input {
        id: String,
        label: String,
        value: String,
        #[serde(default)]
        multiline: bool,
    },
    Checkbox {
        id: String,
        label: String,
        checked: bool,
    },
    Select {
        id: String,
        label: String,
        value: String,
        options: Vec<(String, String)>,
    },
    Link {
        label: String,
        url: String,
    },
    Row {
        children: Vec<Widget>,
    },
    Separator,
}
fn yes() -> bool {
    true
}
impl Widget {
    pub fn text(text: impl Into<String>) -> Self {
        Self::Text { text: text.into() }
    }
    pub fn button(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self::Button {
            id: id.into(),
            label: label.into(),
            enabled: true,
        }
    }
    pub fn input(
        id: impl Into<String>,
        label: impl Into<String>,
        value: impl Into<String>,
    ) -> Self {
        Self::Input {
            id: id.into(),
            label: label.into(),
            value: value.into(),
            multiline: false,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Action {
    pub generation: u64,
    pub id: String,
    pub values: BTreeMap<String, Value>,
}
impl Action {
    pub fn text(&self, id: &str) -> String {
        self.values
            .get(id)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned()
    }
    pub fn checked(&self, id: &str) -> bool {
        self.values
            .get(id)
            .and_then(Value::as_bool)
            .unwrap_or(false)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Service {
    /// Argument arrays only; no implicit shell. Missing program has code `missing_tool`.
    Run {
        program: String,
        args: Vec<String>,
        input: Option<String>,
        credential: bool,
    },
    Http {
        method: String,
        url: String,
        body: Option<Value>,
        credential: bool,
    },
    Git {
        operation: GitOperation,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
pub enum GitOperation {
    Read {
        args: Vec<String>,
    },
    Fetch {
        remote: String,
        branch: String,
    },
    Checkout {
        branch: String,
        remote: String,
    },
    Push {
        remote: String,
        branch: String,
    },
    DeleteLocal {
        branch: String,
        expected: String,
        base: String,
    },
    DeleteRemote {
        remote: String,
        branch: String,
        expected: String,
    },
}
impl GitOperation {
    pub fn changes_worktree(&self) -> bool {
        matches!(self, Self::Checkout { .. } | Self::DeleteLocal { .. })
    }
    pub fn mutation(&self) -> bool {
        !matches!(self, Self::Read { .. })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ServiceCall {
    pub generation: u64,
    pub service: Service,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ServiceReply {
    pub ok: bool,
    pub code: String,
    pub output: String,
}
impl ServiceReply {
    pub fn success(output: impl Into<String>) -> Self {
        Self {
            ok: true,
            code: String::new(),
            output: output.into(),
        }
    }
    pub fn error(code: &str, output: impl Into<String>) -> Self {
        Self {
            ok: false,
            code: code.into(),
            output: output.into(),
        }
    }
    pub fn result(self) -> Result<String, String> {
        if self.ok {
            Ok(self.output)
        } else {
            Err(self.output)
        }
    }
}

/// JSON-RPC 2.0, one bounded JSON object per line. No binary/egui/Rust ABI crosses this boundary.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Message {
    pub jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<Value>,
}
impl Message {
    pub fn notification(method: &str, params: impl Serialize) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id: None,
            method: Some(method.into()),
            params: Some(serde_json::to_value(params).unwrap()),
            result: None,
            error: None,
        }
    }
    pub fn response(id: u64, result: impl Serialize) -> Self {
        Self {
            id: Some(id),
            method: None,
            params: None,
            result: Some(serde_json::to_value(result).unwrap()),
            ..Self::notification("", ())
        }
    }
    pub fn request(id: u64, method: &str, params: impl Serialize) -> Self {
        Self {
            id: Some(id),
            ..Self::notification(method, params)
        }
    }
}
pub fn read_message(reader: &mut impl BufRead) -> io::Result<Option<Message>> {
    let mut bytes = Vec::new();
    loop {
        let chunk = reader.fill_buf()?;
        if chunk.is_empty() {
            return if bytes.is_empty() {
                Ok(None)
            } else {
                Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "Incomplete addon message",
                ))
            };
        }
        let count = chunk
            .iter()
            .position(|c| *c == b'\n')
            .map_or(chunk.len(), |i| i + 1);
        if bytes.len() + count > MAX_MESSAGE {
            return Err(io::Error::other("Addon message exceeds size limit"));
        }
        let done = chunk[count - 1] == b'\n';
        bytes.extend_from_slice(&chunk[..count]);
        reader.consume(count);
        if done {
            break;
        }
    }
    let message: Message = serde_json::from_slice(&bytes)?;
    if message.jsonrpc != "2.0" {
        return Err(io::Error::other("Unsupported JSON-RPC version"));
    }
    Ok(Some(message))
}
pub fn write_message(writer: &mut impl Write, message: &Message) -> io::Result<()> {
    let bytes = serde_json::to_vec(message)?;
    if bytes.len() >= MAX_MESSAGE {
        return Err(io::Error::other("Addon message exceeds size limit"));
    }
    writer.write_all(&bytes)?;
    writer.write_all(b"\n")?;
    writer.flush()
}

pub struct Connection<R, W> {
    reader: R,
    writer: W,
    queued: VecDeque<Message>,
    next_id: u64,
}
impl<R: BufRead, W: Write> Connection<R, W> {
    pub fn new(reader: R, writer: W) -> Self {
        Self {
            reader,
            writer,
            queued: VecDeque::new(),
            next_id: 1,
        }
    }
    pub fn receive(&mut self) -> io::Result<Option<Message>> {
        if let Some(message) = self.queued.pop_front() {
            return Ok(Some(message));
        }
        read_message(&mut self.reader)
    }
    pub fn send(&mut self, message: Message) -> io::Result<()> {
        write_message(&mut self.writer, &message)
    }
    pub fn view(&mut self, view: &View) -> io::Result<()> {
        self.send(Message::notification("view.update", view))
    }
    pub fn call(&mut self, generation: u64, service: Service) -> io::Result<ServiceReply> {
        let id = self.next_id;
        self.next_id += 1;
        self.send(Message::request(
            id,
            "host.call",
            ServiceCall {
                generation,
                service,
            },
        ))?;
        loop {
            let message = read_message(&mut self.reader)?
                .ok_or_else(|| io::Error::other("Editor disconnected"))?;
            if message.id == Some(id) && message.method.is_none() {
                return serde_json::from_value(message.result.unwrap_or_else(
                    || json!({"ok":false,"code":"protocol","output":"Invalid host reply"}),
                ))
                .map_err(io::Error::other);
            }
            if self.queued.len() >= 128 {
                return Err(io::Error::other("Too many pending addon events"));
            }
            self.queued.push_back(message);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn framing_and_interleaved_events() {
        let mut wire = Vec::new();
        write_message(
            &mut wire,
            &Message::notification("context.changed", Context::default()),
        )
        .unwrap();
        write_message(
            &mut wire,
            &Message::response(1, ServiceReply::success("ok")),
        )
        .unwrap();
        let mut client = Connection::new(io::Cursor::new(wire), Vec::new());
        let reply = client
            .call(
                0,
                Service::Git {
                    operation: GitOperation::Read {
                        args: vec!["status".into()],
                    },
                },
            )
            .unwrap();
        assert_eq!(reply.output, "ok");
        assert_eq!(
            client.receive().unwrap().unwrap().method.as_deref(),
            Some("context.changed")
        );
    }
    #[test]
    fn rejects_truncated_and_oversized_frames() {
        assert!(read_message(&mut io::Cursor::new(b"{")).is_err());
        assert!(read_message(&mut io::Cursor::new(vec![b' '; MAX_MESSAGE + 1])).is_err());
    }
}
