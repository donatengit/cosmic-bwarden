//! Seal/unseal of raw bytes and vault keys — the public entry points.

use anyhow::{Context as _, Result};
use cosmic_bwarden_core::locked;
use std::path::Path;
use zeroize::Zeroize as _;
use tss_esapi::{
    attributes::SessionAttributesBuilder,
    constants::SessionType,
    handles::ObjectHandle,
    interface_types::{
        algorithm::HashingAlgorithm, reserved_handles::Hierarchy,
        session_handles::PolicySession,
    },
    structures::{Auth, Digest, SensitiveData, SymmetricDefinition},
    Context,
};

use super::blob::{read_blob, write_blob};
use super::open_context;
use super::policy::{compute_policy_digest, pcr_selection_list, primary_template, sealed_template};

/// Seals arbitrary bytes bound to PCR{0,7} ∧ `pin` (PolicyAuthValue). Pass
/// `pin = ""` for PCR-bound only (still requires matching PCRs, no PIN entropy).
/// The sealed blob is written to `blob_path` with mode 0600.
pub async fn seal_bytes(data: &[u8], pin: &str, blob_path: &Path) -> Result<()> {
    let mut ctx = open_context()?;

    let primary_tmpl = primary_template()?;
    let primary = ctx
        .execute_with_nullauth_session(|c| {
            c.create_primary(Hierarchy::Owner, primary_tmpl, None, None, None, None)
        })
        .context("creating TPM primary key")?;

    let policy_digest = compute_policy_digest(&mut ctx)?;
    let tmpl = sealed_template(policy_digest)?;
    let pin_auth = Auth::from_bytes(pin.as_bytes()).context("PIN too long for TPM auth")?;
    let sensitive_data = SensitiveData::try_from(data.to_vec())
        .context("data too large for TPM sensitive data")?;

    let result = ctx
        .execute_with_nullauth_session(|c| {
            c.create(
                primary.key_handle,
                tmpl,
                Some(pin_auth),
                Some(sensitive_data),
                None,
                None,
            )
        })
        .context("creating sealed TPM object")?;

    let _ = ctx.flush_context(primary.key_handle.into());

    write_blob(&result, blob_path)?;
    log::info!("TPM: sealed {} bytes to {} (PCR-bound)", data.len(), blob_path.display());
    Ok(())
}

/// Unseals bytes from `blob_path` using `pin`. Fails on wrong PIN (DA fault),
/// changed PCRs (firmware/Secure-Boot change), or an unsupported blob version.
pub async fn unseal_bytes(blob_path: &Path, pin: &str) -> Result<Vec<u8>> {
    let mut ctx = open_context()?;
    let (private, public) = read_blob(blob_path)?;

    let primary_tmpl = primary_template()?;
    let primary = ctx
        .execute_with_nullauth_session(|c| {
            c.create_primary(Hierarchy::Owner, primary_tmpl, None, None, None, None)
        })
        .context("recreating TPM primary key for unseal")?;

    let obj_handle = ctx
        .execute_with_nullauth_session(|c| c.load(primary.key_handle, private, public))
        .context("loading sealed object into TPM")?;

    let _ = ctx.flush_context(primary.key_handle.into());

    let sensitive = unseal_with_policy(&mut ctx, obj_handle.into(), pin)?;
    Ok(sensitive.as_bytes().to_vec())
}

/// Seals `vault_keys` (64 bytes) bound to PCR{0,7} ∧ `pin`.
pub async fn seal(vault_keys: &locked::Keys, pin: &str, blob_path: &Path) -> Result<()> {
    // Seal only enc_key ‖ mac_key (exactly 64 bytes) — keys.data() may be longer
    // due to PKCS7 padding left over from AES-CBC decryption in the locked buffer.
    let mut key_material = std::vec::Vec::with_capacity(64);
    key_material.extend_from_slice(vault_keys.enc_key());
    key_material.extend_from_slice(vault_keys.mac_key());
    let res = seal_bytes(&key_material, pin, blob_path).await;
    key_material.zeroize();
    res.context("sealing vault keys")?;
    log::info!("TPM: sealed vault keys to {}", blob_path.display());
    Ok(())
}

/// Unseals the vault keys stored in `blob_path` using `pin`.
pub async fn unseal(blob_path: &Path, pin: &str) -> Result<locked::Keys> {
    let raw = unseal_bytes(blob_path, pin).await?;
    anyhow::ensure!(
        raw.len() == 64,
        "TPM unsealed {} bytes; expected 64 (vault keys)",
        raw.len()
    );

    // Copy unsealed bytes into locked (mlock'd) memory, then wipe the transient.
    let mut locked_vec = locked::Vec::new();
    locked_vec.extend(raw.iter().copied());
    let mut raw = raw;
    raw.zeroize();
    Ok(locked::Keys::new(locked_vec))
}

/// Unseal `obj_handle` under a PolicyPCR(0,7) ∧ PolicyAuthValue session with
/// command/response parameter encryption, so the unsealed key is never exposed in
/// the clear on the TPM bus (mitigates bus sniffing).
fn unseal_with_policy(ctx: &mut Context, obj_handle: ObjectHandle, pin: &str) -> Result<SensitiveData> {
    let session = ctx
        .start_auth_session(
            None,
            None,
            None,
            SessionType::Policy,
            SymmetricDefinition::AES_128_CFB,
            HashingAlgorithm::Sha256,
        )
        .context("starting policy session for unseal")?
        .ok_or_else(|| anyhow::anyhow!("TPM returned no policy session handle"))?;
    let (attrs, mask) = SessionAttributesBuilder::new()
        .with_decrypt(true)
        .with_encrypt(true)
        .build();
    ctx.tr_sess_set_attributes(session, attrs, mask)
        .context("setting policy session attributes")?;

    let policy_session = PolicySession::try_from(session)
        .context("converting auth session to policy session")?;
    ctx.policy_pcr(policy_session, Digest::default(), pcr_selection_list()?)
        .context("policy_pcr (PCR state changed? re-run PIN setup)")?;
    ctx.policy_auth_value(policy_session)
        .context("policy_auth_value")?;

    let pin_auth = Auth::from_bytes(pin.as_bytes()).context("PIN too long for TPM auth")?;
    ctx.tr_set_auth(obj_handle, pin_auth)
        .context("setting PIN auth on TPM object")?;

    ctx.execute_with_session(Some(session), |c| c.unseal(obj_handle))
        .context("TPM unseal — wrong PIN, changed PCRs, or DA lockout")
}
