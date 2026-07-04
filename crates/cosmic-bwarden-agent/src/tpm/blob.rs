//! On-disk sealed-blob format (postcard, mode 0600) and its version check.

use anyhow::{Context as _, Result};
use std::path::Path;
use tss_esapi::{
    structures::{CreateKeyResult, Private, Public},
    traits::{Marshall, UnMarshall},
};

/// Sealed-blob format version. v2 binds the object to PCR{0,7} ∧ PolicyAuthValue.
/// v1 (no policy) blobs cannot be unsealed by this code — the user must re-run PIN
/// setup (also required after a firmware/Secure-Boot change invalidates the PCRs).
const SEALED_BLOB_VERSION: u8 = 2;

#[derive(serde::Serialize, serde::Deserialize)]
struct SealedBlob {
    #[serde(default)]
    version: u8,
    out_private: Vec<u8>,
    out_public: Vec<u8>,
}

/// Serialize a freshly created sealed object to disk (0600, versioned).
pub(super) fn write_blob(result: &CreateKeyResult, blob_path: &Path) -> Result<()> {
    let blob = SealedBlob {
        version: SEALED_BLOB_VERSION,
        out_private: result
            .out_private
            .marshall()
            .context("marshalling TPM private")?,
        out_public: result
            .out_public
            .marshall()
            .context("marshalling TPM public")?,
    };
    let blob_bytes = postcard::to_allocvec(&blob).context("serializing TPM blob")?;

    use std::os::unix::fs::OpenOptionsExt as _;
    std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(blob_path)
        .and_then(|mut f| {
            use std::io::Write as _;
            f.write_all(&blob_bytes)
        })
        .context("writing TPM blob to disk")?;
    Ok(())
}

/// Read and version-check a sealed blob, returning its TPM private/public parts.
pub(super) fn read_blob(blob_path: &Path) -> Result<(Private, Public)> {
    let blob_bytes = std::fs::read(blob_path).context("reading TPM blob file")?;
    let blob: SealedBlob = postcard::from_bytes(&blob_bytes).context("deserializing TPM blob")?;
    anyhow::ensure!(
        blob.version == SEALED_BLOB_VERSION,
        "sealed blob version {} is not supported (expected {}); re-run PIN setup",
        blob.version,
        SEALED_BLOB_VERSION
    );
    let private =
        Private::unmarshall(&blob.out_private).context("deserializing TPM private portion")?;
    let public =
        Public::unmarshall(&blob.out_public).context("deserializing TPM public portion")?;
    Ok((private, public))
}
