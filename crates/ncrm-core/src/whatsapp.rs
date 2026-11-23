//! Sending WhatsApp messages through Meta's Cloud API.
//!
//! Two kinds of message go through here:
//! - a sign-in code, using an *authentication template* (Meta only allows
//!   one-time codes through templates of that category, and they must
//!   have a "copy code" button), sent from the platform's own number;
//! - a studio's messages (replies, the bot, reminders), sent from that
//!   studio's number. One access token serves every number in the
//!   WhatsApp Business account, so the caller says which number to send
//!   from.
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
        self.post(&self.config.phone_number_id, &body)
            .await
            .map(|_| ())
    }

    /// Send ordinary text from a studio's number. WhatsApp only delivers
    /// this within 24 hours of the person's last message to that number;
    /// outside that window it must be a template (`send_template`).
    /// Returns WhatsApp's id for the message, which delivery reports quote.
    pub async fn send_text(
        &self,
        from_number_id: &str,
        phone: &str,
        text: &str,
    ) -> Result<String, SendError> {
        let body = json!({
            "messaging_product": "whatsapp",
            "to": phone.trim_start_matches('+'),
            "type": "text",
            "text": { "body": text, "preview_url": false }
        });
        self.post(from_number_id, &body).await
    }

    /// Send an approved template with its `{{1}}, {{2}}…` filled in, in
    /// order. This is how a studio writes first (reminders, receipts).
    pub async fn send_template(
        &self,
        from_number_id: &str,
        phone: &str,
        template: &str,
        params: &[String],
    ) -> Result<String, SendError> {
        let mut spec = json!({
            "name": template,
            "language": { "code": self.config.template_lang },
        });
        if !params.is_empty() {
            let filled: Vec<_> = params
                .iter()
                .map(|p| json!({ "type": "text", "text": p }))
                .collect();
            spec["components"] = json!([{ "type": "body", "parameters": filled }]);
        }
        let body = json!({
            "messaging_product": "whatsapp",
            "to": phone.trim_start_matches('+'),
            "type": "template",
            "template": spec
        });
        self.post(from_number_id, &body).await
    }

    /// One call to `/{number id}/messages`. On success, WhatsApp's id for
    /// the message (empty if it didn't give one). On failure only the
    /// status and Meta's error number: never the request or the reply.
    async fn post(
        &self,
        from_number_id: &str,
        body: &serde_json::Value,
    ) -> Result<String, SendError> {
        let url = format!("{}/{}/messages", self.config.api_base, from_number_id);
        let response = self
            .http
            .post(url)
            .bearer_auth(&self.config.token)
            .json(body)
            .send()
            .await
            .map_err(|_| SendError::Unreachable)?;
        let status = response.status();
        let reply = response.json::<serde_json::Value>().await.ok();
        if status.is_success() {
            return Ok(reply
                .as_ref()
                .and_then(|v| v.get("messages")?.get(0)?.get("id")?.as_str())
                .unwrap_or_default()
                .to_string());
        }
        // Keep the error number, drop everything else.
        let code = reply.and_then(|v| v.get("error")?.get("code")?.as_i64());
        Err(SendError::Refused {
            status: status.as_u16(),
            code,
        })
    }
}

// ---------------------------------------------------------------------------
// Incoming: the webhook
// ---------------------------------------------------------------------------

/// What the webhook needs to trust and answer Meta.
#[derive(Clone)]
pub struct WebhookConfig {
    /// A string we choose and give to Meta when subscribing; Meta sends
    /// it back once to prove the address is ours (`WHATSAPP_VERIFY_TOKEN`).
    pub verify_token: String,
    /// The Meta app's secret. Every delivery is signed with it
    /// (`WHATSAPP_APP_SECRET`).
    pub app_secret: String,
}

impl fmt::Debug for WebhookConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("WebhookConfig { verify_token: <hidden>, app_secret: <hidden> }")
    }
}

impl WebhookConfig {
    /// `Ok(None)` when neither is set (the webhook then answers 404);
    /// an error when only one is, or when they are too short to be real.
    pub fn from_vars(get: impl Fn(&str) -> Option<String>) -> Result<Option<Self>, String> {
        let var = |name: &str| {
            get(name)
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
        };
        match (var("WHATSAPP_VERIFY_TOKEN"), var("WHATSAPP_APP_SECRET")) {
            (None, None) => Ok(None),
            (Some(verify_token), Some(app_secret)) => {
                if verify_token.len() < 16 {
                    return Err(
                        "WHATSAPP_VERIFY_TOKEN should be at least 16 characters (openssl rand -hex 24)".into(),
                    );
                }
                if app_secret.len() < 16 {
                    return Err("WHATSAPP_APP_SECRET doesn't look like a Meta app secret".into());
                }
                Ok(Some(WebhookConfig {
                    verify_token,
                    app_secret,
                }))
            }
            (Some(_), None) => {
                Err("WHATSAPP_VERIFY_TOKEN is set but WHATSAPP_APP_SECRET is not".into())
            }
            (None, Some(_)) => {
                Err("WHATSAPP_APP_SECRET is set but WHATSAPP_VERIFY_TOKEN is not".into())
            }
        }
    }

    /// Is this the token we gave Meta? Compared in constant time.
    pub fn token_matches(&self, given: &str) -> bool {
        let mut a = HmacSha256::new_from_slice(b"ncrm-verify").expect("any key length");
        a.update(self.verify_token.as_bytes());
        let mut b = HmacSha256::new_from_slice(b"ncrm-verify").expect("any key length");
        b.update(given.as_bytes());
        a.verify_slice(&b.finalize().into_bytes()).is_ok()
    }

    /// The signature Meta would put on `body`: `sha256=<hex>`. Used by
    /// the tests and by the local tool that plays Meta's part
    /// (`ncrm-dev-whatsapp`); the server itself only ever checks.
    pub fn sign(&self, body: &[u8]) -> String {
        let mut mac =
            HmacSha256::new_from_slice(self.app_secret.as_bytes()).expect("any key length");
        mac.update(body);
        let hex: String = mac
            .finalize()
            .into_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        format!("sha256={hex}")
    }

    /// Did Meta send this body? `header` is `X-Hub-Signature-256`
    /// (`sha256=<hex>`): an HMAC of the exact bytes received, keyed with
    /// the app secret. Anyone can find a webhook's address; only Meta can
    /// sign for it.
    pub fn signature_is_valid(&self, body: &[u8], header: &str) -> bool {
        let Some(hex) = header.trim().strip_prefix("sha256=") else {
            return false;
        };
        let Some(given) = decode_hex(hex) else {
            return false;
        };
        let mut mac =
            HmacSha256::new_from_slice(self.app_secret.as_bytes()).expect("any key length");
        mac.update(body);
        // verify_slice compares in constant time.
        mac.verify_slice(&given).is_ok()
    }
}

type HmacSha256 = hmac::Hmac<sha2::Sha256>;
use hmac::{KeyInit, Mac};

fn decode_hex(hex: &str) -> Option<Vec<u8>> {
    if hex.len() != 64 || !hex.is_ascii() {
        return None;
    }
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).ok())
        .collect()
}

/// One thing in a webhook delivery that we act on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WebhookEvent {
    /// Someone wrote to one of our numbers.
    Message {
        /// The id of the number they wrote to: which studio this is for.
        to_number_id: String,
        /// The sender, with the plus.
        from: String,
        wa_message_id: String,
        text: String,
        profile_name: Option<String>,
    },
    /// News about a message we sent.
    Status {
        to_number_id: String,
        wa_message_id: String,
        /// `sent`, `delivered`, `read` or `failed`.
        status: String,
        error_code: Option<i64>,
    },
}

/// Pick the events out of a delivery. Anything unrecognised is skipped:
/// Meta adds fields and event kinds over time, and a webhook that rejects
/// what it doesn't know gets switched off.
pub fn parse_webhook(body: &serde_json::Value) -> Vec<WebhookEvent> {
    let mut events = Vec::new();
    let entries = body.get("entry").and_then(|e| e.as_array());
    for change in entries
        .into_iter()
        .flatten()
        .filter_map(|e| e.get("changes")?.as_array())
        .flatten()
    {
        let Some(value) = change.get("value") else {
            continue;
        };
        let Some(to_number_id) = value
            .get("metadata")
            .and_then(|m| m.get("phone_number_id"))
            .and_then(|v| v.as_str())
            .filter(|id| !id.is_empty() && id.chars().all(|c| c.is_ascii_digit()))
        else {
            continue;
        };
        let name_of = |wa_id: &str| {
            value
                .get("contacts")?
                .as_array()?
                .iter()
                .find(|c| c.get("wa_id").and_then(|v| v.as_str()) == Some(wa_id))?
                .get("profile")?
                .get("name")?
                .as_str()
                .map(|n| n.chars().take(80).collect::<String>())
        };
        for m in value
            .get("messages")
            .and_then(|m| m.as_array())
            .into_iter()
            .flatten()
        {
            let (Some(from), Some(id)) = (
                m.get("from").and_then(|v| v.as_str()),
                m.get("id").and_then(|v| v.as_str()),
            ) else {
                continue;
            };
            if !(8..=15).contains(&from.len())
                || !from.chars().all(|c| c.is_ascii_digit())
                || id.is_empty()
                || id.len() > 200
            {
                continue;
            }
            let kind = m.get("type").and_then(|v| v.as_str()).unwrap_or("");
            let at = |path: &[&str]| {
                path.iter()
                    .try_fold(m, |v, key| v.get(key))
                    .and_then(|v| v.as_str())
                    .map(str::to_string)
            };
            // Text as typed; for a tapped button, what the button said;
            // for anything else, a note so staff know something arrived.
            let text = match kind {
                "text" => at(&["text", "body"]),
                "button" => at(&["button", "text"]),
                "interactive" => at(&["interactive", "button_reply", "title"])
                    .or_else(|| at(&["interactive", "list_reply", "title"])),
                _ => None,
            }
            .unwrap_or_else(|| match kind {
                "image" => "(sent a photo, which can't be shown here yet)".into(),
                "audio" => "(sent a voice message, which can't be played here yet)".into(),
                "video" => "(sent a video, which can't be shown here yet)".into(),
                "document" => "(sent a document, which can't be opened here yet)".into(),
                "location" => "(sent a location)".into(),
                "sticker" => "(sent a sticker)".into(),
                _ => "(sent something that can't be shown here yet)".into(),
            });
            events.push(WebhookEvent::Message {
                to_number_id: to_number_id.to_string(),
                from: format!("+{from}"),
                wa_message_id: id.to_string(),
                text,
                profile_name: name_of(from),
            });
        }
        for s in value
            .get("statuses")
            .and_then(|s| s.as_array())
            .into_iter()
            .flatten()
        {
            let (Some(id), Some(status)) = (
                s.get("id").and_then(|v| v.as_str()),
                s.get("status").and_then(|v| v.as_str()),
            ) else {
                continue;
            };
            if id.is_empty() || id.len() > 200 {
                continue;
            }
            let error_code = s
                .get("errors")
                .and_then(|e| e.get(0))
                .and_then(|e| e.get("code"))
                .and_then(|c| c.as_i64());
            events.push(WebhookEvent::Status {
                to_number_id: to_number_id.to_string(),
                wa_message_id: id.to_string(),
                status: status.to_string(),
                error_code,
            });
        }
    }
    events
}

/// The approved templates a studio's own messages use when it writes
/// first. WhatsApp delivers ordinary text only within 24 hours of the
/// person's last message; a reminder or a broadcast reaches people who
/// haven't written, so it has to be a template Meta has approved.
///
/// The names come from the environment because templates are created in
/// Meta's console, by whoever owns the WhatsApp Business account. The
/// wording each must have is in docs/DEPLOY.md.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Templates {
    /// `WHATSAPP_FEE_REMINDER_TEMPLATE`. Four values: the contact's name,
    /// the studio, the students, and what is owed.
    pub fee_reminder: String,
    /// `WHATSAPP_BROADCAST_TEMPLATE`. Three values: the contact's name,
    /// the studio, and the message.
    pub broadcast: String,
}

impl Default for Templates {
    fn default() -> Self {
        Templates {
            fee_reminder: "ncrm_fee_reminder".into(),
            broadcast: "ncrm_announcement".into(),
        }
    }
}

impl Templates {
    pub fn from_vars(get: impl Fn(&str) -> Option<String>) -> Result<Self, String> {
        let default = Templates::default();
        let name = |var: &str, fallback: String| -> Result<String, String> {
            let Some(value) = get(var)
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
            else {
                return Ok(fallback);
            };
            // Meta's own rule for template names.
            if value.len() > 100
                || !value
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
            {
                return Err(format!(
                    "{var} must be a template name: lowercase letters, digits and underscores"
                ));
            }
            Ok(value)
        };
        Ok(Templates {
            fee_reminder: name("WHATSAPP_FEE_REMINDER_TEMPLATE", default.fee_reminder)?,
            broadcast: name("WHATSAPP_BROADCAST_TEMPLATE", default.broadcast)?,
        })
    }
}

/// Make text safe to put in a template's placeholder. WhatsApp refuses a
/// value containing a line break, a tab, or more than four spaces in a
/// row, and the whole message with it. So: every run of whitespace
/// becomes one space, and the result is cut to `max` characters.
pub fn template_value(text: &str, max: usize) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(max)
        .collect()
}

/// How studio messages leave the server.
///
/// `Test` is what runs when WhatsApp isn't configured: messages are
/// stored and marked sent, and a line is printed, but nothing reaches
/// anyone. It lets the inbox, the bot and reminders be used and tested
/// without a Meta account.
#[derive(Clone, Debug)]
pub enum MessageSender {
    Test,
    Cloud(WhatsApp),
}

impl MessageSender {
    /// True when messages really go out.
    pub fn is_live(&self) -> bool {
        matches!(self, MessageSender::Cloud(_))
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

    fn hook() -> WebhookConfig {
        WebhookConfig {
            verify_token: "a-long-verify-token-123".into(),
            app_secret: "0123456789abcdef0123456789abcdef".into(),
        }
    }

    fn sign(secret: &str, body: &[u8]) -> String {
        let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(body);
        let hex: String = mac
            .finalize()
            .into_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        format!("sha256={hex}")
    }

    #[test]
    fn only_bodies_signed_with_the_app_secret_are_trusted() {
        let hook = hook();
        let body = br#"{"entry":[]}"#;
        assert!(hook.signature_is_valid(body, &sign(&hook.app_secret, body)));
        // Signed with another secret, a changed body, or not signed at all.
        assert!(!hook.signature_is_valid(body, &sign("someone-elses-secret-0000", body)));
        assert!(!hook.signature_is_valid(br#"{"entry":[1]}"#, &sign(&hook.app_secret, body)));
        assert!(!hook.signature_is_valid(body, ""));
        assert!(!hook.signature_is_valid(body, "sha256=zz"));
        assert!(!hook.signature_is_valid(
            body,
            &sign(&hook.app_secret, body).replace("sha256=", "sha1=")
        ));
        assert!(hook.token_matches("a-long-verify-token-123"));
        assert!(!hook.token_matches("a-long-verify-token-124"));
        assert!(!format!("{hook:?}").contains("0123456789abcdef"));
    }

    #[test]
    fn webhook_settings_come_as_a_pair() {
        let with = |pairs: &'static [(&'static str, &'static str)]| {
            WebhookConfig::from_vars(move |name| {
                pairs
                    .iter()
                    .find(|(k, _)| *k == name)
                    .map(|(_, v)| v.to_string())
            })
        };
        assert!(with(&[]).unwrap().is_none());
        assert!(with(&[("WHATSAPP_VERIFY_TOKEN", "a-long-verify-token-123")]).is_err());
        assert!(with(&[
            ("WHATSAPP_VERIFY_TOKEN", "short"),
            ("WHATSAPP_APP_SECRET", "0123456789abcdef0123456789abcdef")
        ])
        .is_err());
        assert!(with(&[
            ("WHATSAPP_VERIFY_TOKEN", "a-long-verify-token-123"),
            ("WHATSAPP_APP_SECRET", "0123456789abcdef0123456789abcdef")
        ])
        .unwrap()
        .is_some());
    }

    #[test]
    fn a_delivery_is_read_into_messages_and_reports() {
        let body = serde_json::json!({
            "object": "whatsapp_business_account",
            "entry": [{ "id": "1", "changes": [{ "field": "messages", "value": {
                "metadata": { "display_phone_number": "919811000000", "phone_number_id": "555" },
                "contacts": [{ "profile": { "name": "Meena" }, "wa_id": "919811100001" }],
                "messages": [
                    { "from": "919811100001", "id": "wamid.A", "type": "text", "text": { "body": "Hi" } },
                    { "from": "919811100001", "id": "wamid.B", "type": "image", "image": { "id": "x" } },
                    { "from": "919811100001", "id": "wamid.C", "type": "interactive",
                      "interactive": { "type": "button_reply", "button_reply": { "id": "2", "title": "Fees due" } } },
                    { "from": "not-a-number", "id": "wamid.D", "type": "text", "text": { "body": "x" } },
                    { "id": "wamid.E", "type": "text" }
                ],
                "statuses": [
                    { "id": "wamid.out", "status": "delivered" },
                    { "id": "wamid.bad", "status": "failed", "errors": [{ "code": 131026 }] }
                ]
            }}]}]
        });
        let events = parse_webhook(&body);
        assert_eq!(events.len(), 5);
        assert_eq!(
            events[0],
            WebhookEvent::Message {
                to_number_id: "555".into(),
                from: "+919811100001".into(),
                wa_message_id: "wamid.A".into(),
                text: "Hi".into(),
                profile_name: Some("Meena".into()),
            }
        );
        assert!(matches!(&events[1], WebhookEvent::Message { text, .. } if text.contains("photo")));
        assert!(matches!(&events[2], WebhookEvent::Message { text, .. } if text == "Fees due"));
        assert_eq!(
            events[4],
            WebhookEvent::Status {
                to_number_id: "555".into(),
                wa_message_id: "wamid.bad".into(),
                status: "failed".into(),
                error_code: Some(131026),
            }
        );
        // Junk is ignored, not an error.
        assert!(parse_webhook(&serde_json::json!({ "entry": "nope" })).is_empty());
        assert!(parse_webhook(&serde_json::json!([1, 2])).is_empty());
        assert!(parse_webhook(&serde_json::json!({ "entry": [{ "changes": [{ "value": {
            "metadata": { "phone_number_id": "abc" },
            "messages": [{ "from": "919811100001", "id": "w", "type": "text", "text": { "body": "x" } }]
        }}]}]}))
        .is_empty());
    }

    #[test]
    fn template_values_have_no_line_breaks_or_runs_of_spaces() {
        assert_eq!(
            template_value("  Class is off\ttomorrow.\n\nSee     you  Monday ", 100),
            "Class is off tomorrow. See you Monday"
        );
        assert_eq!(template_value("abcdef", 3), "abc");
        assert_eq!(template_value(" \n ", 10), "");
    }

    #[test]
    fn template_names_default_and_are_checked() {
        assert_eq!(
            Templates::from_vars(|_| None).unwrap(),
            Templates::default()
        );
        let named = Templates::from_vars(|k| {
            (k == "WHATSAPP_BROADCAST_TEMPLATE").then(|| "studio_news_2".to_string())
        })
        .unwrap();
        assert_eq!(named.broadcast, "studio_news_2");
        assert_eq!(named.fee_reminder, "ncrm_fee_reminder");
        assert!(Templates::from_vars(|_| Some("Fee Reminder".into())).is_err());
    }
}
