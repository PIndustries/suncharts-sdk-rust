//! Minimal: OAuth2 client credentials → call the API.
//! cargo run --example token  (env: CLIENT_ID, CLIENT_SECRET, API_BASE [default sandbox])
use suncharts_sdk::{Client, SANDBOX_BASE_URL};

#[tokio::main]
async fn main() -> suncharts_sdk::Result<()> {
    let env = |k: &str| std::env::var(k).unwrap_or_default();
    let client = Client::builder(std::env::var("API_BASE").unwrap_or(SANDBOX_BASE_URL.into()))
        .client_credentials(env("CLIENT_ID"), env("CLIENT_SECRET"))
        .build()?;
    let t = client.partner_token().await?;
    println!(
        "token ok: type={} expires_in={}s scope={:?}",
        t.token_type, t.expires_in, t.scope
    );
    let me = client.partner_me().await?;
    println!("partner: {} ({})", me.partner.name, me.partner.slug);
    Ok(())
}
