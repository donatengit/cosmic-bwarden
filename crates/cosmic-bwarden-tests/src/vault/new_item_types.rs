//! Full CRUD tests for the Bitwarden v2026.7.0 new item types — BankAccount
//! (6), DriversLicense (7), Passport (8) — plus a coexistence test exercising
//! every cipher type (1-8) in one vault.
//!
//! Vaultwarden only *emits* these types today; the write path rejects them
//! with `Invalid type` until PR #7478 (`pm-32009-new-item-types`) lands. The
//! first `Add*` of each test doubles as a capability probe: on a server that
//! rejects the type the test prints a skip notice and returns, exactly like
//! the swtpm-gated smoke tests. The moment the server supports the types the
//! tests start running with no code changes.
use crate::common::{register_user, setup_env};
use anyhow::Result;
use cosmic_bwarden_core::agent_client::AgentClient;
use cosmic_bwarden_core::db::EntryData;
use cosmic_bwarden_core::protocol::{Action, Response};

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

/// Login via the agent's own API client and probe whether the server accepts
/// new-item-type (6-8) writes: POST a minimal type-6 cipher directly and check
/// the response *body*. Returns `Ok(false)` with a printed notice when the
/// server rejects the type (Vaultwarden's write path is gated behind PR #7478,
/// `pm-32009-new-item-types`); the probe cipher is deleted again on success so
/// it can never pollute later assertions.
async fn server_accepts_new_item_types(
    vault_url: &str,
    email: &str,
    password: &str,
) -> Result<bool> {
    use cosmic_bwarden_core::api::{Client as ApiClient, KdfType};
    use cosmic_bwarden_core::identity::Identity;
    use cosmic_bwarden_core::locked;

    let password_locked = locked::Password::from_string(password);
    let identity = Identity::new(
        email,
        &password_locked,
        KdfType::Pbkdf2,
        600_000,
        None,
        None,
    )?;
    let client = ApiClient::new(vault_url, &format!("{vault_url}/identity"));
    let (access_token, _, _) = client
        .login(
            email,
            "new-item-types-probe",
            &identity.master_password_hash,
            None,
            None,
            None,
            None,
        )
        .await
        .map_err(|e| anyhow::anyhow!("probe login failed: {e}"))?;

    let http = reqwest::Client::new();
    let res = http
        .post(format!("{vault_url}/api/ciphers"))
        .bearer_auth(&access_token)
        .json(&serde_json::json!({ "type": 6, "name": "probe", "bankAccount": {} }))
        .send()
        .await?;

    if res.status().is_success() {
        // Remove the probe cipher so it can't skew entry counts later. Accept
        // both key casings: Vaultwarden echoes PascalCase ("Id"), other
        // servers may use camelCase.
        let created: serde_json::Value = res.json().await?;
        let id = created
            .get("Id")
            .or_else(|| created.get("id"))
            .and_then(|v| v.as_str());
        if let Some(id) = id {
            let _ = http
                .delete(format!("{vault_url}/api/ciphers/{id}"))
                .bearer_auth(&access_token)
                .send()
                .await;
        }
        return Ok(true);
    }

    let status = res.status();
    let body = res.text().await.unwrap_or_default();
    eprintln!(
        "[new-item-types] server rejected the type-6 probe ({status}: {body}) — skipping; \
         Vaultwarden PR #7478 (pm-32009-new-item-types) enables this"
    );
    Ok(false)
}

/// Asserts a new-type creation *succeeded*. The HTTP probe has already proven
/// the server accepts the type, so any agent-side error here is a real bug,
/// not a capability gap — fail loudly instead of skipping.
fn assert_created(res: &Response, type_name: &str) -> Result<()> {
    match res {
        Response::Ack => Ok(()),
        other => anyhow::bail!("unexpected response creating {type_name}: {other:?}"),
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
    let Response::SidebarEntries { entries } = res else {
        anyhow::bail!("Expected SidebarEntries");
    };
    entries
        .iter()
        .find(|e| e.name == name)
        .map(|e| e.id.clone())
        .ok_or_else(|| anyhow::anyhow!("{name} not found in sidebar"))
}

#[tokio::test]
async fn test_bank_account_full_crud() -> Result<()> {
    let env = setup_env().await?;
    std::env::set_var("COSMIC_BWARDEN_PROFILE", &env.profile);

    let email = "bank-crud@example.com";
    let password = "bankpassword123";
    register_user(&env.vault_url, email, password).await?;

    // Capability probe: skip (with a notice) until the server's write path
    // accepts new-item types (Vaultwarden PR #7478).
    if !server_accepts_new_item_types(&env.vault_url, email, password).await? {
        return Ok(());
    }

    let client = AgentClient::new_with_socket(env.socket_path.clone());
    login(&client, &env.vault_url, email, password).await?;

    // 1. Create.
    let res = client
        .send(Action::AddBankAccount {
            name: "My Checking".to_string(),
            bank_name: Some("First Bank".to_string()),
            name_on_account: Some("Alice Smith".to_string()),
            account_type: Some("checking".to_string()),
            account_number: Some("123456789".to_string().into()),
            routing_number: Some("021000021".to_string().into()),
            branch_number: Some("044".to_string()),
            pin: Some("4321".to_string().into()),
            swift_code: Some("CHASUS33".to_string()),
            iban: Some("DE89370400440532013000".to_string()),
            bank_contact_phone: Some("+1-555-0100".to_string()),
            notes: Some("Primary account".into()),
            fields: Vec::new(),
        })
        .await?;
    assert_created(&res, "BankAccount")?;

    client.send(Action::Sync).await?;

    // 2. Verify every field round-trips decrypted.
    let id = sidebar_id(&client, "My Checking").await?;
    let res = client
        .send(Action::GetEntry {
            id: id.clone(),
            password: None,
        })
        .await?;
    let mut entry = if let Response::Entry { entry } = res {
        if let EntryData::BankAccount {
            bank_name,
            name_on_account,
            account_type,
            account_number,
            routing_number,
            branch_number,
            pin,
            swift_code,
            iban,
            bank_contact_phone,
        } = &entry.data
        {
            assert_eq!(bank_name.as_deref(), Some("First Bank"));
            assert_eq!(name_on_account.as_deref(), Some("Alice Smith"));
            assert_eq!(account_type.as_deref(), Some("checking"));
            assert_eq!(
                account_number.as_ref().map(|n| n.expose()),
                Some("123456789")
            );
            assert_eq!(
                routing_number.as_ref().map(|n| n.expose()),
                Some("021000021")
            );
            assert_eq!(branch_number.as_deref(), Some("044"));
            assert_eq!(pin.as_ref().map(|n| n.expose()), Some("4321"));
            assert_eq!(swift_code.as_deref(), Some("CHASUS33"));
            assert_eq!(iban.as_deref(), Some("DE89370400440532013000"));
            assert_eq!(bank_contact_phone.as_deref(), Some("+1-555-0100"));
        } else {
            anyhow::bail!("Expected BankAccount data");
        }
        assert_eq!(
            entry.notes.as_ref().map(|n| n.expose()),
            Some("Primary account")
        );
        entry
    } else {
        anyhow::bail!("Expected Entry");
    };

    // 3. Update every field.
    entry.name = "Updated Checking".to_string();
    if let EntryData::BankAccount {
        ref mut bank_name,
        ref mut name_on_account,
        ref mut account_type,
        ref mut account_number,
        ref mut routing_number,
        ref mut branch_number,
        ref mut pin,
        ref mut swift_code,
        ref mut iban,
        ref mut bank_contact_phone,
    } = entry.data
    {
        *bank_name = Some("Second Bank".to_string());
        *name_on_account = Some("Bob Jones".to_string());
        *account_type = Some("savings".to_string());
        *account_number = Some("987654321".to_string().into());
        *routing_number = Some("011000015".to_string().into());
        *branch_number = Some("099".to_string());
        *pin = Some("1234".to_string().into());
        *swift_code = Some("BOFAUS3N".to_string());
        *iban = Some("FR1420041010050500013M02606".to_string());
        *bank_contact_phone = Some("+1-555-0199".to_string());
    }
    entry.notes = Some("Updated account notes".into());

    let res = client.send(Action::UpdateEntry { entry }).await?;
    assert!(matches!(res, Response::Ack), "UpdateEntry must return Ack");

    client.send(Action::Sync).await?;

    // 4. Verify all updates persisted.
    let res = client
        .send(Action::GetEntry {
            id: id.clone(),
            password: None,
        })
        .await?;
    if let Response::Entry { entry } = res {
        assert_eq!(entry.name, "Updated Checking");
        if let EntryData::BankAccount {
            bank_name,
            name_on_account,
            account_type,
            account_number,
            routing_number,
            branch_number,
            pin,
            swift_code,
            iban,
            bank_contact_phone,
        } = &entry.data
        {
            assert_eq!(bank_name.as_deref(), Some("Second Bank"));
            assert_eq!(name_on_account.as_deref(), Some("Bob Jones"));
            assert_eq!(account_type.as_deref(), Some("savings"));
            assert_eq!(
                account_number.as_ref().map(|n| n.expose()),
                Some("987654321")
            );
            assert_eq!(
                routing_number.as_ref().map(|n| n.expose()),
                Some("011000015")
            );
            assert_eq!(branch_number.as_deref(), Some("099"));
            assert_eq!(pin.as_ref().map(|n| n.expose()), Some("1234"));
            assert_eq!(swift_code.as_deref(), Some("BOFAUS3N"));
            assert_eq!(iban.as_deref(), Some("FR1420041010050500013M02606"));
            assert_eq!(bank_contact_phone.as_deref(), Some("+1-555-0199"));
        } else {
            anyhow::bail!("Expected BankAccount data after update");
        }
        assert_eq!(
            entry.notes.as_ref().map(|n| n.expose()),
            Some("Updated account notes")
        );
    } else {
        anyhow::bail!("Expected Entry after update");
    }

    // 5. Delete.
    let res = client.send(Action::DeleteEntry { id }).await?;
    assert!(matches!(res, Response::Ack));
    Ok(())
}

#[tokio::test]
async fn test_drivers_license_full_crud() -> Result<()> {
    let env = setup_env().await?;
    std::env::set_var("COSMIC_BWARDEN_PROFILE", &env.profile);

    let email = "dl-crud@example.com";
    let password = "dlpassword123";
    register_user(&env.vault_url, email, password).await?;

    // Capability probe: skip (with a notice) until the server's write path
    // accepts new-item types (Vaultwarden PR #7478).
    if !server_accepts_new_item_types(&env.vault_url, email, password).await? {
        return Ok(());
    }

    let client = AgentClient::new_with_socket(env.socket_path.clone());
    login(&client, &env.vault_url, email, password).await?;

    let res = client
        .send(Action::AddDriversLicense {
            name: "My License".to_string(),
            first_name: Some("Alice".to_string()),
            middle_name: Some("Q".to_string()),
            last_name: Some("Smith".to_string()),
            date_of_birth: Some("1990-01-01".to_string()),
            license_number: Some("D1234567".to_string().into()),
            issuing_country: Some("US".to_string()),
            issuing_state: Some("CA".to_string()),
            issue_date: Some("2019-01-01".to_string()),
            expiration_date: Some("2029-01-01".to_string()),
            issuing_authority: Some("DMV".to_string()),
            license_class: Some("C".to_string()),
            notes: Some("License notes".into()),
            fields: Vec::new(),
        })
        .await?;
    assert_created(&res, "DriversLicense")?;

    client.send(Action::Sync).await?;

    let id = sidebar_id(&client, "My License").await?;
    let res = client
        .send(Action::GetEntry {
            id: id.clone(),
            password: None,
        })
        .await?;
    let mut entry = if let Response::Entry { entry } = res {
        if let EntryData::DriversLicense {
            first_name,
            middle_name,
            last_name,
            date_of_birth,
            license_number,
            issuing_country,
            issuing_state,
            issue_date,
            expiration_date,
            issuing_authority,
            license_class,
        } = &entry.data
        {
            assert_eq!(first_name.as_deref(), Some("Alice"));
            assert_eq!(middle_name.as_deref(), Some("Q"));
            assert_eq!(last_name.as_deref(), Some("Smith"));
            assert_eq!(date_of_birth.as_deref(), Some("1990-01-01"));
            assert_eq!(
                license_number.as_ref().map(|n| n.expose()),
                Some("D1234567")
            );
            assert_eq!(issuing_country.as_deref(), Some("US"));
            assert_eq!(issuing_state.as_deref(), Some("CA"));
            assert_eq!(issue_date.as_deref(), Some("2019-01-01"));
            assert_eq!(expiration_date.as_deref(), Some("2029-01-01"));
            assert_eq!(issuing_authority.as_deref(), Some("DMV"));
            assert_eq!(license_class.as_deref(), Some("C"));
        } else {
            anyhow::bail!("Expected DriversLicense data");
        }
        entry
    } else {
        anyhow::bail!("Expected Entry");
    };

    // Update the license number and name.
    entry.name = "Updated License".to_string();
    if let EntryData::DriversLicense {
        ref mut first_name,
        ref mut license_number,
        ref mut expiration_date,
        ..
    } = entry.data
    {
        *first_name = Some("Alicia".to_string());
        *license_number = Some("D7654321".to_string().into());
        *expiration_date = Some("2030-01-01".to_string());
    }

    let res = client.send(Action::UpdateEntry { entry }).await?;
    assert!(matches!(res, Response::Ack));
    client.send(Action::Sync).await?;

    let res = client
        .send(Action::GetEntry {
            id: id.clone(),
            password: None,
        })
        .await?;
    if let Response::Entry { entry } = res {
        assert_eq!(entry.name, "Updated License");
        if let EntryData::DriversLicense {
            first_name,
            license_number,
            expiration_date,
            ..
        } = &entry.data
        {
            assert_eq!(first_name.as_deref(), Some("Alicia"));
            assert_eq!(
                license_number.as_ref().map(|n| n.expose()),
                Some("D7654321")
            );
            assert_eq!(expiration_date.as_deref(), Some("2030-01-01"));
        } else {
            anyhow::bail!("Expected DriversLicense data after update");
        }
    }

    let res = client.send(Action::DeleteEntry { id }).await?;
    assert!(matches!(res, Response::Ack));
    Ok(())
}

#[tokio::test]
async fn test_passport_full_crud() -> Result<()> {
    let env = setup_env().await?;
    std::env::set_var("COSMIC_BWARDEN_PROFILE", &env.profile);

    let email = "passport-crud@example.com";
    let password = "passportpassword123";
    register_user(&env.vault_url, email, password).await?;

    // Capability probe: skip (with a notice) until the server's write path
    // accepts new-item types (Vaultwarden PR #7478).
    if !server_accepts_new_item_types(&env.vault_url, email, password).await? {
        return Ok(());
    }

    let client = AgentClient::new_with_socket(env.socket_path.clone());
    login(&client, &env.vault_url, email, password).await?;

    let res = client
        .send(Action::AddPassport {
            name: "My Passport".to_string(),
            surname: Some("Smith".to_string()),
            given_name: Some("Alice".to_string()),
            date_of_birth: Some("1990-01-01".to_string()),
            sex: Some("F".to_string()),
            birth_place: Some("Springfield".to_string()),
            nationality: Some("US".to_string()),
            issuing_country: Some("US".to_string()),
            passport_number: Some("P1234567".to_string().into()),
            passport_type: Some("P".to_string()),
            national_identification_number: Some("NIN-123".to_string()),
            issuing_authority: Some("DOS".to_string()),
            issue_date: Some("2019-01-01".to_string()),
            expiration_date: Some("2029-01-01".to_string()),
            notes: Some("Passport notes".into()),
            fields: Vec::new(),
        })
        .await?;
    assert_created(&res, "Passport")?;

    client.send(Action::Sync).await?;

    let id = sidebar_id(&client, "My Passport").await?;
    let res = client
        .send(Action::GetEntry {
            id: id.clone(),
            password: None,
        })
        .await?;
    if let Response::Entry { entry } = res {
        if let EntryData::Passport {
            surname,
            given_name,
            date_of_birth,
            sex,
            birth_place,
            nationality,
            issuing_country,
            passport_number,
            passport_type,
            national_identification_number,
            issuing_authority,
            issue_date,
            expiration_date,
        } = &entry.data
        {
            assert_eq!(surname.as_deref(), Some("Smith"));
            assert_eq!(given_name.as_deref(), Some("Alice"));
            assert_eq!(date_of_birth.as_deref(), Some("1990-01-01"));
            assert_eq!(sex.as_deref(), Some("F"));
            assert_eq!(birth_place.as_deref(), Some("Springfield"));
            assert_eq!(nationality.as_deref(), Some("US"));
            assert_eq!(issuing_country.as_deref(), Some("US"));
            assert_eq!(
                passport_number.as_ref().map(|n| n.expose()),
                Some("P1234567")
            );
            assert_eq!(passport_type.as_deref(), Some("P"));
            assert_eq!(national_identification_number.as_deref(), Some("NIN-123"));
            assert_eq!(issuing_authority.as_deref(), Some("DOS"));
            assert_eq!(issue_date.as_deref(), Some("2019-01-01"));
            assert_eq!(expiration_date.as_deref(), Some("2029-01-01"));
        } else {
            anyhow::bail!("Expected Passport data");
        }
    } else {
        anyhow::bail!("Expected Entry");
    }

    // Update the passport number and delete.
    let res = client
        .send(Action::GetEntry {
            id: id.clone(),
            password: None,
        })
        .await?;
    let mut entry = if let Response::Entry { entry } = res {
        entry
    } else {
        anyhow::bail!("Expected Entry for update");
    };
    if let EntryData::Passport {
        ref mut passport_number,
        ref mut expiration_date,
        ..
    } = entry.data
    {
        *passport_number = Some("P7654321".to_string().into());
        *expiration_date = Some("2030-01-01".to_string());
    }
    let res = client.send(Action::UpdateEntry { entry }).await?;
    assert!(matches!(res, Response::Ack));
    client.send(Action::Sync).await?;

    let res = client
        .send(Action::GetEntry {
            id: id.clone(),
            password: None,
        })
        .await?;
    if let Response::Entry { entry } = res {
        if let EntryData::Passport {
            passport_number,
            expiration_date,
            ..
        } = &entry.data
        {
            assert_eq!(
                passport_number.as_ref().map(|n| n.expose()),
                Some("P7654321")
            );
            assert_eq!(expiration_date.as_deref(), Some("2030-01-01"));
        } else {
            anyhow::bail!("Expected Passport data after update");
        }
    }

    let res = client.send(Action::DeleteEntry { id }).await?;
    assert!(matches!(res, Response::Ack));
    Ok(())
}

/// One vault containing every cipher type (1-8); the type filter must return
/// exactly the matching entry, and no filter must return all eight. Gated on
/// the server accepting the new types, like the CRUD tests above.
#[tokio::test]
async fn test_all_eight_types_coexist() -> Result<()> {
    let env = setup_env().await?;
    std::env::set_var("COSMIC_BWARDEN_PROFILE", &env.profile);

    let email = "all-types@example.com";
    let password = "alltypespassword123";
    register_user(&env.vault_url, email, password).await?;

    // Capability probe: skip (with a notice) until the server's write path
    // accepts new-item types (Vaultwarden PR #7478).
    if !server_accepts_new_item_types(&env.vault_url, email, password).await? {
        return Ok(());
    }

    let client = AgentClient::new_with_socket(env.socket_path.clone());
    login(&client, &env.vault_url, email, password).await?;

    macro_rules! add {
        ($pair:expr) => {
            assert_created(&client.send($pair.0).await?, $pair.1)?;
        };
    }

    add!((
        Action::AddEntry {
            name: "T1 Login".to_string(),
            entry_type: cosmic_bwarden_core::protocol::EntryType::Login,
            username: Some("u".to_string()),
            password: Some("p".to_string().into()),
            notes: None,
            fields: Vec::new(),
            totp: None,
            uris: Vec::new(),
        },
        "Login"
    ));
    add!((
        Action::AddSecureNote {
            name: "T2 Note".to_string(),
            notes: "n".to_string().into(),
            fields: Vec::new(),
        },
        "SecureNote"
    ));
    add!((
        Action::AddCard {
            name: "T3 Card".to_string(),
            cardholder_name: Some("Alice".to_string()),
            number: Some("4111111111111111".to_string().into()),
            brand: None,
            exp_month: None,
            exp_year: None,
            code: None,
            notes: None,
            fields: Vec::new(),
        },
        "Card"
    ));
    add!((
        Action::AddIdentity {
            name: "T4 Identity".to_string(),
            first_name: Some("Alice".to_string()),
            last_name: Some("Smith".to_string()),
            address1: None,
            city: None,
            state: None,
            postal_code: None,
            country: None,
            email: None,
            phone: None,
            notes: None,
            fields: Vec::new(),
        },
        "Identity"
    ));
    add!((
        Action::AddSshKey {
            name: "T5 SSH".to_string(),
            private_key: "ssh-rsa AAAA".to_string().into(),
            public_key: Some("ssh-rsa AAAA test@host".to_string()),
            notes: None,
            fields: Vec::new(),
        },
        "SshKey"
    ));
    add!((
        Action::AddBankAccount {
            name: "T6 Bank".to_string(),
            bank_name: None,
            name_on_account: None,
            account_type: None,
            account_number: Some("6".to_string().into()),
            routing_number: None,
            branch_number: None,
            pin: None,
            swift_code: None,
            iban: None,
            bank_contact_phone: None,
            notes: None,
            fields: Vec::new(),
        },
        "BankAccount"
    ));
    add!((
        Action::AddDriversLicense {
            name: "T7 DL".to_string(),
            first_name: None,
            middle_name: None,
            last_name: None,
            date_of_birth: None,
            license_number: Some("7".to_string().into()),
            issuing_country: None,
            issuing_state: None,
            issue_date: None,
            expiration_date: None,
            issuing_authority: None,
            license_class: None,
            notes: None,
            fields: Vec::new(),
        },
        "DriversLicense"
    ));
    add!((
        Action::AddPassport {
            name: "T8 Passport".to_string(),
            surname: None,
            given_name: None,
            date_of_birth: None,
            sex: None,
            birth_place: None,
            nationality: None,
            issuing_country: None,
            passport_number: Some("8".to_string().into()),
            passport_type: None,
            national_identification_number: None,
            issuing_authority: None,
            issue_date: None,
            expiration_date: None,
            notes: None,
            fields: Vec::new(),
        },
        "Passport"
    ));

    client.send(Action::Sync).await?;

    // No filter → all eight.
    let res = client
        .send(Action::GetSidebarEntries {
            query: None,
            entry_type: None,
            only_pinned: false,
            domain: None,
        })
        .await?;
    let Response::SidebarEntries { entries } = res else {
        anyhow::bail!("Expected SidebarEntries");
    };
    assert_eq!(entries.len(), 8, "all eight types must be listed");

    // Each type filter → exactly the matching entry.
    let cases = [
        ("T1 Login", cosmic_bwarden_core::protocol::EntryType::Login),
        ("T3 Card", cosmic_bwarden_core::protocol::EntryType::Card),
        (
            "T4 Identity",
            cosmic_bwarden_core::protocol::EntryType::Identity,
        ),
        (
            "T2 Note",
            cosmic_bwarden_core::protocol::EntryType::SecureNote,
        ),
        ("T5 SSH", cosmic_bwarden_core::protocol::EntryType::SshKey),
        (
            "T6 Bank",
            cosmic_bwarden_core::protocol::EntryType::BankAccount,
        ),
        (
            "T7 DL",
            cosmic_bwarden_core::protocol::EntryType::DriversLicense,
        ),
        (
            "T8 Passport",
            cosmic_bwarden_core::protocol::EntryType::Passport,
        ),
    ];
    for (name, ty) in cases {
        let res = client
            .send(Action::GetSidebarEntries {
                query: None,
                entry_type: Some(ty),
                only_pinned: false,
                domain: None,
            })
            .await?;
        let Response::SidebarEntries { entries } = res else {
            anyhow::bail!("Expected SidebarEntries for {name}");
        };
        assert_eq!(entries.len(), 1, "filter for {name} must match exactly one");
        assert_eq!(entries[0].name, name);
        assert_eq!(entries[0].entry_type, ty);
    }
    Ok(())
}
