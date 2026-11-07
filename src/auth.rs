use std::{
    collections::HashSet,
    sync::{Arc, Mutex, RwLock},
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail};
use axum::{
    Json,
    extract::{Request, State},
    http::{HeaderValue, StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};
use jsonwebtoken::{
    Algorithm, DecodingKey, Validation, decode, decode_header,
    jwk::{Jwk, JwkSet},
};
use serde::Deserialize;
use serde_json::{Value, json};

const JWKS_REFRESH_INTERVAL: Duration = Duration::from_secs(60);

pub struct AuthConfig {
    pub issuer: String,
    pub audience: String,
    pub allowed_subjects: HashSet<String>,
    pub resource: String,
}

pub struct Auth {
    config: AuthConfig,
    jwks: RwLock<Arc<JwkSet>>,
    source: Option<JwksSource>,
}

struct JwksSource {
    client: reqwest::Client,
    uri: String,
    last_fetch: Mutex<Instant>,
}

#[derive(Debug)]
enum Rejection {
    Missing,
    Invalid(String),
}

#[derive(Deserialize)]
struct Claims {
    sub: String,
}

#[derive(Deserialize)]
struct IssuerMetadata {
    issuer: String,
    jwks_uri: String,
}

impl Auth {
    /// Finds the issuer's JWKS through its OpenID or OAuth authorization server metadata.
    pub async fn discover(config: AuthConfig) -> Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()?;
        let base = config.issuer.trim_end_matches('/');
        let mut metadata = None;
        for path in [
            "/.well-known/openid-configuration",
            "/.well-known/oauth-authorization-server",
        ] {
            let response = client.get(format!("{base}{path}")).send().await;
            if let Ok(response) = response.and_then(|r| r.error_for_status()) {
                metadata = Some(response.json::<IssuerMetadata>().await?);
                break;
            }
        }
        let metadata = metadata.context("issuer publishes no authorization server metadata")?;
        if metadata.issuer != config.issuer {
            bail!(
                "issuer metadata names {} but {} is configured",
                metadata.issuer,
                config.issuer
            );
        }

        let jwks = fetch_jwks(&client, &metadata.jwks_uri).await?;
        Ok(Self {
            config,
            jwks: RwLock::new(Arc::new(jwks)),
            source: Some(JwksSource {
                client,
                uri: metadata.jwks_uri,
                last_fetch: Mutex::new(Instant::now()),
            }),
        })
    }

    #[cfg(test)]
    fn with_jwks(config: AuthConfig, jwks: JwkSet) -> Self {
        Self {
            config,
            jwks: RwLock::new(Arc::new(jwks)),
            source: None,
        }
    }

    async fn verify(&self, token: &str) -> Result<String, Rejection> {
        let header = decode_header(token).map_err(invalid)?;
        let kid = header.kid.ok_or_else(|| invalid("token has no key ID"))?;
        let jwk = self.find_key(&kid).await?;

        // The token header is attacker-controlled, so the algorithm comes from the issuer's key.
        let algorithm = jwk
            .common
            .key_algorithm
            .ok_or_else(|| invalid("signing key has no algorithm"))
            .and_then(|alg| Algorithm::try_from(alg).map_err(invalid))?;
        if matches!(
            algorithm,
            Algorithm::HS256 | Algorithm::HS384 | Algorithm::HS512
        ) {
            return Err(invalid("symmetric signing keys are not accepted"));
        }
        let key = DecodingKey::from_jwk(&jwk).map_err(invalid)?;

        let mut validation = Validation::new(algorithm);
        validation.set_issuer(&[&self.config.issuer]);
        validation.set_audience(&[&self.config.audience]);
        validation.set_required_spec_claims(&["exp", "iss", "aud", "sub"]);
        let claims = decode::<Claims>(token, &key, &validation)
            .map_err(invalid)?
            .claims;

        if !self.config.allowed_subjects.contains(&claims.sub) {
            return Err(invalid(format!("subject {} is not allowed", claims.sub)));
        }
        Ok(claims.sub)
    }

    /// Refetches the JWKS on an unknown key ID to pick up key rotation, at most once per interval.
    async fn find_key(&self, kid: &str) -> Result<Jwk, Rejection> {
        if let Some(jwk) = self.cached_key(kid) {
            return Ok(jwk);
        }
        let Some(source) = &self.source else {
            return Err(invalid("unknown signing key"));
        };
        {
            let mut last_fetch = source.last_fetch.lock().expect("lock poisoned");
            if last_fetch.elapsed() < JWKS_REFRESH_INTERVAL {
                return Err(invalid("unknown signing key"));
            }
            *last_fetch = Instant::now();
        }
        let jwks = fetch_jwks(&source.client, &source.uri)
            .await
            .map_err(|e| invalid(format!("refreshing signing keys failed: {e:#}")))?;
        *self.jwks.write().expect("lock poisoned") = Arc::new(jwks);
        self.cached_key(kid)
            .ok_or_else(|| invalid("unknown signing key"))
    }

    fn cached_key(&self, kid: &str) -> Option<Jwk> {
        self.jwks.read().expect("lock poisoned").find(kid).cloned()
    }

    fn resource_metadata_url(&self) -> String {
        format!(
            "{}/.well-known/oauth-protected-resource",
            self.config.resource
        )
    }

    fn reject(&self, rejection: Rejection) -> Response {
        let mut challenge = format!(
            "Bearer resource_metadata=\"{}\"",
            self.resource_metadata_url()
        );
        if let Rejection::Invalid(reason) = &rejection {
            tracing::warn!("rejected bearer token: {reason}");
            challenge.push_str(", error=\"invalid_token\"");
        }
        let mut response = StatusCode::UNAUTHORIZED.into_response();
        response.headers_mut().insert(
            header::WWW_AUTHENTICATE,
            HeaderValue::from_str(&challenge).expect("challenge is a valid header"),
        );
        response
    }
}

async fn fetch_jwks(client: &reqwest::Client, uri: &str) -> Result<JwkSet> {
    Ok(client
        .get(uri)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?)
}

fn invalid(reason: impl ToString) -> Rejection {
    Rejection::Invalid(reason.to_string())
}

pub async fn require_token(
    State(auth): State<Arc<Auth>>,
    request: Request,
    next: Next,
) -> Response {
    let token = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "));
    let Some(token) = token else {
        return auth.reject(Rejection::Missing);
    };
    match auth.verify(token).await {
        Ok(_) => next.run(request).await,
        Err(rejection) => auth.reject(rejection),
    }
}

/// Protected resource metadata (RFC 9728), which tells clients where to obtain tokens.
pub async fn protected_resource_metadata(State(auth): State<Arc<Auth>>) -> Json<Value> {
    Json(json!({
        "resource": auth.config.resource,
        "authorization_servers": [auth.config.issuer],
        "bearer_methods_supported": ["header"],
    }))
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use axum::{Router, body::Body, middleware, routing::get};
    use jsonwebtoken::{EncodingKey, Header, encode};
    use tower::ServiceExt;

    use super::*;

    const ISSUER: &str = "https://issuer.example/";
    const AUDIENCE: &str = "https://mcp.example";
    const SUBJECT: &str = "google-oauth2|123";

    fn auth() -> Auth {
        let jwks = serde_json::from_str(include_str!("../tests/fixtures/jwks.json")).unwrap();
        Auth::with_jwks(
            AuthConfig {
                issuer: ISSUER.into(),
                audience: AUDIENCE.into(),
                allowed_subjects: HashSet::from([SUBJECT.into()]),
                resource: AUDIENCE.into(),
            },
            jwks,
        )
    }

    fn token(claims: Value) -> String {
        let mut header = Header::new(Algorithm::RS256);
        header.kid = Some("test-key".into());
        let key =
            EncodingKey::from_rsa_pem(include_bytes!("../tests/fixtures/rsa_private.pem")).unwrap();
        encode(&header, &claims, &key).unwrap()
    }

    fn claims() -> Value {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        json!({ "iss": ISSUER, "aud": [AUDIENCE, "https://issuer.example/userinfo"], "sub": SUBJECT, "exp": now + 300 })
    }

    fn with(field: &str, value: Value) -> Value {
        let mut claims = claims();
        claims[field] = value;
        claims
    }

    #[tokio::test]
    async fn accepts_valid_token() {
        assert_eq!(auth().verify(&token(claims())).await.unwrap(), SUBJECT);
    }

    #[tokio::test]
    async fn rejects_invalid_claims() {
        let auth = auth();
        for claims in [
            with("iss", json!("https://other.example/")),
            with("aud", json!("https://other.example")),
            with("sub", json!("google-oauth2|456")),
            with("exp", json!(1)),
        ] {
            assert!(auth.verify(&token(claims)).await.is_err());
        }
    }

    #[tokio::test]
    async fn rejects_algorithm_from_token_header() {
        let mut header = Header::new(Algorithm::HS256);
        header.kid = Some("test-key".into());
        let forged = encode(&header, &claims(), &EncodingKey::from_secret(b"secret")).unwrap();
        assert!(auth().verify(&forged).await.is_err());
    }

    #[tokio::test]
    async fn challenges_requests_without_token() {
        let auth = Arc::new(auth());
        let app = Router::new()
            .route("/", get(|| async { "ok" }))
            .route_layer(middleware::from_fn_with_state(auth.clone(), require_token));

        let response = app
            .oneshot(Request::get("/").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(
            response.headers()[header::WWW_AUTHENTICATE],
            "Bearer resource_metadata=\"https://mcp.example/.well-known/oauth-protected-resource\""
        );
    }
}
