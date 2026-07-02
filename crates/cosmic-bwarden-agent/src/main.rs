// Thin entry point for the default `cosmic-bwarden-agent` binary.
// All logic lives in the crate library (`lib.rs`), shared with the
// TPM-enabled `cosmic-bwarden-agent-tpm` binary (`bin/cosmic-bwarden-agent-tpm.rs`).
#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    cosmic_bwarden_agent::run().await
}
