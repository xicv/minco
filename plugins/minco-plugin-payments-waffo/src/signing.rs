use crate::WaffoError;
use async_trait::async_trait;
use aws_lc_rs::{rand::SystemRandom, rsa, signature};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use minco_config::SecretReference;
use sha2::{Digest, Sha256};
use std::{fmt, sync::Arc};
use zeroize::Zeroizing;

/// Resolved secret material that is zeroized when dropped and redacted in diagnostics.
pub struct SecretValue(Zeroizing<String>);

impl SecretValue {
    pub fn new(value: impl Into<String>) -> Self {
        Self(Zeroizing::new(value.into()))
    }

    pub(super) fn expose(&self) -> &str {
        self.0.as_str()
    }

    pub(super) fn expose_for_verification(&self) -> &str {
        self.0.as_str()
    }
}

impl fmt::Debug for SecretValue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SecretValue([REDACTED])")
    }
}

/// Runtime boundary for resolving Minco's opaque `env:` or `ssm:` references.
#[async_trait]
pub trait SecretResolver: Send + Sync {
    async fn resolve(&self, reference: &SecretReference) -> Result<SecretValue, WaffoError>;
}

/// Local resolver that intentionally supports environment references only.
#[derive(Debug, Default, Clone, Copy)]
pub struct EnvironmentSecretResolver;

#[async_trait]
impl SecretResolver for EnvironmentSecretResolver {
    async fn resolve(&self, reference: &SecretReference) -> Result<SecretValue, WaffoError> {
        match reference {
            SecretReference::EnvironmentVariable { name } => std::env::var(name)
                .map(SecretValue::new)
                .map_err(|_| WaffoError::SecretResolution),
            SecretReference::SystemsManagerParameter { .. } => {
                Err(WaffoError::UnsupportedSecretProvider)
            }
        }
    }
}

#[derive(Clone)]
pub(super) struct RequestSigner {
    key_pair: Arc<rsa::KeyPair>,
}

impl fmt::Debug for RequestSigner {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RequestSigner")
            .field("algorithm", &"RSA-PKCS1-SHA256")
            .finish_non_exhaustive()
    }
}

impl RequestSigner {
    pub(super) fn from_pem(private_key: &str) -> Result<Self, WaffoError> {
        let (label, der) = decode_pem(private_key, &["PRIVATE KEY", "RSA PRIVATE KEY"])
            .map_err(|()| WaffoError::InvalidPrivateKey)?;
        let key_pair = match label {
            "PRIVATE KEY" => rsa::KeyPair::from_pkcs8(&der),
            "RSA PRIVATE KEY" => rsa::KeyPair::from_der(&der),
            _ => unreachable!("PEM label was selected from a fixed allowlist"),
        }
        .map_err(|_| WaffoError::InvalidPrivateKey)?;
        Ok(Self {
            key_pair: Arc::new(key_pair),
        })
    }

    #[cfg(test)]
    pub(super) fn from_key_pair(key_pair: rsa::KeyPair) -> Self {
        Self {
            key_pair: Arc::new(key_pair),
        }
    }

    pub(super) fn sign(&self, canonical_request: &[u8]) -> Result<String, WaffoError> {
        let mut signature = vec![0_u8; self.key_pair.public_modulus_len()];
        self.key_pair
            .sign(
                &signature::RSA_PKCS1_SHA256,
                &SystemRandom::new(),
                canonical_request,
                &mut signature,
            )
            .map_err(|_| WaffoError::SigningFailed)?;
        Ok(STANDARD.encode(signature))
    }
}

pub(super) fn canonical_request(method: &str, path: &str, timestamp: i64, body: &[u8]) -> String {
    let body_hash = STANDARD.encode(Sha256::digest(body));
    format!("{method}\n{path}\n{timestamp}\n{body_hash}")
}

pub(super) fn decode_pem(
    value: &str,
    accepted_labels: &[&'static str],
) -> Result<(&'static str, Zeroizing<Vec<u8>>), ()> {
    if value.is_empty() || !value.is_ascii() {
        return Err(());
    }
    let normalized = normalize_newlines(value);
    let value = normalized.trim();
    if value.is_empty() {
        return Err(());
    }

    for &label in accepted_labels {
        let begin = format!("-----BEGIN {label}-----");
        let end = format!("-----END {label}-----");
        let Some(after_begin) = value.strip_prefix(&begin) else {
            continue;
        };
        let Some((body, after_end)) = after_begin.split_once(&end) else {
            return Err(());
        };
        if !after_end.trim().is_empty() {
            return Err(());
        }
        let encoded = compact_ascii_whitespace(body);
        if encoded.is_empty() {
            return Err(());
        }
        let der = STANDARD.decode(encoded.as_bytes()).map_err(|_| ())?;
        return Ok((label, Zeroizing::new(der)));
    }

    if value.contains("-----BEGIN ") || value.contains("-----END ") {
        return Err(());
    }
    let label = accepted_labels.first().copied().ok_or(())?;
    let encoded = compact_ascii_whitespace(value);
    if encoded.is_empty() {
        return Err(());
    }
    let der = STANDARD.decode(encoded.as_bytes()).map_err(|_| ())?;
    Ok((label, Zeroizing::new(der)))
}

fn normalize_newlines(value: &str) -> Zeroizing<String> {
    let bytes = value.as_bytes();
    let mut normalized = Zeroizing::new(String::with_capacity(bytes.len()));
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index..].starts_with(br"\r\n") {
            normalized.push('\n');
            index += 4;
        } else if bytes[index..].starts_with(br"\n") {
            normalized.push('\n');
            index += 2;
        } else if bytes[index] == b'\r' {
            normalized.push('\n');
            index += usize::from(bytes.get(index + 1) == Some(&b'\n')) + 1;
        } else {
            normalized.push(char::from(bytes[index]));
            index += 1;
        }
    }
    normalized
}

fn compact_ascii_whitespace(value: &str) -> Zeroizing<String> {
    let mut compact = Zeroizing::new(String::with_capacity(value.len()));
    compact.extend(
        value
            .chars()
            .filter(|character| !character.is_ascii_whitespace()),
    );
    compact
}

#[cfg(test)]
mod tests {
    use super::*;
    use aws_lc_rs::{rsa::KeySize, signature::KeyPair as _};

    #[test]
    fn canonical_request_matches_documented_shape() {
        assert_eq!(
            canonical_request(
                "POST",
                "/v1/actions/checkout/create-session",
                1_705_312_200,
                b"{}"
            ),
            "POST\n/v1/actions/checkout/create-session\n1705312200\nRBNvo1WzZ4oRRq0W9+hknpT7T8If536DEMBg9hyq/4o="
        );
    }

    #[test]
    fn generated_signature_verifies_with_matching_public_key() {
        let key_pair = rsa::KeyPair::generate(KeySize::Rsa2048).unwrap();
        let public_key = key_pair.public_key().as_ref().to_vec();
        let signer = RequestSigner::from_key_pair(key_pair);
        let message = b"POST\n/v1/actions/store/create-store\n1705312200\nabc";
        let encoded = signer.sign(message).unwrap();
        let signature_bytes = STANDARD.decode(encoded).unwrap();

        signature::UnparsedPublicKey::new(&signature::RSA_PKCS1_2048_8192_SHA256, public_key)
            .verify(message, &signature_bytes)
            .unwrap();
    }

    #[test]
    fn pem_decoder_accepts_environment_and_raw_base64_forms() {
        let escaped = "-----BEGIN PRIVATE KEY-----\\nAQID\\n-----END PRIVATE KEY-----";
        let (label, decoded) = decode_pem(escaped, &["PRIVATE KEY", "RSA PRIVATE KEY"]).unwrap();
        assert_eq!(label, "PRIVATE KEY");
        assert_eq!(decoded.as_slice(), &[1, 2, 3]);

        let (label, decoded) = decode_pem("AQID", &["PUBLIC KEY", "RSA PUBLIC KEY"]).unwrap();
        assert_eq!(label, "PUBLIC KEY");
        assert_eq!(decoded.as_slice(), &[1, 2, 3]);
    }
}
