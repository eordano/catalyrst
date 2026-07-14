use thiserror::Error;

use super::{ingress_admin_token, room_service_base, LivekitError};

// RTMP is the only ingress input Cast 2.0 / OBS streaming uses; kept as a constant so the
// protojson enum name stays in one place if WHIP/URL inputs are ever added.
pub const INGRESS_INPUT_RTMP: &str = "RTMP_INPUT";

#[derive(Debug, Error)]
pub enum IngressError {
    #[error("token mint failed: {0}")]
    Token(#[from] LivekitError),
    #[error("livekit ingress request failed: {0}")]
    Request(String),
    #[error("livekit ingress returned status {0}: {1}")]
    Status(u16, String),
}

#[derive(Debug, Clone, Default)]
pub struct IngressInfo {
    pub ingress_id: String,
    pub url: Option<String>,
    pub stream_key: Option<String>,
}

pub struct IngressClient<'a> {
    pub http: &'a reqwest::Client,
    pub host: String,
    pub api_key: String,
    pub api_secret: String,
}

impl<'a> IngressClient<'a> {
    pub fn new(http: &'a reqwest::Client, host: &str, api_key: &str, api_secret: &str) -> Self {
        Self {
            http,
            host: host.to_string(),
            api_key: api_key.to_string(),
            api_secret: api_secret.to_string(),
        }
    }

    fn endpoint(&self, method: &str) -> String {
        format!(
            "{}/twirp/livekit.Ingress/{}",
            room_service_base(&self.host),
            method
        )
    }

    async fn call(
        &self,
        method: &str,
        body: serde_json::Value,
    ) -> Result<serde_json::Value, IngressError> {
        let token = ingress_admin_token(&self.api_key, &self.api_secret)?;
        let resp = self
            .http
            .post(self.endpoint(method))
            .bearer_auth(&token)
            .json(&body)
            .send()
            .await
            .map_err(|e| IngressError::Request(e.to_string()))?;
        let status = resp.status();
        if !status.is_success() {
            let txt = resp.text().await.unwrap_or_default();
            return Err(IngressError::Status(status.as_u16(), txt));
        }
        resp.json::<serde_json::Value>()
            .await
            .map_err(|e| IngressError::Request(e.to_string()))
    }

    pub async fn list_ingress(&self, room: &str) -> Result<Vec<IngressInfo>, IngressError> {
        let body = self
            .call("ListIngress", serde_json::json!({ "roomName": room }))
            .await?;
        Ok(body
            .get("items")
            .and_then(|i| i.as_array())
            .map(|arr| arr.iter().map(parse_ingress).collect())
            .unwrap_or_default())
    }

    pub async fn create_ingress(
        &self,
        room: &str,
        participant_identity: &str,
    ) -> Result<IngressInfo, IngressError> {
        let body = self
            .call(
                "CreateIngress",
                serde_json::json!({
                    "inputType": INGRESS_INPUT_RTMP,
                    "name": format!("{room}-ingress"),
                    "roomName": room,
                    "participantIdentity": participant_identity,
                }),
            )
            .await?;
        Ok(parse_ingress(&body))
    }

    // Mirrors comms-gatekeeper: one ingress per scene room. Reuse an existing one so the OBS
    // stream key stays stable across repeated PUTs rather than orphaning ingresses in LiveKit.
    pub async fn get_or_create_ingress(
        &self,
        room: &str,
        participant_identity: &str,
    ) -> Result<IngressInfo, IngressError> {
        if let Some(existing) = self.list_ingress(room).await?.into_iter().next() {
            return Ok(existing);
        }
        self.create_ingress(room, participant_identity).await
    }

    pub async fn delete_ingress(&self, ingress_id: &str) -> Result<(), IngressError> {
        match self
            .call(
                "DeleteIngress",
                serde_json::json!({ "ingressId": ingress_id }),
            )
            .await
        {
            Ok(_) => Ok(()),
            // Already gone in LiveKit is success from the caller's view; the DB row is what
            // scene-stream-access DELETE really needs to retire.
            Err(IngressError::Status(404, _)) => Ok(()),
            Err(IngressError::Status(_, txt)) if txt.contains("not_found") => Ok(()),
            Err(e) => Err(e),
        }
    }
}

fn parse_ingress(v: &serde_json::Value) -> IngressInfo {
    IngressInfo {
        ingress_id: v
            .get("ingressId")
            .and_then(|s| s.as_str())
            .unwrap_or_default()
            .to_string(),
        url: v.get("url").and_then(|s| s.as_str()).map(String::from),
        stream_key: v
            .get("streamKey")
            .and_then(|s| s.as_str())
            .map(String::from),
    }
}
