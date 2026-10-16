//! The generated layer (src/generated) works with the ergonomic client's auth, and the hand-written
//! client only calls operations documented in openapi.json.
use serde_json::json;
use suncharts_sdk::{generated::apis::partner_api, Client};
use wiremock::{
    matchers::{header, method, path},
    Mock, MockServer, ResponseTemplate,
};

#[tokio::test]
async fn generated_ops_use_client_credentials() {
    let s = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/oauth/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            json!({"access_token": "pt_g", "token_type": "Bearer", "expires_in": 3600, "scope": "tenants.read"}),
        ))
        .expect(1)
        .mount(&s)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v1/partner/tenants"))
        .and(header("authorization", "Bearer pt_g"))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            json!({"tenants": [{"id": "t", "name": "C1", "slug": "c1", "status": "active"}], "next_cursor": null}),
        ))
        .mount(&s)
        .await;
    let c = Client::builder(s.uri())
        .client_credentials("pc_1", "ps_1")
        .build()
        .unwrap();
    let page = partner_api::partner_list_tenants(&c.api_config().await.unwrap())
        .await
        .unwrap();
    assert_eq!(page.tenants[0].slug, "c1");
    // second config reuses the cached token (`expect(1)` above)
    partner_api::partner_list_tenants(&c.api_config().await.unwrap())
        .await
        .unwrap();
}

#[test]
fn hand_written_paths_are_documented() {
    let spec: serde_json::Value =
        serde_json::from_str(include_str!("../openapi.json")).expect("openapi.json");
    let paths: Vec<&String> = spec["paths"].as_object().unwrap().keys().collect();
    let src = include_str!("../src/lib.rs");
    let mut missing = vec![];
    for chunk in src.split('"').skip(1).step_by(2) {
        if let Some(p) = chunk.strip_prefix("/api/v1/") {
            let lit = format!("/api/v1/{}", p.split(['{', '?']).next().unwrap());
            // relay path prefixes (is_relayed_path), not operations
            if [
                "/api/v1/auth/",
                "/api/v1/invites/",
                "/api/v1/tenant/invites",
            ]
            .contains(&lit.as_str())
            {
                continue;
            }
            if !paths.iter().any(|d| d.starts_with(&lit)) {
                missing.push(lit);
            }
        }
    }
    assert!(missing.is_empty(), "not in openapi.json: {missing:?}");
}
