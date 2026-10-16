//! Official Rust client for the [SunCharts API](https://api.suncharts.app/docs)
//! (OpenAPI: <https://api.suncharts.app/openapi.json>).
//!
//! Auth contexts:
//! - [`Client`] with OAuth2 client credentials (`client_id`/`client_secret` → short-lived `pt_…` access
//!   tokens, fetched from `/oauth/token`, cached and refreshed automatically) **or** an API key (`sk_…`).
//! - [`UserSession`]: an end user's session (sign-in relay); access token refreshed on expiry/401.
//!
//! When partner credentials are configured, end-user auth calls (`/api/v1/auth/*`, `/api/v1/signup*`,
//! invites) — including [`Client::forward`] — carry `X-Partner-Authorization` automatically, so the
//! platform applies your app's white-label auth settings (passkey RP, email delivery, link origin).
//!
//! Every operation in the spec is also available, typed, under [`generated`] (regenerated from
//! `openapi.json` on every API change); [`Client::api_config`] / [`UserSession::api_config`] hand those
//! functions an authenticated configuration:
//!
//! ```no_run
//! # async fn run(client: suncharts_sdk::Client) -> Result<(), Box<dyn std::error::Error>> {
//! use suncharts_sdk::generated::apis::partner_api;
//! let me = partner_api::partner_me(&client.api_config().await?).await?;
//! # Ok(()) }
//! ```
//!
//! ```no_run
//! # async fn run() -> suncharts_sdk::Result<()> {
//! use suncharts_sdk::{Client, LoginRequest, TodayQuery, SANDBOX_BASE_URL};
//! let client = Client::builder(SANDBOX_BASE_URL)
//!     .client_credentials("pc_…", "ps_…")
//!     .build()?;
//! let me = client.partner_me().await?;
//! let user = client.login(LoginRequest::new("demo@example.test", "…")).await?;
//! let today = user.today(&TodayQuery::default()).await?;
//! println!("{} has {} appointments", me.partner.name, today.timeline.len());
//! # Ok(()) }
//! ```

mod error;
pub mod generated;
mod models;

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use reqwest::{RequestBuilder, Response, StatusCode};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;
use tokio::sync::Mutex;

pub use error::{Error, ErrorCode, Result};
pub use models::*;
pub use reqwest::Method;

/// Request headers a pass-through forwards (everything else, e.g. cookies or forwarding headers, is dropped).
pub const FORWARD_REQUEST_HEADERS: &[&str] = &[
    "authorization",
    "content-type",
    "accept",
    "accept-language",
    "if-none-match",
    "if-match",
    "prefer",
    "user-agent",
];

/// Response headers safe to expose to end users (no server/vendor headers).
pub const SAFE_RESPONSE_HEADERS: &[&str] = &[
    "content-type",
    "content-length",
    "content-disposition",
    "cache-control",
    "etag",
    "last-modified",
    "retry-after",
    "location",
    "deprecation",
    "sunset",
];

/// Raw upstream response from [`Client::forward`].
#[derive(Debug, Clone)]
pub struct RawResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl RawResponse {
    /// Only [`SAFE_RESPONSE_HEADERS`].
    pub fn safe_headers(&self) -> impl Iterator<Item = (&str, &str)> {
        self.headers
            .iter()
            .filter(|(k, _)| SAFE_RESPONSE_HEADERS.contains(&k.to_ascii_lowercase().as_str()))
            .map(|(k, v)| (k.as_str(), v.as_str()))
    }
}

const REFRESH_SKEW: Duration = Duration::from_secs(60);

#[derive(Clone)]
struct Creds {
    client_id: String,
    client_secret: String,
    scope: Option<String>,
}

/// Header that identifies the partner app on relayed end-user auth calls.
pub const PARTNER_AUTH_HEADER: &str = "x-partner-authorization";
/// Optional end-user IP on relayed calls (per-user rate limits).
pub const END_USER_IP_HEADER: &str = "x-end-user-ip";

/// Paths that receive [`PARTNER_AUTH_HEADER`] when partner credentials are set.
pub fn is_relayed_path(path: &str) -> bool {
    let p = path.split('?').next().unwrap_or(path);
    p.starts_with("/api/v1/auth/")
        || p.starts_with("/api/v1/signup")
        || p.starts_with("/api/v1/invites/")
        || p.starts_with("/api/v1/tenant/invites")
}

struct Cached {
    token: String,
    expires: Instant,
}

pub struct ClientBuilder {
    base: String,
    creds: Option<Creds>,
    api_key: Option<String>,
    relay: bool,
    http: Option<reqwest::Client>,
    user_agent: String,
}

impl ClientBuilder {
    pub fn partner_credentials(
        mut self,
        client_id: impl Into<String>,
        client_secret: impl Into<String>,
    ) -> Self {
        self.creds = Some(Creds {
            client_id: client_id.into(),
            client_secret: client_secret.into(),
            scope: None,
        });
        self
    }
    /// OAuth2 client credentials (same as [`ClientBuilder::partner_credentials`]).
    pub fn client_credentials(
        self,
        client_id: impl Into<String>,
        client_secret: impl Into<String>,
    ) -> Self {
        self.partner_credentials(client_id, client_secret)
    }
    /// Authenticate partner calls with an API key (`sk_live_…` / `sk_test_…`) instead of client credentials.
    pub fn api_key(mut self, key: impl Into<String>) -> Self {
        self.api_key = Some(key.into());
        self
    }
    /// Attach `X-Partner-Authorization` to end-user auth calls (default `true` when credentials are set).
    pub fn relay(mut self, on: bool) -> Self {
        self.relay = on;
        self
    }
    /// Request a subset of the app's scopes (space separated).
    pub fn scope(mut self, scope: impl Into<String>) -> Self {
        if let Some(c) = self.creds.as_mut() {
            c.scope = Some(scope.into());
        }
        self
    }
    pub fn http_client(mut self, http: reqwest::Client) -> Self {
        self.http = Some(http);
        self
    }
    pub fn user_agent(mut self, ua: impl Into<String>) -> Self {
        self.user_agent = ua.into();
        self
    }
    pub fn build(self) -> Result<Client> {
        let base = url::Url::parse(&self.base)?
            .as_str()
            .trim_end_matches('/')
            .to_string();
        let http = match self.http {
            Some(h) => h,
            None => reqwest::Client::builder()
                .user_agent(self.user_agent)
                .timeout(Duration::from_secs(30))
                .build()?,
        };
        Ok(Client {
            inner: Arc::new(Inner {
                base,
                http,
                creds: self.creds,
                api_key: self.api_key,
                relay: self.relay,
                partner: Mutex::new(None),
            }),
            end_user_ip: None,
        })
    }
}

struct Inner {
    base: String,
    http: reqwest::Client,
    creds: Option<Creds>,
    api_key: Option<String>,
    relay: bool,
    partner: Mutex<Option<Cached>>,
}

#[derive(Clone)]
pub struct Client {
    inner: Arc<Inner>,
    end_user_ip: Option<String>,
}

async fn decode<T: DeserializeOwned>(resp: Response) -> Result<T> {
    let status = resp.status();
    if status.is_success() {
        let bytes = resp.bytes().await?;
        let body = if bytes.is_empty() {
            &b"null"[..]
        } else {
            &bytes[..]
        };
        return Ok(serde_json::from_slice(body)?);
    }
    Err(api_error(resp).await)
}

async fn api_error(resp: Response) -> Error {
    let status = resp.status();
    let retry_after = resp
        .headers()
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse().ok());
    let w: Option<error::WireError> = resp.json().await.ok();
    let code = w
        .as_ref()
        .and_then(|w| w.error.as_deref())
        .map(ErrorCode::from_wire)
        .unwrap_or_else(|| match status {
            StatusCode::UNAUTHORIZED => ErrorCode::Unauthorized,
            StatusCode::FORBIDDEN => ErrorCode::Forbidden,
            StatusCode::NOT_FOUND => ErrorCode::NotFound,
            StatusCode::TOO_MANY_REQUESTS => ErrorCode::RateLimited,
            s if s.is_server_error() => ErrorCode::Upstream,
            _ => ErrorCode::BadRequest,
        });
    let message = w.and_then(|w| w.message).unwrap_or_default();
    Error::Api {
        status: status.as_u16(),
        code,
        message,
        retry_after,
    }
}

/// Production API base.
pub const PRODUCTION_BASE_URL: &str = "https://api.suncharts.app";
/// Sandbox API base (simulated data; self-serve credentials at `/partners/`).
pub const SANDBOX_BASE_URL: &str = "https://sandbox-api.suncharts.app";
/// Default base: production.
pub const DEFAULT_BASE_URL: &str = PRODUCTION_BASE_URL;

impl Client {
    /// Builder pointed at [`DEFAULT_BASE_URL`].
    pub fn builder_default() -> ClientBuilder {
        Self::builder(DEFAULT_BASE_URL)
    }

    pub fn builder(base_url: impl Into<String>) -> ClientBuilder {
        ClientBuilder {
            base: base_url.into(),
            creds: None,
            api_key: None,
            relay: true,
            http: None,
            user_agent: concat!("partner-sdk-rust/", env!("CARGO_PKG_VERSION")).into(),
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.inner.base, path)
    }

    fn req(&self, m: Method, path: &str) -> RequestBuilder {
        self.inner.http.request(m, self.url(path))
    }

    /// A handle that sends `X-End-User-IP: ip` on relayed end-user auth calls (cheap clone).
    pub fn for_end_user_ip(&self, ip: impl Into<String>) -> Client {
        Client {
            inner: self.inner.clone(),
            end_user_ip: Some(ip.into()),
        }
    }

    fn has_partner_auth(&self) -> bool {
        self.inner.api_key.is_some() || self.inner.creds.is_some()
    }

    /// Request for an end-user auth path, with the relay headers when partner credentials are set.
    async fn auth_req(&self, m: Method, path: &str) -> Result<RequestBuilder> {
        let mut r = self.req(m, path);
        if self.inner.relay && self.has_partner_auth() && is_relayed_path(path) {
            r = r.header(
                PARTNER_AUTH_HEADER,
                format!("Bearer {}", self.partner_bearer(false).await?),
            );
            if let Some(ip) = &self.end_user_ip {
                r = r.header(END_USER_IP_HEADER, ip.as_str());
            }
        }
        Ok(r)
    }

    /// Authenticated configuration for the generated operations in [`generated::apis`] (partner
    /// credentials as the bearer; the OAuth token is fetched or refreshed as needed). Build a new one per
    /// batch of calls rather than caching it: client-credentials tokens expire after an hour.
    pub async fn api_config(&self) -> Result<generated::apis::configuration::Configuration> {
        let mut c = generated::apis::configuration::Configuration::new();
        c.base_path = self.inner.base.clone();
        c.client = self.inner.http.clone();
        c.user_agent = Some(concat!("suncharts-sdk-rust/", env!("CARGO_PKG_VERSION")).into());
        if self.has_partner_auth() {
            c.bearer_access_token = Some(self.partner_bearer(false).await?);
        }
        Ok(c)
    }

    // ------------------------------------------------------------ partner auth

    /// Fetch a fresh OAuth2 access token via client credentials (normally automatic).
    /// Uses `POST /oauth/token` (HTTP Basic + form); falls back to the legacy
    /// `/api/v1/partner/token` on servers that predate it.
    pub async fn partner_token(&self) -> Result<PartnerToken> {
        let c = self
            .inner
            .creds
            .as_ref()
            .ok_or(Error::NoPartnerCredentials)?;
        let mut form = vec![("grant_type", "client_credentials".to_string())];
        if let Some(s) = &c.scope {
            form.push(("scope", s.clone()));
        }
        let resp = self
            .req(Method::POST, "/oauth/token")
            .basic_auth(&c.client_id, Some(&c.client_secret))
            .form(&form)
            .send()
            .await?;
        if resp.status() != StatusCode::NOT_FOUND {
            return decode(resp).await;
        }
        let mut body = serde_json::json!({
            "grant_type": "client_credentials", "client_id": c.client_id, "client_secret": c.client_secret,
        });
        if let Some(s) = &c.scope {
            body["scope"] = Value::String(s.clone());
        }
        decode(
            self.req(Method::POST, "/api/v1/partner/token")
                .json(&body)
                .send()
                .await?,
        )
        .await
    }

    async fn partner_bearer(&self, force: bool) -> Result<String> {
        if let Some(k) = &self.inner.api_key {
            return Ok(k.clone());
        }
        let mut g = self.inner.partner.lock().await;
        if !force {
            if let Some(c) = g.as_ref() {
                if Instant::now() + REFRESH_SKEW < c.expires {
                    return Ok(c.token.clone());
                }
            }
        }
        let t = self.partner_token().await?;
        *g = Some(Cached {
            token: t.access_token.clone(),
            expires: Instant::now() + Duration::from_secs(t.expires_in),
        });
        Ok(t.access_token)
    }

    async fn partner_call<B: Serialize, T: DeserializeOwned>(
        &self,
        m: Method,
        path: &str,
        body: Option<&B>,
    ) -> Result<T> {
        let mut force = false;
        loop {
            let tok = self.partner_bearer(force).await?;
            let mut r = self.req(m.clone(), path).bearer_auth(tok);
            if let Some(b) = body {
                r = r.json(b);
            }
            let resp = r.send().await?;
            if resp.status() == StatusCode::UNAUTHORIZED && !force && self.inner.api_key.is_none() {
                force = true;
                continue;
            }
            return decode(resp).await;
        }
    }

    /// Your app's white-label auth settings (scope `partner.read`).
    pub async fn auth_settings(&self) -> Result<AuthSettings> {
        self.partner_call::<(), _>(Method::GET, "/api/v1/partner/auth-settings", None)
            .await
    }
    /// Update your app's auth settings; `None` fields are left unchanged (scope `apps.manage`).
    pub async fn update_auth_settings(&self, patch: &AuthSettingsPatch) -> Result<AuthSettings> {
        self.partner_call(Method::PATCH, "/api/v1/partner/auth-settings", Some(patch))
            .await
    }

    pub async fn partner_me(&self) -> Result<PartnerMe> {
        self.partner_call::<(), _>(Method::GET, "/api/v1/partner/me", None)
            .await
    }
    pub async fn list_tenants(&self) -> Result<Page<Tenant>> {
        self.partner_call::<(), _>(Method::GET, "/api/v1/partner/tenants", None)
            .await
    }
    pub async fn create_tenant(&self, t: &NewTenant) -> Result<Tenant> {
        self.partner_call(Method::POST, "/api/v1/partner/tenants", Some(t))
            .await
    }

    // ------------------------------------------------------------ typed pass-through

    /// Forward one request to the platform as-is (generic `/api/v1` pass-through for partner backends).
    /// `path_and_query` must start with `/`. Only the listed request headers are sent; the caller decides
    /// which upstream response headers to expose (see [`RawResponse::safe_headers`]).
    pub async fn forward(
        &self,
        method: Method,
        path_and_query: &str,
        headers: &[(String, String)],
        body: Vec<u8>,
    ) -> Result<RawResponse> {
        if !path_and_query.starts_with('/') || path_and_query.contains("://") {
            return Err(Error::Url(url::ParseError::RelativeUrlWithoutBase));
        }
        let mut r = self.auth_req(method, path_and_query).await?;
        for (k, v) in headers {
            let lk = k.to_ascii_lowercase();
            if FORWARD_REQUEST_HEADERS.contains(&lk.as_str()) && lk != PARTNER_AUTH_HEADER {
                r = r.header(k.as_str(), v.as_str());
            }
        }
        if !body.is_empty() {
            r = r.body(body);
        }
        let resp = r.send().await?;
        let status = resp.status().as_u16();
        let headers = resp
            .headers()
            .iter()
            .filter_map(|(k, v)| {
                v.to_str()
                    .ok()
                    .map(|v| (k.as_str().to_string(), v.to_string()))
            })
            .collect();
        let body = resp.bytes().await?.to_vec();
        Ok(RawResponse {
            status,
            headers,
            body,
        })
    }

    // ------------------------------------------------------------ end-user auth

    /// Sign an end user in with email + password (legacy passthrough; no clinic code). Returns a
    /// session that refreshes itself. When the email belongs to several clinics this fails with
    /// `ErrorCode::Other("clinic_selection_required")`: use [`Client::sign_in`] to get the picker.
    pub async fn login(&self, req: LoginRequest) -> Result<UserSession> {
        match self.sign_in(req).await? {
            SignInOutcome::Session { session, .. } => Ok(session),
            SignInOutcome::ChooseClinic { .. } => Err(Error::Api {
                status: 200,
                code: ErrorCode::Other("clinic_selection_required".into()),
                message: "this email belongs to several clinics; choose one with select_clinic"
                    .into(),
                retry_after: None,
            }),
        }
    }

    /// Email + password sign-in that surfaces multi-clinic accounts.
    pub async fn sign_in(&self, req: LoginRequest) -> Result<SignInOutcome> {
        self.sign_in_call("/api/v1/auth/login", &serde_json::to_value(&req)?)
            .await
    }

    /// Finish a multi-clinic sign-in.
    pub async fn select_clinic(
        &self,
        selection_token: &str,
        tenant_id: &str,
    ) -> Result<SignInOutcome> {
        self.sign_in_call(
            "/api/v1/auth/login/select",
            &serde_json::json!({"selection_token": selection_token, "tenant_id": tenant_id}),
        )
        .await
    }

    /// Email a single-use sign-in link + 6-digit code, bound to `device_nonce` (random per device/session).
    /// The answer is identical whether or not the email has an account.
    pub async fn request_sign_in_link(
        &self,
        email: &str,
        device_nonce: &str,
    ) -> Result<MagicLinkSent> {
        decode(
            self.auth_req(Method::POST, "/api/v1/auth/magic/start")
                .await?
                .json(&serde_json::json!({"email": email, "device_nonce": device_nonce}))
                .send()
                .await?,
        )
        .await
    }

    /// Passwordless sign-up: email + clinic name (the clinic's internal id is generated server-side).
    pub async fn request_sign_up(
        &self,
        email: &str,
        clinic_name: &str,
        terms_version: &str,
        device_nonce: &str,
    ) -> Result<MagicLinkSent> {
        decode(
            self.auth_req(Method::POST, "/api/v1/signup/start")
                .await?
                .json(&serde_json::json!({"email": email, "clinic_name": clinic_name, "accept_terms": true,
                    "terms_version": terms_version, "device_nonce": device_nonce}))
                .send()
                .await?,
        )
        .await
    }

    /// Redeem the 6-digit code from the email (same `device_nonce` that requested it).
    pub async fn verify_sign_in_code(
        &self,
        email: &str,
        code: &str,
        device_nonce: &str,
    ) -> Result<SignInOutcome> {
        self.sign_in_call(
            "/api/v1/auth/magic/verify",
            &serde_json::json!({"email": email, "code": code, "device_nonce": device_nonce}),
        )
        .await
    }

    /// Redeem the token from the link (`https://<host>/m#t=<token>`).
    pub async fn verify_sign_in_link(
        &self,
        token: &str,
        device_nonce: &str,
    ) -> Result<SignInOutcome> {
        self.sign_in_call(
            "/api/v1/auth/magic/verify",
            &serde_json::json!({"token": token, "device_nonce": device_nonce}),
        )
        .await
    }

    /// Start a passkey sign-in (discoverable; no email).
    pub async fn passkey_sign_in_start(&self) -> Result<PasskeyChallenge> {
        decode(
            self.auth_req(Method::POST, "/api/v1/auth/passkey/login/start")
                .await?
                .send()
                .await?,
        )
        .await
    }

    /// Finish a passkey sign-in with the platform's `PublicKeyCredential` JSON.
    pub async fn passkey_sign_in_finish(
        &self,
        ceremony_id: &str,
        credential: Value,
    ) -> Result<SignInOutcome> {
        self.sign_in_call(
            "/api/v1/auth/passkey/login/finish",
            &serde_json::json!({"ceremony_id": ceremony_id, "credential": credential}),
        )
        .await
    }

    async fn sign_in_call(&self, path: &str, body: &Value) -> Result<SignInOutcome> {
        let w: SignInWire = decode(
            self.auth_req(Method::POST, path)
                .await?
                .json(body)
                .send()
                .await?,
        )
        .await?;
        if w.clinic_selection_required {
            return Ok(SignInOutcome::ChooseClinic {
                selection_token: w.selection_token.unwrap_or_default(),
                clinics: w.clinics,
            });
        }
        let t = UserTokens {
            access_token: w.access_token.ok_or(Error::NotSignedIn)?,
            refresh_token: w.refresh_token.unwrap_or_default(),
            expires_in: w.expires_in.unwrap_or(900),
        };
        Ok(SignInOutcome::Session {
            session: self.session(t),
            clinics: w.clinics,
            passkey_enrollment_suggested: w.passkey_enrollment_suggested,
        })
    }

    /// Resume a session from stored tokens.
    pub fn session(&self, t: UserTokens) -> UserSession {
        let exp = Instant::now() + Duration::from_secs(t.expires_in);
        UserSession {
            client: self.clone(),
            state: Arc::new(Mutex::new((t, exp))),
        }
    }

    /// Exchange a refresh token (rotating).
    pub async fn refresh(&self, refresh_token: &str) -> Result<UserTokens> {
        decode(
            self.auth_req(Method::POST, "/api/v1/auth/refresh")
                .await?
                .json(&serde_json::json!({ "refresh_token": refresh_token }))
                .send()
                .await?,
        )
        .await
    }
}

/// Result of a sign-in: a session, or a clinic picker for multi-clinic accounts.
pub enum SignInOutcome {
    Session {
        session: UserSession,
        clinics: Vec<ClinicMembership>,
        passkey_enrollment_suggested: bool,
    },
    ChooseClinic {
        selection_token: String,
        clinics: Vec<ClinicMembership>,
    },
}

/// An end user's session. Cheap to clone; shares one token state.
#[derive(Clone)]
pub struct UserSession {
    client: Client,
    state: Arc<Mutex<(UserTokens, Instant)>>,
}

impl UserSession {
    /// Current tokens (persist these to resume later).
    pub async fn tokens(&self) -> UserTokens {
        self.state.lock().await.0.clone()
    }

    /// Authenticated configuration for the generated end-user operations in [`generated::apis`]
    /// (the user's access token, refreshed if it is about to expire).
    pub async fn api_config(&self) -> Result<generated::apis::configuration::Configuration> {
        let mut c = self.client.api_config().await?;
        c.bearer_access_token = Some(self.bearer(false).await?);
        Ok(c)
    }

    async fn bearer(&self, force: bool) -> Result<String> {
        let mut g = self.state.lock().await;
        if force || Instant::now() + REFRESH_SKEW >= g.1 {
            let t = self.client.refresh(&g.0.refresh_token).await?;
            g.1 = Instant::now() + Duration::from_secs(t.expires_in);
            g.0 = t;
        }
        Ok(g.0.access_token.clone())
    }

    /// Raw authenticated call returning JSON. `path` starts with `/api/v1`.
    pub async fn call<T: DeserializeOwned>(
        &self,
        m: Method,
        path: &str,
        body: Option<&Value>,
    ) -> Result<T> {
        let mut force = false;
        loop {
            let tok = self.bearer(force).await?;
            let mut r = self.client.req(m.clone(), path).bearer_auth(tok);
            if let Some(b) = body {
                r = r.json(b);
            }
            let resp = r.send().await?;
            if resp.status() == StatusCode::UNAUTHORIZED && !force {
                force = true;
                continue;
            }
            return decode(resp).await;
        }
    }

    pub async fn me(&self) -> Result<Value> {
        self.call(Method::GET, "/api/v1/me", None).await
    }
    pub async fn logout(&self) -> Result<()> {
        let _: Value = self.call(Method::POST, "/api/v1/auth/logout", None).await?;
        Ok(())
    }
    pub async fn connections(&self) -> Result<Value> {
        self.call(Method::GET, "/api/v1/connections", None).await
    }
    pub async fn today(&self, q: &TodayQuery) -> Result<Today> {
        let qs = serde_urlencoded_lite(q);
        self.call(Method::GET, &format!("/api/v1/today{qs}"), None)
            .await
    }
    pub async fn inbox(&self) -> Result<Value> {
        self.call(Method::GET, "/api/v1/inbox", None).await
    }

    /// FHIR search through the EHR connection. `query` is a raw FHIR query string (no leading `?`).
    pub async fn fhir_search(
        &self,
        connection_id: &str,
        resource: &str,
        query: &str,
    ) -> Result<Bundle> {
        let q = if query.is_empty() {
            String::new()
        } else {
            format!("?{query}")
        };
        self.call(
            Method::GET,
            &format!("/api/v1/connections/{connection_id}/fhir/{resource}{q}"),
            None,
        )
        .await
    }

    /// Follows `next` links and collects every page (bounded by `max_pages`).
    pub async fn fhir_search_all(
        &self,
        connection_id: &str,
        resource: &str,
        query: &str,
        max_pages: usize,
    ) -> Result<Vec<Value>> {
        let mut out = Vec::new();
        let mut q = query.to_string();
        for _ in 0..max_pages.max(1) {
            let b = self.fhir_search(connection_id, resource, &q).await?;
            out.extend(b.resources().cloned());
            match b.next_query() {
                Some(n) => q = n,
                None => break,
            }
        }
        Ok(out)
    }

    pub async fn patients(&self, connection_id: &str, name: Option<&str>) -> Result<Bundle> {
        let q = name.map(|n| format!("name={}", enc(n))).unwrap_or_default();
        self.fhir_search(connection_id, "Patient", &q).await
    }
    pub async fn patient(&self, connection_id: &str, id: &str) -> Result<Value> {
        self.call(
            Method::GET,
            &format!(
                "/api/v1/connections/{connection_id}/fhir/Patient/{}",
                enc(id)
            ),
            None,
        )
        .await
    }
    /// One chart section for a patient.
    pub async fn chart(
        &self,
        connection_id: &str,
        patient: &str,
        section: ChartSection,
    ) -> Result<Bundle> {
        let (res, extra) = section.resource();
        let mut q = format!("patient={}", enc(patient));
        if let Some(e) = extra {
            q.push('&');
            q.push_str(e);
        }
        self.fhir_search(connection_id, res, &q).await
    }
    pub async fn directory(
        &self,
        connection_id: &str,
        d: Directory,
        name: Option<&str>,
    ) -> Result<Bundle> {
        let q = name.map(|n| format!("name={}", enc(n))).unwrap_or_default();
        self.fhir_search(connection_id, d.resource(), &q).await
    }
}

fn enc(s: &str) -> String {
    url::form_urlencoded::byte_serialize(s.as_bytes()).collect()
}

fn serde_urlencoded_lite<T: Serialize>(q: &T) -> String {
    let v = serde_json::to_value(q).unwrap_or(Value::Null);
    let mut ser = url::form_urlencoded::Serializer::new(String::new());
    let mut any = false;
    if let Value::Object(m) = v {
        for (k, v) in m {
            if let Some(s) = v.as_str() {
                ser.append_pair(&k, s);
                any = true;
            }
        }
    }
    if any {
        format!("?{}", ser.finish())
    } else {
        String::new()
    }
}
