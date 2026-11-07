use std::{env, net::SocketAddr};

use anyhow::{Context, Result, bail};
use axum::http::Uri;

pub struct Config {
    pub listen_addr: SocketAddr,
    pub public_url: Uri,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let listen_addr = env::var("LISTEN_ADDR")
            .unwrap_or_else(|_| "0.0.0.0:8080".into())
            .parse()
            .context("LISTEN_ADDR must be a socket address")?;
        let public_url: Uri = required("PUBLIC_URL")?
            .parse()
            .context("PUBLIC_URL must be a URL")?;
        if public_url.authority().is_none() {
            bail!("PUBLIC_URL must include a host");
        }
        Ok(Self {
            listen_addr,
            public_url,
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
