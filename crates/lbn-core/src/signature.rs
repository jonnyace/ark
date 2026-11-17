//! Publisher signatures on pack manifests and catalogs: minisign (Ed25519), verified with `minisign-verify`.
//! No cryptography is implemented here; this only selects the trusted key and maps errors.
use crate::error::{Error, Result};
use minisign_verify::{PublicKey, Signature};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrustedKey {
    /// Short name shown to people ("Ark sample publisher").
    pub name: String,
    /// The minisign public key, base64 (the second line of a `minisign.pub` file).
    pub public_key: String,
}

/// Verify `sig_text` (contents of a `.minisig` file) over `data` with any of the trusted keys.
/// Returns the name of the key that verified it.
pub fn verify<'a>(data: &[u8], sig_text: &str, keys: &'a [TrustedKey]) -> Result<&'a TrustedKey> {
    let sig = Signature::decode(sig_text).map_err(|e| Error::Signature(format!("signature file is damaged: {e}")))?;
    if keys.is_empty() {
        return Err(Error::Signature("no trusted publisher keys are configured".into()));
    }
    for k in keys {
        let Ok(pk) = PublicKey::from_base64(k.public_key.trim()) else { continue };
        if pk.verify(data, &sig, false).is_ok() {
            return Ok(k);
        }
    }
    Err(Error::Signature("the signature does not match this pack or any trusted publisher".into()))
}
