// Thin entry point for the default `cosmarden-agent` binary.
// All logic lives in the crate library (`lib.rs`), shared with the
// TPM-enabled `cosmarden-agent-tpm` binary (`bin/cosmarden-agent-tpm.rs`).
#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    cosmarden_agent::run().await
}
