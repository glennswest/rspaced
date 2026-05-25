//! Load registry credentials from a Docker `config.json` / OpenShift pull
//! secret (`{"auths": {"<registry>": {"auth": base64("user:pass")}}}`).

use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result};
use base64::Engine;
use serde::Deserialize;

#[derive(Deserialize)]
struct DockerConfig {
    #[serde(default)]
    auths: HashMap<String, AuthEntry>,
}

#[derive(Deserialize)]
struct AuthEntry {
    #[serde(default)]
    auth: Option<String>,
    #[serde(default)]
    username: Option<String>,
    #[serde(default)]
    password: Option<String>,
}

/// Parse a pull secret file into `registry-host -> (username, password)`.
pub fn load_docker_config(path: &Path) -> Result<HashMap<String, (String, String)>> {
    let bytes = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    let cfg: DockerConfig =
        serde_json::from_slice(&bytes).context("parsing docker config json / pull secret")?;

    let mut out = HashMap::new();
    for (registry, entry) in cfg.auths {
        if let Some(b64) = entry.auth.as_deref() {
            if let Ok(decoded) = base64::engine::general_purpose::STANDARD.decode(b64.trim()) {
                if let Ok(s) = String::from_utf8(decoded) {
                    if let Some((u, p)) = s.split_once(':') {
                        out.insert(registry, (u.to_string(), p.to_string()));
                        continue;
                    }
                }
            }
        }
        if let (Some(u), Some(p)) = (entry.username, entry.password) {
            out.insert(registry, (u, p));
        }
    }
    Ok(out)
}
