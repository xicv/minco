//! Waffo short-ID validation shared by payment request contracts.
#![allow(clippy::redundant_pub_crate)]

pub(super) fn validate_short_id(value: &str, prefix: &str) -> Result<(), ()> {
    let Some(suffix) = value
        .strip_prefix(prefix)
        .and_then(|value| value.strip_prefix('_'))
    else {
        return Err(());
    };
    if suffix.len() != 22 || !suffix.bytes().all(|byte| byte.is_ascii_alphanumeric()) {
        return Err(());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_only_provider_short_id_shape() {
        assert!(validate_short_id("PROD_0123456789ABCDEFGHIJKL", "PROD").is_ok());
        assert!(validate_short_id("PROD_ABC123", "PROD").is_err());
        assert!(validate_short_id("STO_0123456789ABCDEFGHIJK!", "STO").is_err());
    }
}
