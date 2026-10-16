# suncharts-sdk (Rust)

Official async Rust client for the **SunCharts API**.

- API reference & guides: <https://api.suncharts.app/docs>
- OpenAPI 3.1: <https://api.suncharts.app/openapi.json>
- Sandbox (simulated data, self-serve credentials): <https://sandbox-api.suncharts.app/partners/>

## Install

```toml
[dependencies]
suncharts-sdk = { git = "https://github.com/PIndustries/suncharts-sdk-rust", tag = "26.10.16.1" }
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

## Quickstart

```rust
use suncharts_sdk::{Client, SANDBOX_BASE_URL};

#[tokio::main]
async fn main() -> suncharts_sdk::Result<()> {
    let client = Client::builder(SANDBOX_BASE_URL)
        .client_credentials(std::env::var("CLIENT_ID").unwrap(), std::env::var("CLIENT_SECRET").unwrap())
        .build()?;
    let me = client.partner_me().await?; // token fetched, cached and refreshed automatically
    println!("{}", me.partner.name);
    Ok(())
}
```

`cargo run --example token` (env `CLIENT_ID`, `CLIENT_SECRET`) does exactly this against the sandbox.

## Authentication

| | |
|---|---|
| OAuth2 client credentials | `.client_credentials(id, secret)` — `POST /oauth/token` (HTTP Basic), 1 h `pt_…` tokens cached and refreshed (also on 401). `.scope("partner.read tenants.read")` narrows the token. |
| API key | `.api_key("sk_live_…")` — sent as a bearer; no token exchange. |

Never ship credentials to browsers or mobile apps; use the SDK from your backend.

## White-label auth relay

When partner credentials are set, end-user auth calls (`/api/v1/auth/*`, `/api/v1/signup*`, invites) — including
`Client::forward` — automatically carry `X-Partner-Authorization`, so the platform applies your app's auth settings
(passkey RP ID/origins, link origin, email delivery). Use `client.for_end_user_ip(ip)` to pass the user's IP for
rate limiting, and `.relay(false)` to opt out.

```rust
use suncharts_sdk::AuthSettingsPatch;
client.update_auth_settings(&AuthSettingsPatch {
    auth_origin: Some("https://app.example.com".into()),
    rp_id: Some("example.com".into()),
    email_delivery: Some("partner".into()),
    ..Default::default()
}).await?;
let sent = client.for_end_user_ip("203.0.113.7").request_sign_in_link("user@example.com", device_nonce).await?;
if let Some(d) = sent.delivery { /* email d.token / d.code to the user yourself */ }
```

## Generated operations

`suncharts_sdk::generated` contains every operation and model in the public OpenAPI spec, regenerated whenever
the API changes, so nothing is missing even before it gets a hand-written helper:

```rust
use suncharts_sdk::generated::apis::{partner_api, appointments_api};
let page = partner_api::partner_list_tenants(&client.api_config().await?).await?;
let session = client.session(tokens);                     // end-user session
let today = appointments_api::today(&session.api_config().await?, None, None, None, None).await?;
```

`scripts/generate.sh` regenerates `src/generated` from `openapi.json` (Java 11+); don't edit it by hand.
`examples/sandbox_smoke.rs` runs a token + read calls against the sandbox.

## Errors

`Error::Api { status, code, message, retry_after }` with neutral `ErrorCode`s (`Unauthorized`, `RateLimited`,
`EhrLinkRequired`, …). `Error::is_partner_delivery_required()` flags the 409 for platform delivery before your
custom domain is verified.

## Versioning

SemVer. The API is versioned by path (`/api/v1`, additive changes only). See the
[changelog](CHANGELOG.md).

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT) at your option.

## Versioning

Releases use the P.Industries format `YY.MM.DD.<build-of-day>` (UTC date), e.g. `26.10.16.1`; the git tag is exactly
that (no `v`). Cargo needs 3-part SemVer, so the crate version is `YY.(MM*100+DD).BUILD`: `26.10.16.1` → `26.1016.1` (Jan 5 → `27.105.1`; ordering is preserved). Bump with `scripts/version.sh set $(scripts/version.sh next)`; merging a `VERSION` change to
`main` tags the release automatically (`.github/workflows/release.yml`).
