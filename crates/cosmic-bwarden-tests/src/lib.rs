#[cfg(test)]
mod agent;
#[cfg(test)]
mod applet_flow;
#[cfg(test)]
mod browser_host;
#[cfg(test)]
mod cli_lifecycle;
#[cfg(test)]
mod common;
#[cfg(all(test, feature = "tpm-smoke"))]
mod common_tpm;
#[cfg(all(test, feature = "tpm-smoke"))]
mod tpm_lifecycle;
#[cfg(test)]
mod custom_fields_cli;
#[cfg(test)]
mod custom_fields_ui;
#[cfg(all(test, feature = "browser-e2e"))]
mod extension_e2e;
#[cfg(test)]
mod paths;
#[cfg(test)]
mod pinned_ops;
#[cfg(test)]
mod security;
#[cfg(test)]
mod ssh_test_utils;
#[cfg(test)]
mod sync_persistence;
#[cfg(test)]
mod vault;
#[cfg(test)]
mod version;
#[cfg(test)]
mod window_flow;
