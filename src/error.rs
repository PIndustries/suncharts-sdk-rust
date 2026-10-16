use serde::Deserialize;

/// Neutral, brand-free error codes. Safe to surface (mapped) to end users.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErrorCode {
    Unauthorized,
    Forbidden,
    NotFound,
    BadRequest,
    Conflict,
    RateLimited,
    MfaRequired,
    MfaInvalid,
    MfaEnrollmentRequired,
    EhrLinkRequired,
    EhrRelinkRequired,
    Upstream,
    Internal,
    Other(String),
}

impl ErrorCode {
    pub fn from_wire(s: &str) -> Self {
        match s {
            "unauthorized" => Self::Unauthorized,
            "forbidden" => Self::Forbidden,
            "not_found" => Self::NotFound,
            "bad_request" | "invalid_scope" | "unsupported_grant_type" | "invalid_request" => {
                Self::BadRequest
            }
            "invalid_client" => Self::Unauthorized,
            "conflict" => Self::Conflict,
            "rate_limited" | "too_many_requests" => Self::RateLimited,
            "mfa_required" => Self::MfaRequired,
            "mfa_invalid" => Self::MfaInvalid,
            "mfa_enrollment_required" => Self::MfaEnrollmentRequired,
            // legacy wire codes are normalised to neutral ones
            "ehr_link_required" | "openemr_link_required" => Self::EhrLinkRequired,
            "ehr_relink_required" | "openemr_relink_required" => Self::EhrRelinkRequired,
            "upstream_error" | "upstream" | "bad_gateway" => Self::Upstream,
            "internal" | "internal_error" => Self::Internal,
            o => Self::Other(o.to_string()),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("api error {status}: {code:?}: {message}")]
    Api {
        status: u16,
        code: ErrorCode,
        message: String,
        /// Seconds, from Retry-After on 429.
        retry_after: Option<u64>,
    },
    #[error("transport: {0}")]
    Transport(#[from] reqwest::Error),
    #[error("decode: {0}")]
    Decode(#[from] serde_json::Error),
    #[error("invalid url: {0}")]
    Url(#[from] url::ParseError),
    #[error("not signed in")]
    NotSignedIn,
    #[error("partner credentials not configured")]
    NoPartnerCredentials,
}

impl Error {
    /// Relay with platform email delivery before the partner's custom domain is verified.
    pub fn is_partner_delivery_required(&self) -> bool {
        matches!(self, Error::Api { status: 409, message, .. } if message.starts_with("partner_delivery_required"))
    }

    pub fn code(&self) -> Option<&ErrorCode> {
        match self {
            Error::Api { code, .. } => Some(code),
            _ => None,
        }
    }
}

#[derive(Deserialize)]
pub(crate) struct WireError {
    pub error: Option<String>,
    #[serde(alias = "error_description")]
    pub message: Option<String>,
}

pub type Result<T> = std::result::Result<T, Error>;
