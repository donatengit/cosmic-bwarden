// TPM-enabled agent binary, gated by `required-features = ["tpm"]` in Cargo.toml.
// Identical to the default binary except for its name — the TPM E2E suite locates
// `target/debug/cosmarden-agent-tpm` to be certain it is running a build with
// TPM support compiled in. Logic is shared via the crate library.
#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    cosmarden_agent::run().await
}
