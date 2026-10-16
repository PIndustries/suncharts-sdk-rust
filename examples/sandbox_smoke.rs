//! Sandbox smoke test: OAuth2 token, a hand-written call and a generated call.
//! SUNCHARTS_CLIENT_ID=pc_… SUNCHARTS_CLIENT_SECRET=… cargo run --example sandbox_smoke
use suncharts_sdk::{generated::apis::partner_api, Client, SANDBOX_BASE_URL};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let env = |k: &str| std::env::var(k).unwrap_or_default();
    let base = std::env::var("SUNCHARTS_BASE_URL").unwrap_or(SANDBOX_BASE_URL.into());
    let client = Client::builder(base)
        .client_credentials(env("SUNCHARTS_CLIENT_ID"), env("SUNCHARTS_CLIENT_SECRET"))
        .scope("partner.read tenants.read")
        .build()?;
    let me = client.partner_me().await?; // hand-written, typed
    println!("partner: {}", me.partner.name);
    let page = partner_api::partner_list_tenants(&client.api_config().await?).await?; // generated
    let slugs: Vec<_> = page.tenants.iter().map(|t| t.slug.as_str()).collect();
    println!("tenants: {slugs:?}");
    Ok(())
}
