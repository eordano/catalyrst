use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Discovery {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub control: Option<ControlEndpoint>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pulse: Option<PulseEndpoint>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub island_refresh_url: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ControlEndpoint {
    pub url: String,
    pub audience: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PulseEndpoint {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub native_endpoint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub web_transport_url: Option<String>,
    pub audience: String,
}

impl Discovery {
    pub(super) fn from_env() -> Option<Self> {
        Self::read(|key| std::env::var(key).ok())
    }

    fn read(get: impl Fn(&str) -> Option<String>) -> Option<Self> {
        let control = get("COMMS_V4_CONTROL_URL").and_then(|endpoint| {
            Some(ControlEndpoint {
                url: public_url(endpoint, &["wss"], &["ws"])?,
                audience: audience(get("COMMS_V4_CONTROL_AUDIENCE")?)?,
            })
        });
        let native_endpoint = get("COMMS_V4_PULSE_NATIVE_ENDPOINT").and_then(native_endpoint);
        let web_transport_url = get("COMMS_V4_PULSE_WEBTRANSPORT_URL")
            .and_then(|endpoint| public_url(endpoint, &["https"], &[]));
        let pulse = if native_endpoint.is_some() || web_transport_url.is_some() {
            get("COMMS_V4_PULSE_AUDIENCE")
                .and_then(audience)
                .map(|audience| PulseEndpoint {
                    native_endpoint,
                    web_transport_url,
                    audience,
                })
        } else {
            None
        };
        let island_refresh_url = get("COMMS_V4_ISLAND_REFRESH_URL")
            .and_then(|endpoint| public_url(endpoint, &["https"], &["http"]));
        (control.is_some() || pulse.is_some()).then_some(Self {
            control,
            pulse,
            island_refresh_url,
        })
    }
}

fn audience(value: String) -> Option<String> {
    (!value.is_empty()
        && value.len() <= 256
        && value.trim() == value
        && !value.chars().any(char::is_control))
    .then_some(value)
}

fn public_url(value: String, secure: &[&str], loopback: &[&str]) -> Option<String> {
    let url = url::Url::parse(&value).ok()?;
    let host = url.host_str()?;
    let is_loopback = host == "localhost"
        || host
            .trim_matches(['[', ']'])
            .parse::<std::net::IpAddr>()
            .is_ok_and(|address| address.is_loopback());
    (value.len() <= 2048
        && value.trim() == value
        && !value.chars().any(char::is_control)
        && (secure.contains(&url.scheme()) || (is_loopback && loopback.contains(&url.scheme())))
        && url.username().is_empty()
        && url.password().is_none()
        && url.query().is_none()
        && url.fragment().is_none())
    .then_some(value)
}

fn native_endpoint(value: String) -> Option<String> {
    let url = url::Url::parse(&format!("quic://{value}")).ok()?;
    (value.len() <= 512
        && !value.chars().any(char::is_whitespace)
        && url.host_str().is_some()
        && url.port().is_some_and(|port| port != 0)
        && url.username().is_empty()
        && url.password().is_none()
        && url.path().is_empty()
        && url.query().is_none()
        && url.fragment().is_none())
    .then_some(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(values: &[(&str, &str)]) -> Option<Discovery> {
        Discovery::read(|key| {
            values
                .iter()
                .find(|(name, _)| *name == key)
                .map(|(_, value)| (*value).into())
        })
    }

    #[test]
    fn absent_or_incomplete_configuration_is_not_advertised() {
        assert!(read(&[]).is_none());
        assert!(read(&[("COMMS_V4_CONTROL_URL", "wss://realm.example/ws/v4")]).is_none());
        assert!(read(&[("COMMS_V4_PULSE_AUDIENCE", "realm")]).is_none());
        assert!(read(&[
            ("COMMS_V4_CONTROL_URL", "wss://realm.example/ws/v4"),
            ("COMMS_V4_CONTROL_AUDIENCE", " "),
        ])
        .is_none());
    }

    #[test]
    fn endpoints_have_independent_audiences_and_camel_case_names() {
        let config = read(&[
            ("COMMS_V4_CONTROL_URL", "wss://realm.example/ws/v4"),
            ("COMMS_V4_CONTROL_AUDIENCE", "realm-control"),
            ("COMMS_V4_PULSE_NATIVE_ENDPOINT", "pulse.example:7777"),
            (
                "COMMS_V4_PULSE_WEBTRANSPORT_URL",
                "https://pulse.example:7743",
            ),
            ("COMMS_V4_PULSE_AUDIENCE", "realm-pulse"),
        ])
        .unwrap();
        assert_eq!(
            serde_json::to_value(config).unwrap(),
            serde_json::json!({
                "control": {"url": "wss://realm.example/ws/v4", "audience": "realm-control"},
                "pulse": {"nativeEndpoint": "pulse.example:7777", "webTransportUrl": "https://pulse.example:7743", "audience": "realm-pulse"}
            })
        );
    }

    #[test]
    fn public_endpoints_exclude_credentials_and_insecure_remote_urls() {
        for endpoint in [
            "wss://user:secret@realm.example/ws/v4",
            "wss://realm.example/ws/v4?token=secret",
            "wss://realm.example/ws/v4#secret",
            "ws://realm.example/ws/v4",
            "wss://realm.example/ws/v4\n",
        ] {
            assert!(public_url(endpoint.into(), &["wss"], &["ws"]).is_none());
        }
        assert!(public_url("ws://127.0.0.1:1234/ws/v4".into(), &["wss"], &["ws"]).is_some());
        assert!(public_url("ws://[::1]:1234/ws/v4".into(), &["wss"], &["ws"]).is_some());
    }

    #[test]
    fn native_endpoints_require_a_host_and_nonzero_port_only() {
        for endpoint in ["pulse.example:7777", "[::1]:7777"] {
            assert!(native_endpoint(endpoint.into()).is_some());
        }
        for endpoint in [
            "pulse.example",
            "pulse.example:0",
            "secret@pulse.example:7777",
            "pulse.example:7777/path",
            "pulse.example:7777?token=secret",
        ] {
            assert!(native_endpoint(endpoint.into()).is_none());
        }
    }

    #[test]
    fn island_refresh_has_an_explicit_secure_endpoint_independent_of_control() {
        let config = read(&[
            ("COMMS_V4_CONTROL_URL", "wss://control.example/ws"),
            ("COMMS_V4_CONTROL_AUDIENCE", "realm-control"),
            (
                "COMMS_V4_ISLAND_REFRESH_URL",
                "https://comms.example/comms/island-refresh",
            ),
        ])
        .unwrap();
        assert_eq!(
            serde_json::to_value(config).unwrap()["islandRefreshUrl"],
            "https://comms.example/comms/island-refresh"
        );
        let config = read(&[
            ("COMMS_V4_CONTROL_URL", "wss://control.example/ws"),
            ("COMMS_V4_CONTROL_AUDIENCE", "realm-control"),
            (
                "COMMS_V4_ISLAND_REFRESH_URL",
                "http://comms.example/island-refresh",
            ),
        ])
        .unwrap();
        assert!(config.island_refresh_url.is_none());
        assert!(read(&[(
            "COMMS_V4_ISLAND_REFRESH_URL",
            "https://comms.example/island-refresh"
        )])
        .is_none());
    }
}
