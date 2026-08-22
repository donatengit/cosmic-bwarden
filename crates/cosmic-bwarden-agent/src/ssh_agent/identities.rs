use crate::state::State;
use cosmic_bwarden_core::protocol::EntryType;
use ssh_agent_lib::proto::Identity;
use ssh_agent_lib::ssh_key::public::KeyData;
use ssh_agent_lib::ssh_key::PublicKey;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Appended to each identity comment while the vault is locked in this process.
/// Stable, not localised — scripts and coding agents grep `ssh-add -l` for it.
pub const LOCKED_COMMENT_TOKEN: &str = "[cosmic-bwarden:locked]";

/// Decrypted public key + vault entry name. Survives `State::lock()`; cleared
/// on logout. Private keys are never stored here.
#[derive(Clone, Debug)]
pub struct CachedSshIdentity {
    pub pubkey: KeyData,
    pub comment: String,
}

pub fn locked_comment(comment: &str) -> String {
    format!("{comment} {LOCKED_COMMENT_TOKEN}")
}

pub fn identities_for_list(cache: &[CachedSshIdentity], locked: bool) -> Vec<Identity> {
    cache
        .iter()
        .map(|id| Identity {
            pubkey: id.pubkey.clone(),
            comment: if locked {
                locked_comment(&id.comment)
            } else {
                id.comment.clone()
            },
        })
        .collect()
}

pub fn cache_has_pubkey(cache: &[CachedSshIdentity], pubkey: &KeyData) -> bool {
    cache.iter().any(|id| id.pubkey == *pubkey)
}

/// Rebuild the SSH identity cache from the just-rebuilt sidebar cache.
/// Call only while `state.keys` is populated (unlock / mutation).
pub fn rebuild_from_sidebar(state: &mut State) {
    state.ssh_identity_cache.clear();
    for cached in &state.sidebar_cache {
        if cached.entry.entry_type != EntryType::SshKey {
            continue;
        }
        let Some(pk_str) = cached.entry.public_key.as_deref() else {
            continue;
        };
        if let Ok(pk) = pk_str.parse::<PublicKey>() {
            state.ssh_identity_cache.push(CachedSshIdentity {
                pubkey: pk.key_data().clone(),
                comment: cached.entry.name.clone(),
            });
        }
    }
}

pub async fn list_identities(
    state: &Arc<Mutex<State>>,
) -> Result<Vec<Identity>, ssh_agent_lib::error::AgentError> {
    let mut state = state.lock().await;
    let locked = state.keys.is_none();
    if locked {
        state.request_unlock();
    }
    Ok(identities_for_list(&state.ssh_identity_cache, locked))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ssh_agent_lib::ssh_key::public::KeyData;

    fn dummy_key() -> KeyData {
        let pk: PublicKey = FIXTURE_PUB.parse().unwrap();
        pk.key_data().clone()
    }

    const FIXTURE_PUB: &str =
        "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIJHyaFrh+3ZryOJGk8ShXZHLPBvwT1wwh5Db5/5/adxp unit-test-key";

    #[test]
    fn locked_comment_appends_token_once() {
        let c = locked_comment("My Work Key");
        assert_eq!(c, "My Work Key [cosmic-bwarden:locked]");
        assert!(c.contains(LOCKED_COMMENT_TOKEN));
    }

    #[test]
    fn list_suffix_only_when_locked() {
        let cache = vec![CachedSshIdentity {
            pubkey: dummy_key(),
            comment: "My Work Key".into(),
        }];
        let locked = identities_for_list(&cache, true);
        let unlocked = identities_for_list(&cache, false);
        assert_eq!(locked.len(), 1);
        assert_eq!(unlocked.len(), 1);
        assert_eq!(locked[0].pubkey, unlocked[0].pubkey);
        assert!(locked[0].comment.contains(LOCKED_COMMENT_TOKEN));
        assert!(!unlocked[0].comment.contains(LOCKED_COMMENT_TOKEN));
        assert_eq!(unlocked[0].comment, "My Work Key");
    }
}
