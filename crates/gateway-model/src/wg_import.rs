//! Parse wg-quick text or base64(wg-quick). No I/O; keys stay in the returned struct.

use crate::ValidateError;
use base64::Engine;
use serde::Serialize;

/// Fields extracted from a pasted WireGuard client config.
#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub struct WgImport {
    pub address: Option<String>,
    pub listen_port: Option<u16>,
    pub peer_endpoint: Option<String>,
    pub peer_allowed_ips: Option<String>,
    pub private_key: Option<String>,
    pub peer_public_key: Option<String>,
    pub preshared_key: Option<String>,
}

/// Decode a paste buffer: UTF-8 wg-quick, or standard/URL-safe base64 wrapping that text.
pub fn parse_wireguard_blob(raw: &str) -> Result<WgImport, ValidateError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(ValidateError::Message("empty WireGuard paste".into()));
    }
    let text = decode_maybe_base64(trimmed);
    parse_wg_quick(&text)
}

fn decode_maybe_base64(trimmed: &str) -> String {
    if trimmed.contains('[') {
        return trimmed.to_string();
    }
    let compact: String = trimmed.chars().filter(|c| !c.is_whitespace()).collect();
    let engines = [
        base64::engine::general_purpose::STANDARD,
        base64::engine::general_purpose::STANDARD_NO_PAD,
        base64::engine::general_purpose::URL_SAFE,
        base64::engine::general_purpose::URL_SAFE_NO_PAD,
    ];
    for engine in engines {
        if let Ok(bytes) = engine.decode(compact.as_bytes())
            && let Ok(text) = String::from_utf8(bytes)
            && text.contains('[')
        {
            return text;
        }
    }
    trimmed.to_string()
}

fn parse_wg_quick(text: &str) -> Result<WgImport, ValidateError> {
    let mut section = String::new();
    let mut out = WgImport::default();
    let mut saw_interface = false;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(name) = line.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            section = name.to_ascii_lowercase();
            if section == "interface" {
                saw_interface = true;
            }
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim().to_ascii_lowercase();
        let value = value.trim().to_string();
        match (section.as_str(), key.as_str()) {
            ("interface", "address") => out.address = Some(value),
            ("interface", "listenport") => {
                out.listen_port = value.parse().ok();
            }
            ("interface", "privatekey") => out.private_key = Some(value),
            ("peer", "publickey") => out.peer_public_key = Some(value),
            ("peer", "presharedkey") => out.preshared_key = Some(value),
            ("peer", "endpoint") => out.peer_endpoint = Some(value),
            ("peer", "allowedips") => out.peer_allowed_ips = Some(value),
            _ => {}
        }
    }
    if !saw_interface {
        return Err(ValidateError::Message(
            "not a wg-quick config ([Interface] missing)".into(),
        ));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::{parse_wg_quick, parse_wireguard_blob};
    use base64::Engine;

    const SAMPLE: &str = "[Interface]
PrivateKey = aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa=
Address = 10.66.0.2/32
ListenPort = 51820

[Peer]
PublicKey = bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb=
PresharedKey = ccccccccccccccccccccccccccccccccccccccccccc=
Endpoint = vps.example:51820
AllowedIPs = 0.0.0.0/0
";

    #[test]
    fn parses_plain_wg_quick() {
        let got = parse_wg_quick(SAMPLE).unwrap();
        assert_eq!(got.address.as_deref(), Some("10.66.0.2/32"));
        assert_eq!(got.listen_port, Some(51820));
        assert_eq!(got.peer_endpoint.as_deref(), Some("vps.example:51820"));
        assert!(got.private_key.is_some());
        assert!(got.peer_public_key.is_some());
    }

    #[test]
    fn parses_standard_base64() {
        let blob = base64::engine::general_purpose::STANDARD.encode(SAMPLE.as_bytes());
        let got = parse_wireguard_blob(&blob).unwrap();
        assert_eq!(got.address.as_deref(), Some("10.66.0.2/32"));
        assert_eq!(got.peer_allowed_ips.as_deref(), Some("0.0.0.0/0"));
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse_wireguard_blob("not-config").is_err());
    }
}
