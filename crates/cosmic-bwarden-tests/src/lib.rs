#[cfg(test)]
mod agent;
#[cfg(test)]
mod applet_flow;
#[cfg(test)]
mod browser_host;
#[cfg(test)]
mod cli_lifecycle;
#[cfg(all(test, feature = "browser-e2e"))]
mod extension_e2e;
#[cfg(test)]
mod common;
#[cfg(test)]
mod custom_fields_cli;
#[cfg(test)]
mod custom_fields_ui;
#[cfg(test)]
mod paths;
#[cfg(test)]
mod pinned_ops;
#[cfg(test)]
mod security;
#[cfg(test)]
mod ssh_test_utils;
#[cfg(test)]
mod vault;
#[cfg(test)]
mod window_flow;
