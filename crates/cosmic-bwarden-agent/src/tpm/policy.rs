//! TPM object templates and the PolicyPCR(0,7) ∧ PolicyAuthValue digest that
//! binds sealed blobs to firmware/Secure-Boot state plus the user PIN.

use anyhow::{Context as _, Result};
use tss_esapi::{
    attributes::{ObjectAttributesBuilder, SessionAttributesBuilder},
    constants::SessionType,
    interface_types::{
        algorithm::{HashingAlgorithm, PublicAlgorithm},
        session_handles::PolicySession,
    },
    structures::{
        Digest, KeyedHashScheme, PcrSelectionList, PcrSlot, Public, PublicBuilder,
        PublicKeyedHashParameters, SymmetricCipherParameters, SymmetricDefinition,
        SymmetricDefinitionObject,
    },
    Context,
};

/// AES-128-CFB symmetric cipher storage parent (same template as tss-esapi examples).
/// Creating a primary key with this exact template always produces the same key
/// (deterministic from the TPM's owner-hierarchy seed), so we never need to persist it.
pub(super) fn primary_template() -> Result<Public> {
    let attrs = ObjectAttributesBuilder::new()
        .with_fixed_tpm(true)
        .with_fixed_parent(true)
        .with_st_clear(false)
        .with_sensitive_data_origin(true)
        .with_user_with_auth(true)
        .with_decrypt(true)
        .with_restricted(true)
        .build()
        .context("building primary key attributes")?;

    PublicBuilder::new()
        .with_public_algorithm(PublicAlgorithm::SymCipher)
        .with_name_hashing_algorithm(HashingAlgorithm::Sha256)
        .with_object_attributes(attrs)
        .with_symmetric_cipher_parameters(SymmetricCipherParameters::new(
            SymmetricDefinitionObject::AES_128_CFB,
        ))
        .with_symmetric_cipher_unique_identifier(Digest::default())
        .build()
        .context("building primary key template")
}

/// PCR selection the sealed blobs are bound to: SHA-256 bank, PCR 0 (firmware /
/// UEFI code) and PCR 7 (Secure Boot state). Booting different firmware or
/// changing Secure Boot changes these, so the policy no longer satisfies and the
/// blob cannot be unsealed — the intended anti-evil-maid property.
pub(super) fn pcr_selection_list() -> Result<PcrSelectionList> {
    PcrSelectionList::builder()
        .with_selection(HashingAlgorithm::Sha256, &[PcrSlot::Slot0, PcrSlot::Slot7])
        .build()
        .context("building PCR selection list")
}

/// Compute the authPolicy digest for "PolicyPCR(0,7) ∧ PolicyAuthValue" using a
/// trial session. Sealing sets this as the object's authPolicy; unsealing must
/// satisfy the same policy (correct PCRs) AND supply the PIN (auth value).
pub(super) fn compute_policy_digest(ctx: &mut Context) -> Result<Digest> {
    let trial = ctx
        .start_auth_session(
            None,
            None,
            None,
            SessionType::Trial,
            SymmetricDefinition::AES_128_CFB,
            HashingAlgorithm::Sha256,
        )
        .context("starting trial policy session")?
        .ok_or_else(|| anyhow::anyhow!("TPM returned no trial session handle"))?;
    let (attrs, mask) = SessionAttributesBuilder::new().build();
    ctx.tr_sess_set_attributes(trial, attrs, mask)
        .context("setting trial session attributes")?;

    let policy_session =
        PolicySession::try_from(trial).context("converting trial session to policy session")?;
    ctx.policy_pcr(policy_session, Digest::default(), pcr_selection_list()?)
        .context("trial policy_pcr")?;
    ctx.policy_auth_value(policy_session)
        .context("trial policy_auth_value")?;
    ctx.policy_get_digest(policy_session)
        .context("reading trial policy digest")
    // `trial` flushes when `ctx` drops (handle manager flushes on drop).
}

/// Sealed-data-object template bound to `policy_digest`. `userWithAuth=false` means
/// the PIN (object auth value) is usable *only* through the policy, which also
/// requires the PCRs to match. DA lockout stays enabled (no_da not set).
pub(super) fn sealed_template(policy_digest: Digest) -> Result<Public> {
    let attrs = ObjectAttributesBuilder::new()
        .with_fixed_tpm(true)
        .with_fixed_parent(true)
        .with_st_clear(false)
        .with_user_with_auth(false)
        // with_no_da NOT set → TPM enforces dictionary-attack lockout on wrong PINs
        .build()
        .context("building sealed object attributes")?;

    PublicBuilder::new()
        .with_public_algorithm(PublicAlgorithm::KeyedHash)
        .with_name_hashing_algorithm(HashingAlgorithm::Sha256)
        .with_object_attributes(attrs)
        .with_keyed_hash_parameters(PublicKeyedHashParameters::new(KeyedHashScheme::Null))
        .with_keyed_hash_unique_identifier(Digest::default())
        .with_auth_policy(policy_digest)
        .build()
        .context("building sealed object template")
}
