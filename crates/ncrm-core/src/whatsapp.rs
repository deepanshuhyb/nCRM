//! Sending WhatsApp messages through Meta's Cloud API.
//!
//! So far this sends one thing: a sign-in code, using an *authentication
//! template* (Meta only allows one-time codes through templates of that
//! category, and they must have a "copy code" button). Studio messages,
//! the inbox and the bot build on the same client later.
//!
//! Rules this module keeps:
//! - The code is sent and forgotten. It is never logged, and an error from
//!   Meta is reduced to its status and error number before it leaves this
//!   module, so a response that echoed the request could not leak it.
//! - Every call has a timeout. Sign-in must not hang on Meta.
//!
//! Configuration is `WhatsAppConfig`, filled from the environment by the
//! API (`WHATSAPP_*`, see `.env.example`).

use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use serde_json::json;

/// Meta's Graph API. The version is part of the path; change it with
/// `WHATSAPP_API_BASE` when Meta retires this one.
pub const DEFAULT_API_BASE: &str = "https://graph.facebook.com/v21.0";

#[derive(Clone)]
pub struct WhatsAppConfig {
    /// Where the API lives. Overridden in tests to point at a stand-in.
    pub api_base: String,
    /// A System User access token with `whatsapp_business_messaging`.
    pub token: String,
    /// The id of the sending number (not the number itself).
    pub phone_number_id: String,
    /// Name of the approved authentication template for sign-in codes.
    pub otp_template: String,
    /// That template's language code, exactly as approved (`en`, `en_US`).
    pub template_lang: String,
}

// Written by hand so the token can never end up in a log line.
impl fmt::Debug for WhatsAppConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WhatsAppConfig")
            .field("api_base", &self.api_base)
            .field("token", &"<hidden>")
            .field("phone_number_id", &self.phone_number_id)
            .field("otp_template", &self.otp_template)
            .field("template_lang", &self.template_lang)
            .finish()
    }
}

impl WhatsAppConfig {
    /// Read the settings through `get` (the environment, in production).
    /// `Ok(None)` when WhatsApp isn't configured at all; an error naming
    /// what's missing when it is half configured.
    pub fn from_vars(get: impl Fn(&str) -> Option<String>) -> Result<Option<Self>, String> {
        let var = |name: &str| {
            get(name)
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
        };
        let token = var("WHATSAPP_TOKEN");
        let phone_number_id = var("WHATSAPP_PHONE_NUMBER_ID");
        let (token, phone_number_id) = match (token, phone_number_id) {
            (None, None) => return Ok(None),
            (Some(t), Some(p)) => (t, p),
            (None, Some(_)) => {
                return Err("WHATSAPP_PHONE_NUMBER_ID is set but WHATSAPP_TOKEN is not".into())
            }
            (Some(_), None) => {
                return Err("WHATSAPP_TOKEN is set but WHATSAPP_PHONE_NUMBER_ID is not".into())
            }
        };
        if !phone_number_id.chars().all(|c| c.is_ascii_digit()) {
            return Err(
                "WHATSAPP_PHONE_NUMBER_ID should be the number's id from Meta (digits only), not the phone number".into(),
            );
        }
        let api_base = var("WHATSAPP_API_BASE").unwrap_or_else(|| DEFAULT_API_BASE.to_string());
        if !(api_base.starts_with("https://") || api_base.starts_with("http://127.0.0.1")) {
            return Err("WHATSAPP_API_BASE must be an https address".into());
        }
        Ok(Some(WhatsAppConfig {
            api_base: api_base.trim_end_matches('/').to_string(),
            token,
            phone_number_id,
            otp_template: var("WHATSAPP_OTP_TEMPLATE")
                .unwrap_or_else(|| "ncrm_sign_in_code".into()),
            template_lang: var("WHATSAPP_TEMPLATE_LANG").unwrap_or_else(|| "en".into()),
        }))
    }
}

/// Why a send failed. Deliberately small: nothing from the request, and
/// from the response only the status and Meta's error number.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SendError {
    /// Couldn't reach Meta, or it didn't answer in time.
    Unreachable,
    /// Meta refused. `code` is its error number when it gave one
    /// (190: token expired, 131026: number not on WhatsApp, 132001:
    /// template missing or not approved in that language).
    Refused { status: u16, code: Option<i64> },
}

impl fmt::Display for SendError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SendError::Unreachable => write!(f, "WhatsApp could not be reached"),
            SendError::Refused {
                status,
                code: Some(code),
            } => write!(
                f,
                "WhatsApp refused the message (HTTP {status}, error {code})"
            ),
            SendError::Refused { status, code: None } => {
                write!(f, "WhatsApp refused the message (HTTP {status})")
            }
        }
    }
}

impl std::error::Error for SendError {}

/// A client for one sending number. Cheap to clone.
#[derive(Clone, Debug)]
pub struct WhatsApp {
    http: reqwest::Client,
    config: Arc<WhatsAppConfig>,
}

impl WhatsApp {
    pub fn new(config: WhatsAppConfig) -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .connect_timeout(Duration::from_secs(5))
            // Never follow a redirect with the token attached.
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .expect("a plain HTTP client can always be built");
        WhatsApp {
            http,
            config: Arc::new(config),
        }
    }

    /// Send a sign-in code to `phone` (international format, `+91…`).
    pub async fn send_sign_in_code(&self, phone: &str, code: &str) -> Result<(), SendError> {
        let body = json!({
            "messaging_product": "whatsapp",
            "to": phone.trim_start_matches('+'),
            "type": "template",
            "template": {
                "name": self.config.otp_template,
                "language": { "code": self.config.template_lang },
                // Authentication templates take the code twice: once in
                // the text, once for the "copy code" button.
                "components": [
                    { "type": "body", "parameters": [{ "type": "text", "text": code }] },
                    { "type": "button", "sub_type": "url", "index": "0",
                      "parameters": [{ "type": "text", "text": code }] }
                ]
            }
        });
        let url = format!(
            "{}/{}/messages",
            self.config.api_base, self.config.phone_number_id
        );
        let response = self
            .http
            .post(url)
            .bearer_auth(&self.config.token)
            .json(&body)
            .send()
            .await
            .map_err(|_| SendError::Unreachable)?;
        let status = response.status();
        if status.is_success() {
            return Ok(());
        }
        // Keep the error number, drop everything else.
        let code = response
            .json::<serde_json::Value>()
            .await
            .ok()
            .and_then(|v| v.get("error")?.get("code")?.as_i64());
        Err(SendError::Refused {
            status: status.as_u16(),
            code,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn vars(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        move |name| map.get(name).cloned()
    }

    #[test]
    fn unset_means_off_and_half_set_is_an_error() {
        assert!(WhatsAppConfig::from_vars(vars(&[])).unwrap().is_none());
        assert!(WhatsAppConfig::from_vars(vars(&[("WHATSAPP_TOKEN", "  ")]))
            .unwrap()
            .is_none());
        for half in [
            [("WHATSAPP_TOKEN", "t")],
            [("WHATSAPP_PHONE_NUMBER_ID", "123")],
        ] {
            let err = WhatsAppConfig::from_vars(vars(&half)).unwrap_err();
            assert!(err.contains("is not"), "{err}");
        }
    }

    #[test]
    fn settings_are_checked_and_defaults_filled() {
        let ok = WhatsAppConfig::from_vars(vars(&[
            ("WHATSAPP_TOKEN", "secret-token"),
            ("WHATSAPP_PHONE_NUMBER_ID", "1055512345"),
        ]))
        .unwrap()
        .unwrap();
        assert_eq!(ok.api_base, DEFAULT_API_BASE);
        assert_eq!(
            (ok.otp_template.as_str(), ok.template_lang.as_str()),
            ("ncrm_sign_in_code", "en")
        );
        assert!(
            !format!("{ok:?}").contains("secret-token"),
            "the token must not print"
        );

        // The phone number itself instead of its id is the usual mistake.
        let wrong = WhatsAppConfig::from_vars(vars(&[
            ("WHATSAPP_TOKEN", "t"),
            ("WHATSAPP_PHONE_NUMBER_ID", "+919599195718"),
        ]));
        assert!(wrong.unwrap_err().contains("digits only"));
        let plain = WhatsAppConfig::from_vars(vars(&[
            ("WHATSAPP_TOKEN", "t"),
            ("WHATSAPP_PHONE_NUMBER_ID", "1"),
            ("WHATSAPP_API_BASE", "http://example.com"),
        ]));
        assert!(plain.unwrap_err().contains("https"));
    }

    #[test]
    fn errors_say_little() {
        let e = SendError::Refused {
            status: 401,
            code: Some(190),
        };
        assert_eq!(
            e.to_string(),
            "WhatsApp refused the message (HTTP 401, error 190)"
        );
    }
}
