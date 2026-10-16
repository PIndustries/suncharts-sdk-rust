//! cargo run --example today  (env: API_BASE [default sandbox], PARTNER_CLIENT_ID, PARTNER_CLIENT_SECRET, USER_EMAIL, USER_PASSWORD)
use suncharts_sdk::{Client, LoginRequest, TodayQuery, SANDBOX_BASE_URL};

#[tokio::main]
async fn main() -> suncharts_sdk::Result<()> {
    let env = |k: &str| std::env::var(k).unwrap_or_default();
    let client = Client::builder(std::env::var("API_BASE").unwrap_or(SANDBOX_BASE_URL.into()))
        .client_credentials(env("PARTNER_CLIENT_ID"), env("PARTNER_CLIENT_SECRET"))
        .build()?;
    let me = client.partner_me().await?;
    println!(
        "partner: {} ({} tenants)",
        me.partner.name,
        client.list_tenants().await?.data.len()
    );
    let user = client
        .login(LoginRequest {
            device_name: Some("example".into()),
            ..LoginRequest::new(env("USER_EMAIL"), env("USER_PASSWORD"))
        })
        .await?;
    let today = user.today(&TodayQuery::default()).await?;
    for a in &today.timeline {
        println!(
            "{} {}",
            a.start.as_deref().unwrap_or("?"),
            a.patient_display().unwrap_or("?")
        );
    }
    Ok(())
}
