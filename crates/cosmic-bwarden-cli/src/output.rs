use anyhow::Result;
use cosmic_bwarden_core::db::Entry;
use cosmic_bwarden_core::protocol::Response;

pub fn handle_response(res: Response) -> Result<()> {
    match res {
        Response::Error { message } => anyhow::bail!("Agent error: {}", message),
        _ => Ok(()),
    }
}

pub fn output_entry(entry: &Entry, fields_str: &str, show_secrets: bool) -> Result<()> {
    let all_fields = fields_str == "all";
    let requested_fields: std::collections::HashSet<&str> = fields_str.split(',').collect();

    if all_fields || requested_fields.contains("name") {
        println!("Name: {}", entry.name);
    }
    if all_fields || requested_fields.contains("id") {
        println!("ID: {}", entry.id);
    }

    match &entry.data {
        cosmic_bwarden_core::db::EntryData::Login {
            username,
            password,
            ..
        } => {
            if all_fields || requested_fields.contains("type") {
                println!("Type: Login");
            }
            if all_fields || requested_fields.contains("username") {
                println!("Username: {}", username.as_deref().unwrap_or(""));
            }
            if all_fields || requested_fields.contains("password") {
                if let Some(p) = password {
                    if show_secrets {
                        println!("Password: {}", p.expose());
                    } else {
                        println!("Password: ********");
                    }
                }
            }
        }
        cosmic_bwarden_core::db::EntryData::Card {
            number,
            brand,
            exp_month,
            exp_year,
            code,
            ..
        } => {
            if all_fields || requested_fields.contains("type") {
                println!("Type: Card");
            }
            if all_fields || requested_fields.contains("number") {
                if let Some(n) = number {
                    if show_secrets {
                        println!("Number: {}", n.expose());
                    } else {
                        println!("Number: ********");
                    }
                }
            }
            if all_fields || requested_fields.contains("brand") {
                println!("Brand: {}", brand.as_deref().unwrap_or(""));
            }
            if all_fields || requested_fields.contains("expiry") {
                println!(
                    "Expiry: {}/{}",
                    exp_month.as_deref().unwrap_or(""),
                    exp_year.as_deref().unwrap_or("")
                );
            }
            if all_fields || requested_fields.contains("code") {
                if let Some(c) = code {
                    if show_secrets {
                        println!("Code: {}", c.expose());
                    } else {
                        println!("Code: ********");
                    }
                }
            }
        }
        cosmic_bwarden_core::db::EntryData::Identity {
            first_name,
            last_name,
            email,
            ..
        } => {
            if all_fields || requested_fields.contains("type") {
                println!("Type: Identity");
            }
            if all_fields || requested_fields.contains("name") {
                println!(
                    "Identity Name: {} {}",
                    first_name.as_deref().unwrap_or(""),
                    last_name.as_deref().unwrap_or("")
                );
            }
            if all_fields || requested_fields.contains("email") {
                println!("Email: {}", email.as_deref().unwrap_or(""));
            }
        }
        cosmic_bwarden_core::db::EntryData::SecureNote => {
            if all_fields || requested_fields.contains("type") {
                println!("Type: Secure Note");
            }
        }
        cosmic_bwarden_core::db::EntryData::SshKey {
            private_key,
            public_key,
            fingerprint,
        } => {
            if all_fields || requested_fields.contains("type") {
                println!("Type: SSH Key");
            }
            if all_fields || requested_fields.contains("private_key") {
                if let Some(pk) = private_key {
                    if show_secrets {
                        println!("Private Key:\n{}", pk.expose());
                    } else {
                        println!("Private Key: ********");
                    }
                }
            }
            if all_fields || requested_fields.contains("public_key") {
                if let Some(pubk) = public_key {
                    println!("Public Key: {}", pubk);
                }
            }
            if all_fields || requested_fields.contains("fingerprint") {
                if let Some(fp) = fingerprint {
                    println!("Fingerprint: {}", fp);
                }
            }
        }
    }

    // Show notes for CLI output if they are present and show_secrets is enabled
    if let Some(notes) = &entry.notes {
        if show_secrets {
            println!("Notes:\n{}", notes.expose());
        } else {
            println!("Notes: ********");
        }
    }

    for field in &entry.fields {
        if let (Some(name), Some(value)) = (&field.name, &field.value) {
            if all_fields || requested_fields.contains(name.as_str()) {
                let display_value = if field.ty == Some(cosmic_bwarden_core::api::FieldType::Hidden)
                    && !show_secrets
                {
                    "********".to_string()
                } else {
                    value.expose().to_string()
                };
                println!("{}: {}", name, display_value);
            }
        }
    }

    Ok(())
}
