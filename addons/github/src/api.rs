use focus_addon_sdk::{Connection, GitOperation, Service, ServiceReply, View};
use serde_json::{Value, json};
use std::io::{BufRead, Write};

pub trait Host {
    fn service(&mut self, generation: u64, service: Service) -> Result<ServiceReply, String>;
    fn publish(&mut self, view: &View) -> Result<(), String>;
}
impl<R: BufRead, W: Write> Host for Connection<R, W> {
    fn service(&mut self, generation: u64, service: Service) -> Result<ServiceReply, String> {
        self.call(generation, service).map_err(|e| e.to_string())
    }
    fn publish(&mut self, view: &View) -> Result<(), String> {
        self.view(view).map_err(|e| e.to_string())
    }
}
#[derive(Default)]
pub struct Api {
    pub no_gh: bool,
}
impl Api {
    pub fn request(
        &mut self,
        host: &mut impl Host,
        generation: u64,
        method: &str,
        endpoint: &str,
        body: Option<Value>,
    ) -> Result<Value, String> {
        let mut reply = None;
        if !self.no_gh {
            let mut args = vec![
                "api".into(),
                "--hostname".into(),
                "github.com".into(),
                "--method".into(),
                method.into(),
                endpoint.into(),
            ];
            if body.is_some() {
                args.extend(["--input".into(), "-".into()]);
            }
            let result = host.service(
                generation,
                Service::Run {
                    program: "gh".into(),
                    args,
                    input: body.as_ref().map(Value::to_string),
                    credential: true,
                },
            )?;
            if result.code == "missing_tool" {
                self.no_gh = true;
            } else {
                reply = Some(result);
            }
        }
        let reply = match reply {
            Some(reply) => reply,
            None => host.service(
                generation,
                Service::Http {
                    method: method.into(),
                    url: format!(
                        "https://api.github.com/{}",
                        endpoint.trim_start_matches('/')
                    ),
                    body,
                    credential: true,
                },
            )?,
        };
        let text = reply.result()?;
        if text.trim().is_empty() {
            return Ok(Value::Null);
        }
        let value: Value =
            serde_json::from_str(&text).map_err(|_| "GitHub returned an invalid response")?;
        if let Some(errors) = value.get("errors") {
            return Err(format!("GitHub: {errors}"));
        }
        Ok(value)
    }
    pub fn graphql(
        &mut self,
        host: &mut impl Host,
        generation: u64,
        query: &str,
        variables: Value,
    ) -> Result<Value, String> {
        self.request(
            host,
            generation,
            "POST",
            "graphql",
            Some(json!({"query":query,"variables":variables})),
        )
        .map(|v| v["data"].clone())
    }
}
pub fn git(
    host: &mut impl Host,
    generation: u64,
    operation: GitOperation,
) -> Result<String, String> {
    host.service(generation, Service::Git { operation })?
        .result()
}
pub fn read(host: &mut impl Host, generation: u64, args: &[&str]) -> Result<String, String> {
    git(
        host,
        generation,
        GitOperation::Read {
            args: args.iter().map(|s| s.to_string()).collect(),
        },
    )
}
pub fn segment(value: &str) -> String {
    url::form_urlencoded::byte_serialize(value.as_bytes()).collect()
}
pub fn string(value: &Value, key: &str) -> String {
    value[key].as_str().unwrap_or_default().to_owned()
}
