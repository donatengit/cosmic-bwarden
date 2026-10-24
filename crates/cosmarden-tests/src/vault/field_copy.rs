//! End-to-end coverage for every copy button in the desktop detail pane.
//!
//! A plain selection uses `GetEntryMeta`, which redacts every secret-class
//! field. Each copy button then either copies the value already in the entry
//! (public key, usernames, labels) or asks the agent for exactly one field.
//! Both the choice of field class and the action/answer mapping live in
//! `cosmarden_core::field_access`, which `view/vault/field.rs` calls — so this
//! test sends the same action the button sends and asserts the text the UI
//! would put on the clipboard.
//!
//! Regression: the SSH *public* key was classified as secret because its label
//! contains "key". Its copy button sent `CopySecretField`, `field_plaintext`
//! had no public-key arm, and the clipboard silently stayed unchanged.

use crate::common::{register_user, setup_env};
use anyhow::Result;
use cosmarden_core::agent_client::AgentClient;
use cosmarden_core::api::FieldType;
use cosmarden_core::db::{Entry, EntryData, Field, Secret};
use cosmarden_core::field_access::{
    field_is_secret, field_plaintext, on_demand_secret, parse_on_demand_response, secret_is_loaded,
    OnDemandPayload,
};
use cosmarden_core::protocol::{Action, EntryType, Response};

async fn login(client: &AgentClient, vault_url: &str, email: &str, password: &str) -> Result<()> {
    client
        .send(Action::Login {
            email: email.to_string(),
            password: password.to_string(),
            server_url: Some(vault_url.to_string()),
            remember_me: true,
            two_factor_token: None,
            two_factor_provider: None,
            two_factor_code: None,
            device_verification_code: None,
        })
        .await?;
    Ok(())
}

async fn ack(client: &AgentClient, action: Action) -> Result<()> {
    match client.send(action).await? {
        Response::Ack => Ok(()),
        other => anyhow::bail!("expected Ack, got {other:?}"),
    }
}

async fn sidebar_id(client: &AgentClient, name: &str) -> Result<String> {
    let res = client
        .send(Action::GetSidebarEntries {
            query: None,
            entry_type: None,
            only_pinned: false,
            domain: None,
        })
        .await?;
    match res {
        Response::SidebarEntries { entries } => entries
            .into_iter()
            .find(|e| e.name == name)
            .map(|e| e.id)
            .ok_or_else(|| anyhow::anyhow!("entry {name:?} not found in the sidebar")),
        other => anyhow::bail!("expected SidebarEntries, got {other:?}"),
    }
}

/// The redacted entry a selection stores in the detail pane.
async fn meta(client: &AgentClient, id: &str) -> Result<Entry> {
    match client
        .send(Action::GetEntryMeta { id: id.to_string() })
        .await?
    {
        Response::EntryMeta { entry, .. } => Ok(entry),
        other => anyhow::bail!("expected EntryMeta, got {other:?}"),
    }
}

/// Run one copy button: send the field's action and return the clipboard text.
async fn copy_field(client: &AgentClient, id: &str, field: &str) -> Result<String> {
    let action = on_demand_secret(field, id.to_string(), None);
    let payload = parse_on_demand_response(field, client.send(action).await?)
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    match payload {
        OnDemandPayload::Password(secret) | OnDemandPayload::Totp(secret) => {
            Ok(secret.expose().to_string())
        }
        OnDemandPayload::Entry(entry) => field_plaintext(&entry, field)
            .ok_or_else(|| anyhow::anyhow!("entry response carries no plaintext for {field:?}")),
    }
}

/// Assert one redacted field is fetched and copies exactly `expected`.
async fn expect_secret_copy(
    client: &AgentClient,
    id: &str,
    redacted: &Entry,
    field: &str,
    expected: &str,
) -> Result<()> {
    assert!(
        !secret_is_loaded(redacted, field),
        "{field:?} must be redacted in GetEntryMeta"
    );
    assert_eq!(
        field_plaintext(redacted, field),
        None,
        "{field:?} meta leak"
    );
    let copied = copy_field(client, id, field).await?;
    assert_eq!(copied, expected, "copy button for {field:?}");
    Ok(())
}

fn secret(v: &str) -> Option<Secret> {
    Some(v.to_string().into())
}

async fn replace_entry(client: &AgentClient, entry: Entry) -> Result<()> {
    ack(client, Action::UpdateEntry { entry }).await?;
    client.send(Action::Sync).await?;
    Ok(())
}

#[tokio::test]
async fn test_detail_copy_buttons_round_trip_every_secret_field() -> Result<()> {
    let env = setup_env().await?;
    let _profile = crate::state_guard::ProfileEnv::set(&env.profile);

    let email = "copy-buttons@example.com";
    let password = "copypassword123";
    register_user(&env.vault_url, email, password).await?;

    let client = AgentClient::new_with_socket(env.socket_path.clone());
    login(&client, &env.vault_url, email, password).await?;

    let hidden = Field {
        ty: Some(FieldType::Hidden),
        name: Some("Hidden Note".into()),
        value: secret("hidden-value"),
        linked_id: None,
    };
    let text = Field {
        ty: Some(FieldType::Text),
        name: Some("Comment".into()),
        value: secret("plain-value"),
        linked_id: None,
    };

    // Login: TOTP is added by a second write, as the UI's edit form does.
    ack(
        &client,
        Action::AddEntry {
            name: "Copy Login".into(),
            entry_type: EntryType::Login,
            username: Some("alice".into()),
            password: secret("login-pass"),
            notes: secret("login notes"),
            fields: vec![hidden.clone(), text.clone()],
            totp: None,
            uris: Vec::new(),
        },
    )
    .await?;
    client.send(Action::Sync).await?;
    let login_id = sidebar_id(&client, "Copy Login").await?;
    let mut login_entry = match client
        .send(Action::GetEntry {
            id: login_id.clone(),
            password: None,
        })
        .await?
    {
        Response::Entry { entry } => entry,
        other => anyhow::bail!("expected Entry, got {other:?}"),
    };
    if let EntryData::Login { ref mut totp, .. } = login_entry.data {
        *totp = secret("JBSWY3DPEHPK3PXP");
    }
    replace_entry(&client, login_entry).await?;

    ack(
        &client,
        Action::AddCard {
            name: "Copy Card".into(),
            cardholder_name: Some("Alice Smith".into()),
            number: secret("4111111111111111"),
            brand: Some("Visa".into()),
            exp_month: Some("12".into()),
            exp_year: Some("2030".into()),
            code: secret("321"),
            notes: secret("card notes"),
            fields: Vec::new(),
        },
    )
    .await?;
    client.send(Action::Sync).await?;

    ack(
        &client,
        Action::AddIdentity {
            name: "Copy Identity".into(),
            first_name: Some("Alice".into()),
            last_name: Some("Smith".into()),
            address1: None,
            city: None,
            state: None,
            postal_code: None,
            country: None,
            email: Some("alice@example.com".into()),
            phone: None,
            notes: secret("identity notes"),
            fields: Vec::new(),
        },
    )
    .await?;
    client.send(Action::Sync).await?;
    let identity_id = sidebar_id(&client, "Copy Identity").await?;
    let mut identity_entry = match client
        .send(Action::GetEntry {
            id: identity_id.clone(),
            password: None,
        })
        .await?
    {
        Response::Entry { entry } => entry,
        other => anyhow::bail!("expected Entry, got {other:?}"),
    };
    if let EntryData::Identity {
        ref mut ssn,
        ref mut license_number,
        ref mut passport_number,
        ..
    } = identity_entry.data
    {
        *ssn = Some("123-45-6789".into());
        *license_number = Some("D7654321".into());
        *passport_number = Some("P7654321".into());
    }
    replace_entry(&client, identity_entry).await?;

    ack(
        &client,
        Action::AddSshKey {
            name: "Copy SSH".into(),
            private_key:
                "-----BEGIN OPENSSH PRIVATE KEY-----\nFAKE\n-----END OPENSSH PRIVATE KEY-----"
                    .to_string()
                    .into(),
            public_key: Some("ssh-ed25519 AAAAC3Nza test@host".into()),
            notes: secret("ssh notes"),
            fields: vec![hidden.clone()],
        },
    )
    .await?;
    client.send(Action::Sync).await?;

    let bank_created = match client
        .send(Action::AddBankAccount {
            name: "Copy Bank".into(),
            bank_name: Some("First Bank".into()),
            name_on_account: Some("Alice Smith".into()),
            account_type: Some("checking".into()),
            account_number: secret("123456789"),
            routing_number: secret("021000021"),
            branch_number: Some("044".into()),
            pin: secret("4321"),
            swift_code: Some("CHASUS33".into()),
            iban: Some("DE89370400440532013000".into()),
            bank_contact_phone: Some("+1-555-0100".into()),
            notes: secret("bank notes"),
            fields: Vec::new(),
        })
        .await?
    {
        Response::Ack => true,
        other => {
            eprintln!("[copy-buttons] server rejected BankAccount ({other:?}); skipping new types");
            false
        }
    };
    client.send(Action::Sync).await?;

    let dl_created = bank_created
        && match client
            .send(Action::AddDriversLicense {
                name: "Copy License".into(),
                first_name: Some("Alice".into()),
                middle_name: None,
                last_name: Some("Smith".into()),
                date_of_birth: Some("1990-01-01".into()),
                license_number: secret("D1234567"),
                issuing_country: Some("US".into()),
                issuing_state: Some("CA".into()),
                issue_date: Some("2019-01-01".into()),
                expiration_date: Some("2029-01-01".into()),
                issuing_authority: Some("DMV".into()),
                license_class: Some("C".into()),
                notes: secret("license notes"),
                fields: Vec::new(),
            })
            .await?
        {
            Response::Ack => true,
            other => {
                eprintln!("[copy-buttons] server rejected DriversLicense ({other:?})");
                false
            }
        };
    client.send(Action::Sync).await?;

    let passport_created = bank_created
        && match client
            .send(Action::AddPassport {
                name: "Copy Passport".into(),
                surname: Some("Smith".into()),
                given_name: Some("Alice".into()),
                date_of_birth: Some("1990-01-01".into()),
                sex: Some("F".into()),
                birth_place: Some("Springfield".into()),
                nationality: Some("US".into()),
                issuing_country: Some("US".into()),
                passport_number: secret("P1234567"),
                passport_type: Some("P".into()),
                national_identification_number: Some("NIN-123".into()),
                issuing_authority: Some("DOS".into()),
                issue_date: Some("2019-01-01".into()),
                expiration_date: Some("2029-01-01".into()),
                notes: secret("passport notes"),
                fields: Vec::new(),
            })
            .await?
        {
            Response::Ack => true,
            other => {
                eprintln!("[copy-buttons] server rejected Passport ({other:?})");
                false
            }
        };
    client.send(Action::Sync).await?;

    // ── Login ────────────────────────────────────────────────────────────────
    let id = sidebar_id(&client, "Copy Login").await?;
    let redacted = meta(&client, &id).await?;
    expect_secret_copy(&client, &id, &redacted, "Password", "login-pass").await?;
    expect_secret_copy(&client, &id, &redacted, "Notes", "login notes").await?;
    expect_secret_copy(&client, &id, &redacted, "Hidden Note", "hidden-value").await?;
    // TOTP is a live code, never the seed: assert the shape, not the value.
    let code = copy_field(&client, &id, "TOTP").await?;
    assert_eq!(
        code.len(),
        6,
        "TOTP copy must return a 6-digit code: {code:?}"
    );
    assert!(code.chars().all(|c| c.is_ascii_digit()), "code: {code:?}");
    // Non-secret rows copy the value already present in the meta read.
    if let EntryData::Login {
        username: Some(u), ..
    } = &redacted.data
    {
        assert_eq!(u, "alice");
    } else {
        anyhow::bail!("meta login lost its username");
    }
    // A text custom field is copied directly, not fetched.
    assert_eq!(
        field_plaintext(&redacted, "Comment").as_deref(),
        Some("plain-value")
    );

    // ── Card ─────────────────────────────────────────────────────────────────
    let id = sidebar_id(&client, "Copy Card").await?;
    let redacted = meta(&client, &id).await?;
    expect_secret_copy(&client, &id, &redacted, "Card Number", "4111111111111111").await?;
    expect_secret_copy(&client, &id, &redacted, "Security Code", "321").await?;
    expect_secret_copy(&client, &id, &redacted, "Notes", "card notes").await?;
    match &redacted.data {
        EntryData::Card {
            cardholder_name: Some(name),
            ..
        } => assert_eq!(name, "Alice Smith"),
        _ => anyhow::bail!("meta card lost its cardholder"),
    }

    // ── Identity ─────────────────────────────────────────────────────────────
    let id = sidebar_id(&client, "Copy Identity").await?;
    let redacted = meta(&client, &id).await?;
    expect_secret_copy(&client, &id, &redacted, "SSN", "123-45-6789").await?;
    expect_secret_copy(&client, &id, &redacted, "License Number", "D7654321").await?;
    expect_secret_copy(&client, &id, &redacted, "Passport Number", "P7654321").await?;
    expect_secret_copy(&client, &id, &redacted, "Notes", "identity notes").await?;

    // ── SSH key: the reported bug ────────────────────────────────────────────
    let id = sidebar_id(&client, "Copy SSH").await?;
    let redacted = meta(&client, &id).await?;
    // Public key is not a secret: it survives redaction, is rendered as text,
    // and its button copies it directly.
    assert!(!field_is_secret("Public Key", false));
    assert_eq!(
        field_plaintext(&redacted, "Public Key").as_deref(),
        Some("ssh-ed25519 AAAAC3Nza test@host"),
        "public key must be copyable from the meta entry"
    );
    expect_secret_copy(
        &client,
        &id,
        &redacted,
        "Private Key",
        "-----BEGIN OPENSSH PRIVATE KEY-----\nFAKE\n-----END OPENSSH PRIVATE KEY-----",
    )
    .await?;
    expect_secret_copy(&client, &id, &redacted, "Notes", "ssh notes").await?;
    expect_secret_copy(&client, &id, &redacted, "Hidden Note", "hidden-value").await?;

    // ── Bank account, driver's license, passport ─────────────────────────────
    if bank_created {
        let id = sidebar_id(&client, "Copy Bank").await?;
        let redacted = meta(&client, &id).await?;
        expect_secret_copy(&client, &id, &redacted, "Account Number", "123456789").await?;
        expect_secret_copy(&client, &id, &redacted, "Routing Number", "021000021").await?;
        expect_secret_copy(&client, &id, &redacted, "Branch Number", "044").await?;
        expect_secret_copy(&client, &id, &redacted, "PIN", "4321").await?;
        expect_secret_copy(&client, &id, &redacted, "SWIFT Code", "CHASUS33").await?;
        expect_secret_copy(&client, &id, &redacted, "IBAN", "DE89370400440532013000").await?;
        expect_secret_copy(&client, &id, &redacted, "Notes", "bank notes").await?;
    }
    if dl_created {
        let id = sidebar_id(&client, "Copy License").await?;
        let redacted = meta(&client, &id).await?;
        expect_secret_copy(&client, &id, &redacted, "License Number", "D1234567").await?;
        expect_secret_copy(&client, &id, &redacted, "Notes", "license notes").await?;
    }
    if passport_created {
        let id = sidebar_id(&client, "Copy Passport").await?;
        let redacted = meta(&client, &id).await?;
        expect_secret_copy(&client, &id, &redacted, "Passport Number", "P1234567").await?;
        expect_secret_copy(&client, &id, &redacted, "Date of Birth", "1990-01-01").await?;
        expect_secret_copy(
            &client,
            &id,
            &redacted,
            "National Identification Number",
            "NIN-123",
        )
        .await?;
        expect_secret_copy(&client, &id, &redacted, "Notes", "passport notes").await?;
    }

    Ok(())
}
