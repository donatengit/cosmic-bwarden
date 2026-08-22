use super::identities::LOCKED_COMMENT_TOKEN;
use super::SshAgent;
use crate::state::State;
use cosmic_bwarden_core::api::CipherRepromptType;
use cosmic_bwarden_core::cipherstring::CipherString;
use cosmic_bwarden_core::db::{Db, Entry, EntryData};
use cosmic_bwarden_core::locked;
use ssh_agent_lib::agent::Session;
use ssh_agent_lib::proto::SignRequest;
use ssh_agent_lib::ssh_key::public::KeyData;
use signature::Verifier as _;
use ssh_agent_lib::ssh_key::{PublicKey, Signature};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

const FIXTURE_PUB: &str =
    "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIJHyaFrh+3ZryOJGk8ShXZHLPBvwT1wwh5Db5/5/adxp unit-test-key";

const FIXTURE_PRIV: &str = "-----BEGIN OPENSSH PRIVATE KEY-----
b3BlbnNzaC1rZXktdjEAAAAABG5vbmUAAAAEbm9uZQAAAAAAAAABAAAAMwAAAAtzc2gtZW
QyNTUxOQAAACCR8mha4ft2a8jiRpPEoV2Ryzwb8E9cMIeQ2+f+f2ncaQAAAJBPRSBtT0Ug
bQAAAAtzc2gtZWQyNTUxOQAAACCR8mha4ft2a8jiRpPEoV2Ryzwb8E9cMIeQ2+f+f2ncaQ
AAAEC7+psTYWI53AzE6HJvhVuiJ4TFoviZqCet6xr9LWAg0pHyaFrh+3ZryOJGk8ShXZHL
PBvwT1wwh5Db5/5/adxpAAAADXVuaXQtdGVzdC1rZXk=
-----END OPENSSH PRIVATE KEY-----
";

fn test_keys() -> locked::Keys {
    let mut key_data = locked::Vec::new();
    key_data.extend([0u8; 64].iter().copied());
    locked::Keys::new(key_data)
}

fn enc(keys: &locked::Keys, s: &str) -> String {
    CipherString::encrypt_symmetric(keys, s.as_bytes())
        .unwrap()
        .to_string()
}

fn fixture_pubkey() -> KeyData {
    let pk: PublicKey = FIXTURE_PUB.parse().unwrap();
    pk.key_data().clone()
}

fn unlocked_state() -> (State, locked::Keys) {
    let keys = test_keys();
    let entry = Entry {
        id: "ssh-1".into(),
        org_id: None,
        folder: None,
        folder_id: None,
        name: enc(&keys, "My Work Key"),
        favorite: false,
        data: EntryData::SshKey {
            private_key: Some(enc(&keys, FIXTURE_PRIV).into()),
            public_key: Some(enc(&keys, FIXTURE_PUB)),
            fingerprint: None,
        },
        fields: Vec::new(),
        notes: None,
        history: Vec::new(),
        key: None,
        master_password_reprompt: CipherRepromptType::None,
    };
    let mut db = Db::new();
    db.entries.push(entry);
    let mut state = State::new();
    state.keys = Some(keys.clone());
    state.db = Some(db);
    state.rebuild_sidebar_cache();
    (state, keys)
}

fn verify_sig(sig: &Signature, data: &[u8]) {
    let pk: PublicKey = FIXTURE_PUB.parse().unwrap();
    pk.key_data()
        .verify(data, sig)
        .expect("signature must verify with the cached public key");
}

#[tokio::test]
async fn unlocked_list_has_real_blob_without_locked_token() {
    let (state, _) = unlocked_state();
    let mut agent = SshAgent::new(Arc::new(Mutex::new(state)));
    let ids = agent.request_identities().await.unwrap();
    assert_eq!(ids.len(), 1, "must not advertise a dummy identity");
    assert_eq!(ids[0].pubkey, fixture_pubkey());
    assert_eq!(ids[0].comment, "My Work Key");
    assert!(
        !ids[0].comment.contains(LOCKED_COMMENT_TOKEN),
        "unlocked comment must not carry the locked token"
    );
}

#[tokio::test]
async fn lock_keeps_same_blob_and_suffixes_comment() {
    let (state, keys) = unlocked_state();
    let state = Arc::new(Mutex::new(state));
    let mut agent = SshAgent::new(Arc::clone(&state));

    let unlocked = agent.request_identities().await.unwrap();
    {
        let mut g = state.lock().await;
        g.lock();
        assert!(
            !g.unlock_requested_notified,
            "lock must reset the unlock-request debounce"
        );
    }
    let locked = agent.request_identities().await.unwrap();
    assert!(
        state.lock().await.unlock_requested_notified,
        "listing while locked must still call request_unlock"
    );
    assert_eq!(locked.len(), 1);
    assert_eq!(locked[0].pubkey, unlocked[0].pubkey);
    assert!(
        locked[0].comment.contains(LOCKED_COMMENT_TOKEN),
        "locked comment must contain {LOCKED_COMMENT_TOKEN}, got {:?}",
        locked[0].comment
    );

    {
        let mut g = state.lock().await;
        g.keys = Some(keys);
        g.rebuild_sidebar_cache();
    }
    let relisted = agent.request_identities().await.unwrap();
    assert_eq!(relisted.len(), 1);
    assert_eq!(relisted[0].pubkey, unlocked[0].pubkey);
    assert!(
        !relisted[0].comment.contains(LOCKED_COMMENT_TOKEN),
        "unlock must drop the locked token, got {:?}",
        relisted[0].comment
    );
}

#[tokio::test]
async fn never_unlocked_and_logout_and_unauthorized_list_empty() {
    let mut agent = SshAgent::new(Arc::new(Mutex::new(State::new())));
    assert!(agent.request_identities().await.unwrap().is_empty());

    let (state, _) = unlocked_state();
    let state = Arc::new(Mutex::new(state));
    let mut agent = SshAgent::new(Arc::clone(&state));
    assert_eq!(agent.request_identities().await.unwrap().len(), 1);
    {
        let mut g = state.lock().await;
        g.clear_account();
    }
    assert!(agent.request_identities().await.unwrap().is_empty());

    let (state, _) = unlocked_state();
    let mut agent = SshAgent::unauthorized(Arc::new(Mutex::new(state)));
    assert!(agent.request_identities().await.unwrap().is_empty());
}

#[tokio::test]
async fn sign_while_locked_waits_then_succeeds_after_unlock() {
    let (state, keys) = unlocked_state();
    let state = Arc::new(Mutex::new(state));
    let agent = SshAgent::with_sign_wait(Arc::clone(&state), Duration::from_secs(5));
    {
        let mut g = state.lock().await;
        g.lock();
    }

    let request = SignRequest {
        pubkey: fixture_pubkey(),
        data: b"challenge-bytes".to_vec(),
        flags: 0,
    };
    let pending = tokio::spawn({
        let mut agent = agent.clone();
        let request = request.clone();
        async move { agent.sign(request).await }
    });
    tokio::time::sleep(Duration::from_millis(80)).await;
    assert!(
        !pending.is_finished(),
        "sign must not complete (or emit a vault signature) before unlock"
    );

    {
        let mut g = state.lock().await;
        g.keys = Some(keys);
        g.rebuild_sidebar_cache();
    }
    let sig = pending
        .await
        .expect("sign task")
        .expect("sign after unlock");
    verify_sig(&sig, b"challenge-bytes");

    // Control: the same request while still locked on a fresh agent with a
    // tiny wait must not yield a verifying signature.
    let (state2, _) = unlocked_state();
    let state2 = Arc::new(Mutex::new(state2));
    {
        let mut g = state2.lock().await;
        g.lock();
    }
    let mut short = SshAgent::with_sign_wait(state2, Duration::from_millis(120));
    let err = short.sign(request).await.unwrap_err();
    let msg = format!("{err:#}");
    assert!(
        msg.contains("timed out"),
        "timeout without unlock must fail, got {msg}"
    );
}

#[tokio::test]
async fn unauthorized_sign_fails_without_waiting() {
    let (state, _) = unlocked_state();
    let mut agent = SshAgent::unauthorized(Arc::new(Mutex::new(state)));
    let err = agent
        .sign(SignRequest {
            pubkey: fixture_pubkey(),
            data: b"x".to_vec(),
            flags: 0,
        })
        .await
        .unwrap_err();
    assert!(format!("{err:#}").contains("unauthorized"));
}
