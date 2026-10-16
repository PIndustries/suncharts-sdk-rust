use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Deserialize)]
pub struct PartnerToken {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: u64,
    pub scope: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PartnerInfo {
    pub id: String,
    pub slug: String,
    pub name: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct Branding {
    pub logo_url: Option<String>,
    pub primary_color: Option<String>,
    pub email_from_name: Option<String>,
    pub support_email: Option<String>,
    pub custom_domain: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AppInfo {
    pub id: String,
    pub client_id: String,
    pub scopes: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PartnerMe {
    pub partner: PartnerInfo,
    #[serde(default)]
    pub branding: Branding,
    pub app: AppInfo,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Tenant {
    pub id: String,
    pub name: String,
    pub slug: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct NewTenant {
    pub name: String,
    pub slug: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Page<T> {
    pub data: Vec<T>,
    #[serde(default)]
    pub next_cursor: Option<String>,
}

/// Legacy email + password sign-in (when enabled for your account). No clinic code needed:
/// the email is looked up across clinics. Prefer passkeys / email sign-in links for end users.
#[derive(Debug, Clone, Default, Serialize)]
pub struct LoginRequest {
    /// Deprecated: a clinic code is no longer needed. Leave empty (it is then not sent);
    /// multi-clinic emails get a picker via [`crate::Client::sign_in`] / [`crate::Client::select_clinic`].
    #[serde(skip_serializing_if = "String::is_empty", default)]
    pub tenant: String,
    pub email: String,
    pub password: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub totp: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_name: Option<String>,
}

impl LoginRequest {
    pub fn new(email: impl Into<String>, password: impl Into<String>) -> Self {
        Self {
            email: email.into(),
            password: password.into(),
            ..Default::default()
        }
    }
}

/// A clinic the signed-in email belongs to (no clinic codes).
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct ClinicMembership {
    pub tenant_id: String,
    pub name: String,
    #[serde(default)]
    pub role: Option<String>,
}

/// Answer of `magic/start` and `signup/start` (always the same, enumeration-safe).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MagicLinkSent {
    pub message: String,
    #[serde(default)]
    pub resend_after: u64,
    #[serde(default)]
    pub expires_in: u64,
    /// Partner email delivery only: what to deliver to the user yourself (`None` = nothing to send).
    #[serde(default)]
    pub delivery: Option<Delivery>,
}

/// Secret for partner-delivered auth email (`email_delivery = partner`).
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct Delivery {
    pub email: String,
    pub token: String,
    #[serde(default)]
    pub code: Option<String>,
    /// `signin`, `signup` or `signup_verify`.
    pub purpose: String,
    pub expires_in_mins: u64,
}

/// Per-app white-label auth settings.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct AuthSettings {
    #[serde(default)]
    pub rp_id: Option<String>,
    #[serde(default)]
    pub auth_origin: Option<String>,
    #[serde(default)]
    pub email_link_base: Option<String>,
    #[serde(default)]
    pub android_cert_sha256: Vec<String>,
    #[serde(default)]
    pub apple_app_ids: Vec<String>,
    /// `platform` or `partner`.
    #[serde(default)]
    pub email_delivery: String,
    #[serde(default)]
    pub custom_domain_verified: bool,
    #[serde(default)]
    pub platform_delivery_available: bool,
}

/// Partial update for [`AuthSettings`] (`None` = unchanged; `Some("")` / `Some(vec![])` = clear).
#[derive(Debug, Clone, Serialize, Default)]
pub struct AuthSettingsPatch {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rp_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_origin: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email_link_base: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub android_cert_sha256: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub apple_app_ids: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email_delivery: Option<String>,
}

/// WebAuthn ceremony: pass `options["publicKey"]` to the platform API, then `finish_*` with the credential JSON.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PasskeyChallenge {
    pub ceremony_id: String,
    pub options: Value,
}

/// Raw sign-in answer (tokens or a clinic-selection challenge).
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct SignInWire {
    #[serde(default)]
    pub access_token: Option<String>,
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub expires_in: Option<u64>,
    #[serde(default)]
    pub clinic_selection_required: bool,
    #[serde(default)]
    pub selection_token: Option<String>,
    #[serde(default)]
    pub clinics: Vec<ClinicMembership>,
    #[serde(default)]
    pub passkey_enrollment_suggested: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct UserTokens {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Appointment {
    #[serde(default)]
    pub start: Option<String>,
    #[serde(default)]
    pub end: Option<String>,
    /// Patient reference as returned by `/api/v1/today` (`{name, uuid, pid, dob, mrn}`).
    #[serde(default)]
    pub patient: Option<PatientRef>,
    #[serde(default)]
    pub patient_name: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub patient_uuid: Option<String>,
    #[serde(default)]
    pub provider_uuid: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub reason: Option<String>,
    /// Everything else the server returns (forward compatible).
    #[serde(flatten)]
    pub extra: serde_json::Map<String, Value>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct PatientRef {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub uuid: Option<String>,
    #[serde(default)]
    pub dob: Option<String>,
}

impl Appointment {
    /// Best display name for the patient.
    pub fn patient_display(&self) -> Option<&str> {
        self.patient
            .as_ref()
            .and_then(|p| p.name.as_deref())
            .or(self.patient_name.as_deref())
    }
    /// Patient id (FHIR uuid).
    pub fn patient_id(&self) -> Option<&str> {
        self.patient
            .as_ref()
            .and_then(|p| p.uuid.as_deref())
            .or(self.patient_uuid.as_deref())
    }
    /// Visit reason / title.
    pub fn reason_display(&self) -> Option<&str> {
        self.reason.as_deref().or(self.title.as_deref())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Today {
    pub connection_id: Option<String>,
    pub date: String,
    #[serde(default)]
    pub tz: Option<String>,
    #[serde(default)]
    pub timeline: Vec<Appointment>,
    #[serde(default)]
    pub kpis: Value,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct TodayQuery {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connection: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tz: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
}

/// FHIR R4 Bundle (resources kept as JSON).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Bundle {
    #[serde(default)]
    pub total: Option<u64>,
    #[serde(default)]
    pub link: Vec<BundleLink>,
    #[serde(default)]
    pub entry: Vec<BundleEntry>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BundleLink {
    pub relation: String,
    pub url: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BundleEntry {
    pub resource: Value,
}

impl Bundle {
    pub fn resources(&self) -> impl Iterator<Item = &Value> {
        self.entry.iter().map(|e| &e.resource)
    }
    /// Query string of the `next` link (re-issued against the proxy path), if any.
    pub fn next_query(&self) -> Option<String> {
        let l = self.link.iter().find(|l| l.relation == "next")?;
        url::Url::parse(&l.url)
            .ok()
            .and_then(|u| u.query().map(String::from))
            .or_else(|| l.url.split_once('?').map(|(_, q)| q.to_string()))
    }
}

/// Chart sections, mapped to FHIR resource types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChartSection {
    Encounters,
    Notes,
    Vitals,
    Problems,
    Allergies,
    Medications,
    Documents,
    Appointments,
}

impl ChartSection {
    pub fn resource(self) -> (&'static str, Option<&'static str>) {
        match self {
            Self::Encounters => ("Encounter", None),
            Self::Notes => ("DocumentReference", Some("category=clinical-note")),
            Self::Vitals => ("Observation", Some("category=vital-signs")),
            Self::Problems => ("Condition", None),
            Self::Allergies => ("AllergyIntolerance", None),
            Self::Medications => ("MedicationRequest", None),
            Self::Documents => ("DocumentReference", None),
            Self::Appointments => ("Appointment", None),
        }
    }
}

/// Directory resources.
#[derive(Debug, Clone, Copy)]
pub enum Directory {
    Practitioners,
    Organizations,
    Locations,
}

impl Directory {
    pub fn resource(self) -> &'static str {
        match self {
            Self::Practitioners => "Practitioner",
            Self::Organizations => "Organization",
            Self::Locations => "Location",
        }
    }
}
