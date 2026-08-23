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

    // Raw mode: `--fields notes` alone (nothing else) prints just the note
    // body, byte-for-byte, with no "Notes:" label or other fields mixed in.
    // This is the restore half of the `add --stdin` round trip: `get NAME
    // --fields notes --show-secrets > file.yaml` reproduces exactly what
    // `cat file.yaml | add NAME --stdin` stored.
    if !all_fields && requested_fields.len() == 1 && requested_fields.contains("notes") {
        if let Some(notes) = &entry.notes {
            if show_secrets {
                use std::io::Write;
                std::io::stdout().write_all(notes.expose().as_bytes())?;
            } else {
                println!("********");
            }
        }
        return Ok(());
    }

    if all_fields || requested_fields.contains("name") {
        println!("Name: {}", entry.name);
    }
    if all_fields || requested_fields.contains("id") {
        println!("ID: {}", entry.id);
    }

    match &entry.data {
        cosmic_bwarden_core::db::EntryData::Login {
            username, password, ..
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
        cosmic_bwarden_core::db::EntryData::BankAccount {
            bank_name,
            name_on_account,
            account_number,
            routing_number,
            pin,
            swift_code,
            iban,
            ..
        } => {
            if all_fields || requested_fields.contains("type") {
                println!("Type: Bank Account");
            }
            if all_fields || requested_fields.contains("bank_name") {
                println!("Bank Name: {}", bank_name.as_deref().unwrap_or(""));
            }
            if all_fields || requested_fields.contains("name_on_account") {
                println!(
                    "Name on Account: {}",
                    name_on_account.as_deref().unwrap_or("")
                );
            }
            if all_fields || requested_fields.contains("account_number") {
                if let Some(n) = account_number {
                    if show_secrets {
                        println!("Account Number: {}", n.expose());
                    } else {
                        println!("Account Number: ********");
                    }
                }
            }
            if all_fields || requested_fields.contains("routing_number") {
                if let Some(n) = routing_number {
                    if show_secrets {
                        println!("Routing Number: {}", n.expose());
                    } else {
                        println!("Routing Number: ********");
                    }
                }
            }
            if all_fields || requested_fields.contains("pin") {
                if let Some(p) = pin {
                    if show_secrets {
                        println!("PIN: {}", p.expose());
                    } else {
                        println!("PIN: ********");
                    }
                }
            }
            if all_fields || requested_fields.contains("swift_code") {
                println!("SWIFT Code: {}", swift_code.as_deref().unwrap_or(""));
            }
            if all_fields || requested_fields.contains("iban") {
                println!("IBAN: {}", iban.as_deref().unwrap_or(""));
            }
        }
        cosmic_bwarden_core::db::EntryData::DriversLicense {
            first_name,
            last_name,
            date_of_birth,
            license_number,
            issuing_state,
            expiration_date,
            ..
        } => {
            if all_fields || requested_fields.contains("type") {
                println!("Type: Driver's License");
            }
            if all_fields || requested_fields.contains("name") {
                println!(
                    "Name: {} {}",
                    first_name.as_deref().unwrap_or(""),
                    last_name.as_deref().unwrap_or("")
                );
            }
            if all_fields || requested_fields.contains("date_of_birth") {
                println!("Date of Birth: {}", date_of_birth.as_deref().unwrap_or(""));
            }
            if all_fields || requested_fields.contains("license_number") {
                if let Some(n) = license_number {
                    if show_secrets {
                        println!("License Number: {}", n.expose());
                    } else {
                        println!("License Number: ********");
                    }
                }
            }
            if all_fields || requested_fields.contains("issuing_state") {
                println!("Issuing State: {}", issuing_state.as_deref().unwrap_or(""));
            }
            if all_fields || requested_fields.contains("expiration_date") {
                println!(
                    "Expiration Date: {}",
                    expiration_date.as_deref().unwrap_or("")
                );
            }
        }
        cosmic_bwarden_core::db::EntryData::Passport {
            surname,
            given_name,
            passport_number,
            issuing_country,
            expiration_date,
            ..
        } => {
            if all_fields || requested_fields.contains("type") {
                println!("Type: Passport");
            }
            if all_fields || requested_fields.contains("name") {
                println!(
                    "Name: {} {}",
                    given_name.as_deref().unwrap_or(""),
                    surname.as_deref().unwrap_or("")
                );
            }
            if all_fields || requested_fields.contains("passport_number") {
                if let Some(n) = passport_number {
                    if show_secrets {
                        println!("Passport Number: {}", n.expose());
                    } else {
                        println!("Passport Number: ********");
                    }
                }
            }
            if all_fields || requested_fields.contains("issuing_country") {
                println!(
                    "Issuing Country: {}",
                    issuing_country.as_deref().unwrap_or("")
                );
            }
            if all_fields || requested_fields.contains("expiration_date") {
                println!(
                    "Expiration Date: {}",
                    expiration_date.as_deref().unwrap_or("")
                );
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

    // Show notes for CLI output if they are present, requested, and show_secrets is enabled
    if all_fields || requested_fields.contains("notes") {
        if let Some(notes) = &entry.notes {
            if show_secrets {
                println!("Notes:\n{}", notes.expose());
            } else {
                println!("Notes: ********");
            }
        }
    }

    for field in &entry.fields {
        let Some(name) = field.name.as_deref() else { continue };
        if !all_fields && !requested_fields.contains(name) {
            continue;
        }
        let is_hidden = field.ty == Some(cosmic_bwarden_core::api::FieldType::Hidden);
        // A hidden field read via GetEntryMeta arrives with its value already
        // redacted (None) — still print the masked placeholder so the field's
        // existence is visible without ever pulling the secret into the
        // process (see fetch_get_action). Non-hidden fields with no value are
        // skipped, as before.
        if is_hidden && !show_secrets {
            println!("{}: ********", name);
        } else if let Some(value) = &field.value {
            println!("{}: {}", name, value.expose());
        }
    }

    Ok(())
}
