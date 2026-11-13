use crate::logging::Arguments;
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    io,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::PathBuf,
};
use subtle::ConstantTimeEq;

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileConfig {
    bind: Option<IpAddr>,
    port: Option<u16>,
    username: Option<String>,
    password: Option<String>,
}

pub(crate) struct Config {
    pub address: SocketAddr,
    authorization: [u8; 32],
}

impl Config {
    pub fn path() -> io::Result<PathBuf> {
        dirs::config_dir()
            .map(|path| path.join("focus-editor/server.toml"))
            .ok_or_else(|| io::Error::other("Cannot find the server configuration directory"))
    }

    pub fn load(args: &Arguments) -> Result<Self, Box<dyn std::error::Error>> {
        let path = match &args.server_config {
            Some(path) => path.clone(),
            None => Self::path()?,
        };
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => Some(text),
            Err(error)
                if error.kind() == io::ErrorKind::NotFound && args.server_config.is_none() =>
            {
                None
            }
            Err(error) => {
                return Err(format!(
                    "Cannot read server configuration {}: {}",
                    path.display(),
                    error.kind()
                )
                .into());
            }
        };
        // TOML errors can include the source line, which might contain a password.
        let file = text
            .as_deref()
            .map(toml::from_str::<FileConfig>)
            .transpose()
            .map_err(
                |_| "Invalid server configuration; check bind, port, username, and password fields",
            )?
            .unwrap_or_default();
        Self::resolve(
            args,
            file,
            std::env::var("FOCUS_EDITOR_SERVER_USERNAME").ok(),
            std::env::var("FOCUS_EDITOR_SERVER_PASSWORD").ok(),
        )
    }

    fn resolve(
        args: &Arguments,
        file: FileConfig,
        username: Option<String>,
        password: Option<String>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let username = username
            .or(file.username)
            .filter(|value| !value.is_empty() && !value.contains(':'))
            .ok_or(
                "Set a nonempty server username in server.toml or FOCUS_EDITOR_SERVER_USERNAME",
            )?;
        let password = password
            .or(file.password)
            .filter(|value| !value.is_empty())
            .ok_or("Set a server password in server.toml or FOCUS_EDITOR_SERVER_PASSWORD")?;
        let authorization = Sha256::digest(format!(
            "Basic {}",
            STANDARD.encode(format!("{username}:{password}"))
        ))
        .into();
        Ok(Self {
            address: SocketAddr::new(
                args.bind
                    .or(file.bind)
                    .unwrap_or(IpAddr::V4(Ipv4Addr::UNSPECIFIED)),
                args.port.or(file.port).unwrap_or(8999),
            ),
            authorization,
        })
    }

    pub fn authorized(&self, value: &str) -> bool {
        let digest: [u8; 32] = Sha256::digest(value.as_bytes()).into();
        bool::from(digest.ct_eq(&self.authorization))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn credentials_defaults_and_precedence() {
        let config = Config::resolve(
            &Arguments::default(),
            FileConfig::default(),
            Some("user".into()),
            Some("secret".into()),
        )
        .unwrap();
        assert_eq!(config.address.to_string(), "0.0.0.0:8999");
        assert!(config.authorized(&format!("Basic {}", STANDARD.encode("user:secret"))));
        assert!(!config.authorized("Basic invalid"));
        let args = Arguments {
            bind: Some("127.0.0.1".parse().unwrap()),
            port: Some(9001),
            ..Default::default()
        };
        let file = FileConfig {
            bind: Some("0.0.0.0".parse().unwrap()),
            port: Some(9000),
            username: Some("file".into()),
            password: Some("file".into()),
        };
        let config =
            Config::resolve(&args, file, Some("env".into()), Some("override".into())).unwrap();
        assert_eq!(config.address.to_string(), "127.0.0.1:9001");
        assert!(config.authorized(&format!("Basic {}", STANDARD.encode("env:override"))));
        assert!(Config::resolve(&Arguments::default(), FileConfig::default(), None, None).is_err());
    }
}
