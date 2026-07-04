/// Full CRUD + update tests for Card and Identity entry types, covering every field.
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

#[tokio::test]
async fn test_card_full_update() -> Result<()> {
    let env = setup_env().await?;
    std::env::set_var("COSMIC_BWARDEN_PROFILE", &env.profile);

    let email = "card-update@example.com";
    let password = "cardpassword456";
    register_user(&env.vault_url, email, password).await?;

    let client = AgentClient::new_with_socket(env.socket_path.clone());
    login(&client, &env.vault_url, email, password).await?;

    // 1. Create with all fields
    let res = client
        .send(Action::AddCard {
            name: "My Credit Card".to_string(),
            cardholder_name: Some("Alice Smith".to_string()),
            number: Some("4111111111111111".to_string().into()),
            brand: Some("Visa".to_string()),
            exp_month: Some("06".to_string()),
            exp_year: Some("2028".to_string()),
            code: Some("321".to_string().into()),
            notes: Some("Primary card".into()),
            fields: Vec::new(),
        })
        .await?;
    assert!(matches!(res, Response::Ack));

    client.send(Action::Sync).await?;

    // 2. Verify all fields
    let res = client
        .send(Action::GetSidebarEntries {
            query: None,
            entry_type: None,
            only_pinned: false,
        })
        .await?;
    let id = if let Response::SidebarEntries { entries } = res {
        entries
            .iter()
            .find(|e| e.name == "My Credit Card")
            .expect("card not found")
            .id
            .clone()
    } else {
        anyhow::bail!("Expected SidebarEntries");
    };

    let res = client
        .send(Action::GetEntry {
            id: id.clone(),
            password: None,
        })
        .await?;
    let mut entry = if let Response::Entry { entry } = res {
        if let EntryData::Card {
            cardholder_name,
            number,
            brand,
            exp_month,
            exp_year,
            code,
        } = &entry.data
        {
            assert_eq!(cardholder_name.as_deref(), Some("Alice Smith"));
            assert_eq!(
                number.as_ref().map(|n| n.expose()),
                Some("4111111111111111")
            );
            assert_eq!(brand.as_deref(), Some("Visa"));
            assert_eq!(exp_month.as_deref(), Some("06"));
            assert_eq!(exp_year.as_deref(), Some("2028"));
            assert_eq!(code.as_ref().map(|c| c.expose()), Some("321"));
        } else {
            anyhow::bail!("Expected Card data");
        }
        assert_eq!(
            entry.notes.as_ref().map(|n| n.expose()),
            Some("Primary card")
        );
        entry
    } else {
        anyhow::bail!("Expected Entry");
    };

    // 3. Update every field
    entry.name = "Updated Credit Card".to_string();
    if let EntryData::Card {
        ref mut cardholder_name,
        ref mut number,
        ref mut brand,
        ref mut exp_month,
        ref mut exp_year,
        ref mut code,
    } = entry.data
    {
        *cardholder_name = Some("Bob Jones".to_string());
        *number = Some("5500005555555559".to_string().into());
        *brand = Some("Mastercard".to_string());
        *exp_month = Some("12".to_string());
        *exp_year = Some("2030".to_string());
        *code = Some("456".to_string().into());
    }
    entry.notes = Some("Updated card notes".into());

    let res = client.send(Action::UpdateEntry { entry }).await?;
    assert!(matches!(res, Response::Ack), "UpdateEntry must return Ack");

    client.send(Action::Sync).await?;

    // 4. Verify all updates persisted
    let res = client
        .send(Action::GetEntry {
            id: id.clone(),
            password: None,
        })
        .await?;
    if let Response::Entry { entry } = res {
        assert_eq!(entry.name, "Updated Credit Card");
        if let EntryData::Card {
            cardholder_name,
            number,
            brand,
            exp_month,
            exp_year,
            code,
        } = &entry.data
        {
            assert_eq!(cardholder_name.as_deref(), Some("Bob Jones"));
            assert_eq!(
                number.as_ref().map(|n| n.expose()),
                Some("5500005555555559")
            );
            assert_eq!(brand.as_deref(), Some("Mastercard"));
            assert_eq!(exp_month.as_deref(), Some("12"));
            assert_eq!(exp_year.as_deref(), Some("2030"));
            assert_eq!(code.as_ref().map(|c| c.expose()), Some("456"));
        } else {
            anyhow::bail!("Expected Card data after update");
        }
        assert_eq!(
            entry.notes.as_ref().map(|n| n.expose()),
            Some("Updated card notes")
        );
    } else {
        anyhow::bail!("Expected Entry after update");
    }

    client.send(Action::DeleteEntry { id }).await?;
    client.send(Action::Sync).await?;

    Ok(())
}

#[tokio::test]
async fn test_identity_full_update() -> Result<()> {
    let env = setup_env().await?;
    std::env::set_var("COSMIC_BWARDEN_PROFILE", &env.profile);

    let email = "identity-full@example.com";
    let password = "identitypassword456";
    register_user(&env.vault_url, email, password).await?;

    let client = AgentClient::new_with_socket(env.socket_path.clone());
    login(&client, &env.vault_url, email, password).await?;

    // 1. Create with core fields
    let res = client
        .send(Action::AddIdentity {
            name: "My Identity".to_string(),
            first_name: Some("Jane".to_string()),
            last_name: Some("Doe".to_string()),
            address1: Some("123 Main St".to_string()),
            city: Some("Springfield".to_string()),
            state: Some("IL".to_string()),
            postal_code: Some("62701".to_string()),
            country: Some("US".to_string()),
            email: Some("jane.doe@example.com".to_string()),
            phone: Some("555-0100".to_string()),
            notes: Some("Personal identity".into()),
            fields: Vec::new(),
        })
        .await?;
    assert!(matches!(res, Response::Ack));

    client.send(Action::Sync).await?;

    // 2. Get and verify initial fields
    let res = client
        .send(Action::GetSidebarEntries {
            query: None,
            entry_type: None,
            only_pinned: false,
        })
        .await?;
    let id = if let Response::SidebarEntries { entries } = res {
        entries
            .iter()
            .find(|e| e.name == "My Identity")
            .expect("identity not found")
            .id
            .clone()
    } else {
        anyhow::bail!("Expected SidebarEntries");
    };

    let res = client
        .send(Action::GetEntry {
            id: id.clone(),
            password: None,
        })
        .await?;
    let mut entry = if let Response::Entry { entry } = res {
        if let EntryData::Identity {
            first_name,
            last_name,
            email,
            city,
            ..
        } = &entry.data
        {
            assert_eq!(first_name.as_deref(), Some("Jane"));
            assert_eq!(last_name.as_deref(), Some("Doe"));
            assert_eq!(email.as_deref(), Some("jane.doe@example.com"));
            assert_eq!(city.as_deref(), Some("Springfield"));
        } else {
            anyhow::bail!("Expected Identity data");
        }
        entry
    } else {
        anyhow::bail!("Expected Entry");
    };

    // 3. Update ALL identity fields including extended ones not set on create
    entry.name = "Updated Identity".to_string();
    if let EntryData::Identity {
        ref mut title,
        ref mut first_name,
        ref mut middle_name,
        ref mut last_name,
        ref mut address1,
        ref mut address2,
        ref mut address3,
        ref mut city,
        ref mut state,
        ref mut postal_code,
        ref mut country,
        ref mut phone,
        ref mut email,
        ref mut ssn,
        ref mut license_number,
        ref mut passport_number,
        ref mut username,
    } = entry.data
    {
        *title = Some("Dr.".to_string());
        *first_name = Some("John".to_string());
        *middle_name = Some("Michael".to_string());
        *last_name = Some("Smith".to_string());
        *address1 = Some("456 Elm St".to_string());
        *address2 = Some("Apt 7B".to_string());
        *address3 = Some("Building C".to_string());
        *city = Some("Chicago".to_string());
        *state = Some("IL".to_string());
        *postal_code = Some("60601".to_string());
        *country = Some("US".to_string());
        *phone = Some("555-0200".to_string());
        *email = Some("john.smith@example.com".to_string());
        *ssn = Some("123-45-6789".to_string());
        *license_number = Some("DL-999-888".to_string());
        *passport_number = Some("PP12345678".to_string());
        *username = Some("jsmith".to_string());
    }
    entry.notes = Some("Updated identity".into());

    let res = client.send(Action::UpdateEntry { entry }).await?;
    assert!(matches!(res, Response::Ack), "UpdateEntry must return Ack");

    client.send(Action::Sync).await?;

    // 4. Verify every field persisted correctly
    let res = client
        .send(Action::GetEntry {
            id: id.clone(),
            password: None,
        })
        .await?;
    if let Response::Entry { entry } = res {
        assert_eq!(entry.name, "Updated Identity");
        if let EntryData::Identity {
            title,
            first_name,
            middle_name,
            last_name,
            address1,
            address2,
            address3,
            city,
            state,
            postal_code,
            country,
            phone,
            email,
            ssn,
            license_number,
            passport_number,
            username,
        } = &entry.data
        {
            assert_eq!(title.as_deref(), Some("Dr."));
            assert_eq!(first_name.as_deref(), Some("John"));
            assert_eq!(middle_name.as_deref(), Some("Michael"));
            assert_eq!(last_name.as_deref(), Some("Smith"));
            assert_eq!(address1.as_deref(), Some("456 Elm St"));
            assert_eq!(address2.as_deref(), Some("Apt 7B"));
            assert_eq!(address3.as_deref(), Some("Building C"));
            assert_eq!(city.as_deref(), Some("Chicago"));
            assert_eq!(state.as_deref(), Some("IL"));
            assert_eq!(postal_code.as_deref(), Some("60601"));
            assert_eq!(country.as_deref(), Some("US"));
            assert_eq!(phone.as_deref(), Some("555-0200"));
            assert_eq!(email.as_deref(), Some("john.smith@example.com"));
            assert_eq!(ssn.as_deref(), Some("123-45-6789"));
            assert_eq!(license_number.as_deref(), Some("DL-999-888"));
            assert_eq!(passport_number.as_deref(), Some("PP12345678"));
            assert_eq!(username.as_deref(), Some("jsmith"));
        } else {
            anyhow::bail!("Expected Identity data after update");
        }
        assert_eq!(
            entry.notes.as_ref().map(|n| n.expose()),
            Some("Updated identity")
        );
    } else {
        anyhow::bail!("Expected Entry after update");
    }

    client.send(Action::DeleteEntry { id }).await?;
    client.send(Action::Sync).await?;

    Ok(())
}
