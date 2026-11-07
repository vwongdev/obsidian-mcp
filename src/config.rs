use std::{collections::HashSet, env, net::SocketAddr, path::PathBuf};

use anyhow::{Context, Result, bail};
use axum::http::Uri;

use crate::auth::AuthConfig;

pub struct Config {
    pub listen_addr: SocketAddr,
    pub public_url: Uri,
    pub vault_path: PathBuf,
    pub write_folder: String,
    pub auth: AuthConfig,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let listen_addr = env::var("LISTEN_ADDR")
            .unwrap_or_else(|_| "0.0.0.0:8080".into())
            .parse()
            .context("LISTEN_ADDR must be a socket address")?;
        let resource = required("PUBLIC_URL")?.trim_end_matches('/').to_owned();
        let public_url: Uri = resource.parse().context("PUBLIC_URL must be a URL")?;
        if public_url.authority().is_none() {
            bail!("PUBLIC_URL must include a host");
        }
        let allowed_subjects: HashSet<String> = required("ALLOWED_SUBJECTS")?
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(String::from)
            .collect();
        if allowed_subjects.is_empty() {
            bail!("ALLOWED_SUBJECTS must list at least one subject");
        }
        Ok(Self {
            listen_addr,
            public_url,
            vault_path: env::var("VAULT_PATH")
                .unwrap_or_else(|_| "/vault".into())
                .into(),
            write_folder: env::var("WRITE_FOLDER").unwrap_or_else(|_| "AI".into()),
            auth: AuthConfig {
                issuer: required("ISSUER_URL")?,
                audience: required("AUDIENCE")?,
                allowed_subjects,
                resource,
            },
        })
    }

    /// Host header values accepted by the MCP transport, which rejects others to block DNS rebinding.
    pub fn allowed_hosts(&self) -> Vec<String> {
        let authority = self.public_url.authority().expect("validated in from_env");
        let mut hosts = vec![authority.host().to_owned()];
        if authority.port().is_some() {
            hosts.push(authority.as_str().to_owned());
        }
        hosts
    }
}

fn required(name: &str) -> Result<String> {
    env::var(name).with_context(|| format!("{name} must be set"))
}
