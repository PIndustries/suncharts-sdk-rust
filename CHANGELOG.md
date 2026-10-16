# Changelog

## 0.4.0 — 2026-10-16
- New `generated` module: every operation and model in the SunCharts OpenAPI spec (`openapi.json`, API 1.1.0),
  generated with openapi-generator 7.26.0 (`scripts/generate.sh`; CI fails if it is stale). Spec changes arrive
  as automated pull requests.
- `Client::api_config()` / `UserSession::api_config()`: authenticated configuration for the generated functions
  (OAuth2 token cached and refreshed by the client).
- New dependency `serde_with` (used by generated models); `reqwest` gains the `multipart` feature.
- No changes to the existing hand-written API.

## 0.3.0 — 2026-10-16
- First public release (`PIndustries/suncharts-sdk-rust`).
- OAuth2 client credentials via `POST /oauth/token` (HTTP Basic, form); falls back to the legacy token endpoint.
- API keys: `ClientBuilder::api_key`.
- White-label auth relay: `X-Partner-Authorization` attached automatically on end-user auth calls and `forward`;
  `Client::for_end_user_ip`; `ClientBuilder::relay(false)` to opt out.
- `auth_settings()` / `update_auth_settings()`; `MagicLinkSent::delivery` (partner email delivery).
- `SANDBOX_BASE_URL` (`https://sandbox-api.suncharts.app`), `PRODUCTION_BASE_URL`.
- OAuth2 error codes (`invalid_client`, …) mapped to neutral `ErrorCode`s; `Error::is_partner_delivery_required`.

## 0.2.1
- Sign in without a clinic code; passwordless (magic link, passkeys).
