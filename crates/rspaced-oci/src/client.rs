//! Blocking OCI Distribution v2 pull client.
//!
//! Ported from fastregistry `internal/sync/{registry,quay}.go`. Adds the
//! `WWW-Authenticate: Bearer` challenge → token-exchange flow (the Go source
//! assumed a pre-supplied token), which anonymous quay.io pulls require.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::Mutex;
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use reqwest::blocking::{Client as HttpClient, Response};
use reqwest::StatusCode;

use crate::digest::{Digest, Verifier};
use crate::types::{Index, Manifest, MEDIA_TYPES_ACCEPT};

/// A parsed image reference: `registry/repository:tag` or `…@sha256:…`.
#[derive(Clone, Debug)]
pub struct Reference {
    /// Registry host (and optional port), e.g. `quay.io`.
    pub registry: String,
    /// Repository path, e.g. `openshift-release-dev/ocp-release`.
    pub repository: String,
    /// Tag or `sha256:…` digest.
    pub reference: String,
}

impl Reference {
    /// Parse `registry/repository[:tag|@digest]` (tag defaults to `latest`).
    pub fn parse(s: &str) -> Result<Self> {
        let (name, reference) = if let Some((n, d)) = s.split_once('@') {
            (n.to_string(), d.to_string())
        } else {
            let slash = s.rfind('/').map(|i| i + 1).unwrap_or(0);
            match s[slash..].rfind(':') {
                Some(colon) => (
                    s[..slash + colon].to_string(),
                    s[slash + colon + 1..].to_string(),
                ),
                None => (s.to_string(), "latest".to_string()),
            }
        };
        let (registry, repository) = name
            .split_once('/')
            .ok_or_else(|| anyhow!("reference {s:?} has no registry host"))?;
        Ok(Self {
            registry: registry.to_string(),
            repository: repository.to_string(),
            reference,
        })
    }

    fn manifest_url(&self) -> String {
        format!(
            "https://{}/v2/{}/manifests/{}",
            self.registry, self.repository, self.reference
        )
    }

    fn blob_url(&self, digest: &Digest) -> String {
        format!(
            "https://{}/v2/{}/blobs/{}",
            self.registry, self.repository, digest
        )
    }

    fn pull_scope(&self) -> String {
        format!("repository:{}:pull", self.repository)
    }
}

/// A fetched manifest body plus its content type and computed digest.
pub struct RawManifest {
    /// Raw JSON bytes (the digest is computed over these exact bytes).
    pub body: Vec<u8>,
    /// `Content-Type` returned by the registry.
    pub content_type: String,
    /// sha256 of `body`.
    pub digest: Digest,
}

/// Either a single-platform manifest or a multi-arch index.
pub enum ManifestOrIndex {
    /// A concrete image manifest (boxed — it is much larger than `Index`).
    Manifest(Box<Manifest>),
    /// A multi-arch image index / manifest list.
    Index(Index),
}

/// A blocking pull client. One instance can pull from any registry; bearer
/// tokens are cached per `service+scope`.
pub struct Client {
    http: HttpClient,
    /// Optional `(username, password)` for registries that need basic creds
    /// in the token exchange.
    basic: Option<(String, String)>,
    tokens: Mutex<HashMap<String, String>>,
}

impl Client {
    /// Anonymous client.
    pub fn new() -> Result<Self> {
        Self::build(None)
    }

    /// Client that supplies basic credentials during token exchange.
    pub fn with_basic(username: impl Into<String>, password: impl Into<String>) -> Result<Self> {
        Self::build(Some((username.into(), password.into())))
    }

    fn build(basic: Option<(String, String)>) -> Result<Self> {
        let http = HttpClient::builder()
            .timeout(Duration::from_secs(600))
            .user_agent("rspaced-oci")
            .build()
            .context("building HTTP client")?;
        Ok(Self {
            http,
            basic,
            tokens: Mutex::new(HashMap::new()),
        })
    }

    /// Fetch the raw manifest bytes for a reference (no parsing).
    pub fn fetch_manifest(&self, r: &Reference) -> Result<RawManifest> {
        let resp =
            self.authorized_get(&r.manifest_url(), &r.pull_scope(), Some(MEDIA_TYPES_ACCEPT))?;
        let status = resp.status();
        let content_type = header(&resp, "content-type");
        if !status.is_success() {
            bail!("manifest fetch {} returned {}", r.manifest_url(), status);
        }
        let body = resp.bytes().context("reading manifest body")?.to_vec();
        let digest = Digest::from_bytes(&body);
        Ok(RawManifest {
            body,
            content_type,
            digest,
        })
    }

    /// Fetch and parse a manifest, distinguishing a single image from an index.
    pub fn get_manifest(&self, r: &Reference) -> Result<ManifestOrIndex> {
        let raw = self.fetch_manifest(r)?;
        // An index has a `manifests` array; a manifest has `layers`. Probe the
        // JSON rather than trusting Content-Type, which mirrors are loose about.
        let v: serde_json::Value =
            serde_json::from_slice(&raw.body).context("parsing manifest JSON")?;
        if v.get("manifests").is_some() {
            Ok(ManifestOrIndex::Index(serde_json::from_slice(&raw.body)?))
        } else {
            Ok(ManifestOrIndex::Manifest(Box::new(serde_json::from_slice(
                &raw.body,
            )?)))
        }
    }

    /// Resolve a reference to a concrete image manifest for `arch`/`os`,
    /// following an image index if necessary. Returns the manifest and the
    /// digest it was fetched by.
    pub fn resolve_image(&self, r: &Reference, arch: &str, os: &str) -> Result<(Manifest, Digest)> {
        match self.get_manifest(r)? {
            ManifestOrIndex::Manifest(m) => {
                let raw = self.fetch_manifest(r)?;
                Ok((*m, raw.digest))
            }
            ManifestOrIndex::Index(idx) => {
                let desc = idx
                    .select(arch, os)
                    .ok_or_else(|| anyhow!("index has no {arch}/{os} manifest"))?;
                let by_digest = Reference {
                    registry: r.registry.clone(),
                    repository: r.repository.clone(),
                    reference: desc.digest.to_string(),
                };
                match self.get_manifest(&by_digest)? {
                    ManifestOrIndex::Manifest(m) => Ok((*m, desc.digest.clone())),
                    ManifestOrIndex::Index(_) => {
                        bail!("nested image index for {arch}/{os} is not supported")
                    }
                }
            }
        }
    }

    /// Stream a blob to `out`, verifying it matches `digest`.
    pub fn pull_blob(&self, r: &Reference, digest: &Digest, out: &mut impl Write) -> Result<()> {
        let mut resp = self.authorized_get(&r.blob_url(digest), &r.pull_scope(), None)?;
        if !resp.status().is_success() {
            bail!(
                "blob fetch {} returned {}",
                r.blob_url(digest),
                resp.status()
            );
        }
        let mut verifier = Verifier::new(digest.clone())?;
        let mut buf = [0u8; 64 * 1024];
        loop {
            let n = resp.read(&mut buf).context("reading blob")?;
            if n == 0 {
                break;
            }
            verifier.update(&buf[..n]);
            out.write_all(&buf[..n]).context("writing blob")?;
        }
        if !verifier.verified() {
            bail!("blob {} failed digest verification", digest.short_hex());
        }
        Ok(())
    }

    /// GET with bearer-token handling: try with any cached token for `scope`,
    /// and on a 401 carrying a `Bearer` challenge, exchange for a token and
    /// retry once.
    fn authorized_get(&self, url: &str, scope: &str, accept: Option<&str>) -> Result<Response> {
        let resp = self.send(url, accept, self.cached_token(scope))?;
        if resp.status() != StatusCode::UNAUTHORIZED {
            return Ok(resp);
        }
        let Some(challenge) = resp.headers().get("www-authenticate") else {
            return Ok(resp);
        };
        let challenge = challenge.to_str().unwrap_or("").to_string();
        let token = self
            .exchange_token(&challenge, scope)
            .context("bearer token exchange")?;
        self.tokens
            .lock()
            .unwrap()
            .insert(scope.to_string(), token.clone());
        self.send(url, accept, Some(token))
    }

    fn send(&self, url: &str, accept: Option<&str>, token: Option<String>) -> Result<Response> {
        let mut req = self.http.get(url);
        if let Some(a) = accept {
            req = req.header("Accept", a);
        }
        if let Some(t) = token {
            req = req.bearer_auth(t);
        }
        req.send().with_context(|| format!("GET {url}"))
    }

    fn cached_token(&self, scope: &str) -> Option<String> {
        self.tokens.lock().unwrap().get(scope).cloned()
    }

    /// Parse a `Bearer realm=…,service=…,scope=…` challenge and fetch a token.
    fn exchange_token(&self, challenge: &str, fallback_scope: &str) -> Result<String> {
        let params = parse_bearer_challenge(challenge)
            .ok_or_else(|| anyhow!("unsupported auth challenge: {challenge}"))?;
        let realm = params
            .get("realm")
            .ok_or_else(|| anyhow!("auth challenge missing realm"))?;
        let scope = params
            .get("scope")
            .map(String::as_str)
            .unwrap_or(fallback_scope);

        let mut req = self.http.get(realm);
        if let Some(service) = params.get("service") {
            req = req.query(&[("service", service.as_str())]);
        }
        req = req.query(&[("scope", scope)]);
        if let Some((u, p)) = &self.basic {
            req = req.basic_auth(u, Some(p));
        }

        let resp = req
            .send()
            .with_context(|| format!("GET token from {realm}"))?;
        if !resp.status().is_success() {
            bail!("token endpoint {realm} returned {}", resp.status());
        }
        let body: serde_json::Value = resp.json().context("parsing token response")?;
        body.get("token")
            .or_else(|| body.get("access_token"))
            .and_then(|t| t.as_str())
            .map(String::from)
            .ok_or_else(|| anyhow!("token response had no token/access_token field"))
    }
}

fn header(resp: &Response, name: &str) -> String {
    resp.headers()
        .get(name)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string()
}

/// Parse the parameters of a `Bearer key="value",key2="value2"` challenge.
fn parse_bearer_challenge(header: &str) -> Option<HashMap<String, String>> {
    let rest = header
        .strip_prefix("Bearer ")
        .or_else(|| header.strip_prefix("bearer "))?;
    let mut out = HashMap::new();
    for part in rest.split(',') {
        if let Some((k, v)) = part.split_once('=') {
            let v = v.trim().trim_matches('"');
            out.insert(k.trim().to_string(), v.to_string());
        }
    }
    (!out.is_empty()).then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_tag_ref() {
        let r =
            Reference::parse("quay.io/openshift-release-dev/ocp-release:4.18.30-x86_64").unwrap();
        assert_eq!(r.registry, "quay.io");
        assert_eq!(r.repository, "openshift-release-dev/ocp-release");
        assert_eq!(r.reference, "4.18.30-x86_64");
        assert_eq!(
            r.pull_scope(),
            "repository:openshift-release-dev/ocp-release:pull"
        );
    }

    #[test]
    fn parse_digest_ref() {
        let d = "sha256:".to_string() + &"a".repeat(64);
        let r = Reference::parse(&format!("quay.io/foo/bar@{d}")).unwrap();
        assert_eq!(r.repository, "foo/bar");
        assert_eq!(r.reference, d);
    }

    #[test]
    fn parse_defaults_latest() {
        let r = Reference::parse("registry.gt.lo:5000/team/app").unwrap();
        assert_eq!(r.registry, "registry.gt.lo:5000");
        assert_eq!(r.repository, "team/app");
        assert_eq!(r.reference, "latest");
    }

    #[test]
    fn challenge_parsing() {
        let h = r#"Bearer realm="https://quay.io/v2/auth",service="quay.io",scope="repository:openshift-release-dev/ocp-release:pull""#;
        let p = parse_bearer_challenge(h).unwrap();
        assert_eq!(p["realm"], "https://quay.io/v2/auth");
        assert_eq!(p["service"], "quay.io");
        assert_eq!(
            p["scope"],
            "repository:openshift-release-dev/ocp-release:pull"
        );
    }
}
