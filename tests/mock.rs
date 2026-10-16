use serde_json::json;
use suncharts_sdk::{
    ChartSection, Client, ErrorCode, LoginRequest, NewTenant, SignInOutcome, TodayQuery, UserTokens,
};
use wiremock::{
    matchers::{body_partial_json, body_string_contains, header, method, path, query_param},
    Mock, MockServer, ResponseTemplate,
};

async fn partner_client(s: &MockServer) -> Client {
    // HTTP Basic pc_1:ps_1
    Mock::given(method("POST"))
        .and(path("/oauth/token"))
        .and(header("authorization", "Basic cGNfMTpwc18x"))
        .and(body_string_contains("grant_type=client_credentials"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"access_token": "pt_a", "token_type": "Bearer", "expires_in": 3600, "scope": "partner.read tenants.read tenants.write"})))
        .expect(1)
        .mount(s)
        .await;
    Client::builder(s.uri())
        .partner_credentials("pc_1", "ps_1")
        .build()
        .unwrap()
}

#[tokio::test]
async fn partner_token_cached_and_typed() {
    let s = MockServer::start().await;
    let c = partner_client(&s).await;
    Mock::given(method("GET")).and(path("/api/v1/partner/me")).and(header("authorization", "Bearer pt_a"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"partner": {"id": "p", "slug": "acme", "name": "Acme"}, "branding": {"primary_color": "#123456"}, "app": {"id": "a", "client_id": "pc_1", "scopes": ["partner.read"]}})))
        .mount(&s).await;
    Mock::given(method("POST"))
        .and(path("/api/v1/partner/tenants"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"id": "t", "name": "C1", "slug": "c1", "status": "active"})),
        )
        .mount(&s)
        .await;
    assert_eq!(
        c.partner_me()
            .await
            .unwrap()
            .branding
            .primary_color
            .as_deref(),
        Some("#123456")
    );
    let t = c
        .create_tenant(&NewTenant {
            name: "C1".into(),
            slug: "c1".into(),
        })
        .await
        .unwrap();
    assert_eq!(t.slug, "c1");
    // token fetched exactly once (expect(1) verified on drop)
}

#[tokio::test]
async fn neutral_errors_and_retry_after() {
    let s = MockServer::start().await;
    let c = partner_client(&s).await;
    Mock::given(method("GET"))
        .and(path("/api/v1/partner/tenants"))
        .respond_with(
            ResponseTemplate::new(429)
                .insert_header("retry-after", "60")
                .set_body_json(
                    json!({"error": "too_many_requests", "message": "rate limit exceeded"}),
                ),
        )
        .mount(&s)
        .await;
    let e = c.list_tenants().await.unwrap_err();
    assert_eq!(e.code(), Some(&ErrorCode::RateLimited));
    assert!(matches!(
        e,
        suncharts_sdk::Error::Api {
            retry_after: Some(60),
            ..
        }
    ));

    let u = c.session(UserTokens {
        access_token: "u1".into(),
        refresh_token: "r1".into(),
        expires_in: 900,
    });
    Mock::given(method("GET"))
        .and(path("/api/v1/connections/c/fhir/Patient"))
        .respond_with(
            ResponseTemplate::new(428)
                .set_body_json(json!({"error": "openemr_link_required", "message": "link"})),
        )
        .mount(&s)
        .await;
    assert_eq!(
        u.patients("c", None).await.unwrap_err().code(),
        Some(&ErrorCode::EhrLinkRequired)
    );
}

#[tokio::test]
async fn login_refresh_on_401_and_today() {
    let s = MockServer::start().await;
    let c = Client::builder(s.uri()).build().unwrap();
    Mock::given(method("POST"))
        .and(path("/api/v1/auth/login"))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            json!({"access_token": "old", "refresh_token": "r1", "expires_in": 900}),
        ))
        .mount(&s)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/v1/auth/refresh"))
        .and(body_partial_json(json!({"refresh_token": "r1"})))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            json!({"access_token": "new", "refresh_token": "r2", "expires_in": 900}),
        ))
        .expect(1)
        .mount(&s)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v1/today"))
        .and(header("authorization", "Bearer old"))
        .respond_with(ResponseTemplate::new(401).set_body_json(json!({"error": "unauthorized"})))
        .mount(&s)
        .await;
    Mock::given(method("GET")).and(path("/api/v1/today")).and(header("authorization", "Bearer new")).and(query_param("scope", "mine"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"connection_id": "c", "date": "2026-10-15", "timeline": [{"start": "09:00", "patient": {"name": "Test Patient", "uuid": "pu1"}, "title": "Follow-up", "room": "2"}]})))
        .mount(&s).await;
    let u = c.login(LoginRequest::new("e", "p")).await.unwrap();
    let today = u
        .today(&TodayQuery {
            scope: Some("mine".into()),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(today.timeline[0].patient_display(), Some("Test Patient"));
    assert_eq!(today.timeline[0].patient_id(), Some("pu1"));
    assert_eq!(today.timeline[0].reason_display(), Some("Follow-up"));
    assert_eq!(today.timeline[0].extra["room"], "2");
    assert_eq!(u.tokens().await.refresh_token, "r2");
}

#[tokio::test]
async fn fhir_pagination_and_chart_sections() {
    let s = MockServer::start().await;
    let c = Client::builder(s.uri()).build().unwrap();
    let u = c.session(UserTokens {
        access_token: "u".into(),
        refresh_token: "r".into(),
        expires_in: 900,
    });
    Mock::given(method("GET"))
        .and(path("/api/v1/connections/c/fhir/Observation"))
        .and(query_param("_page_token", "2"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"entry": [{"resource": {"id": "o2"}}]})),
        )
        .mount(&s)
        .await;
    Mock::given(method("GET")).and(path("/api/v1/connections/c/fhir/Observation")).and(query_param("category", "vital-signs"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"link": [{"relation": "next", "url": "https://x/fhir/Observation?patient=p1&_page_token=2"}], "entry": [{"resource": {"id": "o1"}}]})))
        .mount(&s).await;
    let first = u.chart("c", "p1", ChartSection::Vitals).await.unwrap();
    assert_eq!(first.next_query().unwrap(), "patient=p1&_page_token=2");
    let all = u
        .fhir_search_all("c", "Observation", "patient=p1&category=vital-signs", 5)
        .await
        .unwrap();
    let ids: Vec<_> = all.iter().map(|r| r["id"].as_str().unwrap()).collect();
    assert_eq!(ids, ["o1", "o2"]);
}

#[tokio::test]
async fn login_without_tenant_and_clinic_picker() {
    let s = MockServer::start().await;
    let c = Client::builder(s.uri()).build().unwrap();
    Mock::given(method("POST"))
        .and(path("/api/v1/auth/login"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "clinic_selection_required": true, "selection_token": "sel",
            "clinics": [{"tenant_id": "a", "name": "Clinic A"}, {"tenant_id": "b", "name": "Clinic B"}]})))
        .mount(&s)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/v1/auth/login/select"))
        .and(body_partial_json(
            json!({"selection_token": "sel", "tenant_id": "b"}),
        ))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(
                json!({"access_token": "a1", "refresh_token": "r1", "expires_in": 900}),
            ),
        )
        .mount(&s)
        .await;
    let body = serde_json::to_value(LoginRequest::new("e", "p")).unwrap();
    assert!(body.get("tenant").is_none(), "no clinic code on the wire");
    let e = c.login(LoginRequest::new("e", "p")).await.err().unwrap();
    assert_eq!(
        e.code(),
        Some(&ErrorCode::Other("clinic_selection_required".into()))
    );
    let SignInOutcome::ChooseClinic {
        selection_token,
        clinics,
    } = c.sign_in(LoginRequest::new("e", "p")).await.unwrap()
    else {
        panic!("expected picker")
    };
    assert_eq!(clinics.len(), 2);
    assert!(matches!(
        c.select_clinic(&selection_token, &clinics[1].tenant_id)
            .await
            .unwrap(),
        SignInOutcome::Session { .. }
    ));
}

#[tokio::test]
async fn magic_link_code_flow() {
    let s = MockServer::start().await;
    let c = Client::builder(s.uri()).build().unwrap();
    Mock::given(method("POST"))
        .and(path("/api/v1/auth/magic/start"))
        .and(body_partial_json(json!({"email": "e@x.test", "device_nonce": "n"})))
        .respond_with(ResponseTemplate::new(202).set_body_json(json!({
            "message": "If that email has an account, we sent a link.", "resend_after": 60, "expires_in": 600})))
        .mount(&s)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/v1/auth/magic/verify"))
        .and(body_partial_json(
            json!({"email": "e@x.test", "code": "123456", "device_nonce": "n"}),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token": "a", "refresh_token": "r", "expires_in": 900,
            "clinics": [{"tenant_id": "a", "name": "A"}], "passkey_enrollment_suggested": true})))
        .mount(&s)
        .await;
    let sent = c.request_sign_in_link("e@x.test", "n").await.unwrap();
    assert_eq!(sent.resend_after, 60);
    match c
        .verify_sign_in_code("e@x.test", "123456", "n")
        .await
        .unwrap()
    {
        SignInOutcome::Session {
            passkey_enrollment_suggested,
            clinics,
            ..
        } => {
            assert!(passkey_enrollment_suggested);
            assert_eq!(clinics[0].name, "A");
        }
        _ => panic!("expected session"),
    }
}

#[tokio::test]
async fn forward_passes_through_and_filters_headers() {
    let s = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/connections/c/rest/patient"))
        .and(header("authorization", "Bearer u"))
        .and(query_param("x", "1"))
        .respond_with(
            ResponseTemplate::new(201)
                .insert_header("x-vendor", "secret-brand")
                .insert_header("etag", "\"1\"")
                .set_body_json(json!({"ok": true})),
        )
        .mount(&s)
        .await;
    let c = Client::builder(s.uri()).build().unwrap();
    let r = c
        .forward(
            reqwest::Method::POST,
            "/api/v1/connections/c/rest/patient?x=1",
            &[
                ("Authorization".into(), "Bearer u".into()),
                ("Cookie".into(), "a=b".into()),
                ("Content-Type".into(), "application/json".into()),
            ],
            b"{}".to_vec(),
        )
        .await
        .unwrap();
    assert_eq!(r.status, 201);
    let hs: Vec<_> = r.safe_headers().map(|(k, _)| k.to_string()).collect();
    assert!(hs.contains(&"etag".to_string()) && !hs.contains(&"x-vendor".to_string()));
    assert!(c
        .forward(reqwest::Method::GET, "https://evil/x", &[], vec![])
        .await
        .is_err());
}

#[tokio::test]
async fn legacy_token_fallback_on_old_servers() {
    let s = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/oauth/token"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&s)
        .await;
    Mock::given(method("POST")).and(path("/api/v1/partner/token"))
        .and(body_partial_json(json!({"grant_type": "client_credentials", "client_id": "pc_1", "client_secret": "ps_1"})))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"access_token": "pt_legacy", "token_type": "Bearer", "expires_in": 3600, "scope": ""})))
        .expect(1).mount(&s).await;
    let c = Client::builder(s.uri())
        .client_credentials("pc_1", "ps_1")
        .build()
        .unwrap();
    assert_eq!(c.partner_token().await.unwrap().access_token, "pt_legacy");
}

#[tokio::test]
async fn oauth_error_maps_to_unauthorized() {
    let s = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/oauth/token"))
        .respond_with(ResponseTemplate::new(401).set_body_json(
            json!({"error": "invalid_client", "error_description": "client authentication failed"}),
        ))
        .mount(&s)
        .await;
    let c = Client::builder(s.uri())
        .client_credentials("pc_1", "bad")
        .build()
        .unwrap();
    let e = c.partner_me().await.unwrap_err();
    assert_eq!(e.code(), Some(&ErrorCode::Unauthorized));
}

#[tokio::test]
async fn api_key_and_relay_header_with_partner_delivery() {
    let s = MockServer::start().await;
    Mock::given(method("GET")).and(path("/api/v1/partner/auth-settings")).and(header("authorization", "Bearer sk_test_k"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"rp_id": "acme.example", "auth_origin": "https://app.acme.example", "android_cert_sha256": [], "apple_app_ids": [], "email_delivery": "partner", "custom_domain_verified": false, "platform_delivery_available": false})))
        .mount(&s).await;
    Mock::given(method("POST")).and(path("/api/v1/auth/magic/start"))
        .and(header("x-partner-authorization", "Bearer sk_test_k"))
        .and(header("x-end-user-ip", "203.0.113.7"))
        .respond_with(ResponseTemplate::new(202).set_body_json(json!({"status": "sent", "message": "m", "resend_after": 60, "expires_in": 600, "code_length": 6,
            "delivery": {"email": "u@acme.example", "token": "tok", "code": "123456", "purpose": "signin", "expires_in_mins": 10}})))
        .expect(1).mount(&s).await;
    Mock::given(method("POST"))
        .and(path("/api/v1/auth/login"))
        .and(header("x-partner-authorization", "Bearer sk_test_k"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(
                json!({"access_token": "u1", "refresh_token": "r1", "expires_in": 900}),
            ),
        )
        .expect(1)
        .mount(&s)
        .await;
    let c = Client::builder(s.uri())
        .api_key("sk_test_k")
        .build()
        .unwrap();
    assert_eq!(c.auth_settings().await.unwrap().email_delivery, "partner");
    let sent = c
        .for_end_user_ip("203.0.113.7")
        .request_sign_in_link("u@acme.example", "0123456789abcdef")
        .await
        .unwrap();
    let d = sent.delivery.unwrap();
    assert_eq!(
        (d.token.as_str(), d.code.as_deref()),
        ("tok", Some("123456"))
    );
    c.login(LoginRequest::new("u@acme.example", "pw"))
        .await
        .unwrap();
    // forward(): relay header added for auth paths, caller-supplied one ignored
    let raw = c
        .forward(
            suncharts_sdk::Method::POST,
            "/api/v1/auth/magic/start",
            &[
                ("X-Partner-Authorization".into(), "Bearer spoof".into()),
                ("content-type".into(), "application/json".into()),
            ],
            br#"{"email":"u@acme.example","device_nonce":"0123456789abcdef"}"#.to_vec(),
        )
        .await;
    // the mock above expects exactly one magic/start with the end-user IP; this one has none → 404 from wiremock
    assert_eq!(raw.unwrap().status, 404);
}

#[tokio::test]
async fn relay_disabled_and_no_creds_send_no_header() {
    let s = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/auth/magic/start"))
        .and(header("x-partner-authorization", "Bearer sk_test_k"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&s)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/v1/auth/magic/start"))
        .respond_with(
            ResponseTemplate::new(202).set_body_json(json!({"status": "sent", "message": "m"})),
        )
        .mount(&s)
        .await;
    let c = Client::builder(s.uri())
        .api_key("sk_test_k")
        .relay(false)
        .build()
        .unwrap();
    assert!(c
        .request_sign_in_link("u@x.example", "0123456789abcdef")
        .await
        .unwrap()
        .delivery
        .is_none());
    let c = Client::builder(s.uri()).build().unwrap();
    assert!(c
        .request_sign_in_link("u@x.example", "0123456789abcdef")
        .await
        .is_ok());
}
