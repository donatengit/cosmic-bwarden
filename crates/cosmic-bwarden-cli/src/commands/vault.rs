use crate::args::{Cli, Commands};
use crate::output::{handle_response, output_entry};
use crate::utils::{find_same_name_entries, resolve_id};
use anyhow::Result;
use cosmic_bwarden_core::agent_client::AgentClient;
use cosmic_bwarden_core::db::{Field, Secret};
use cosmic_bwarden_core::protocol::{Action, EntryType as ProtocolEntryType, Response};

fn entry_type_label(t: ProtocolEntryType) -> &'static str {
    match t {
        ProtocolEntryType::Login => "login",
        ProtocolEntryType::Card => "card",
        ProtocolEntryType::Identity => "identity",
        ProtocolEntryType::SecureNote => "note",
        ProtocolEntryType::SshKey => "sshkey",
    }
}

pub async fn handle_command(
    cli: &Cli,
    client: &AgentClient,
    entry_type: Option<ProtocolEntryType>,
) -> Result<()> {
    match &cli.command {
        Commands::Sync => {
            let res = client.send(Action::Sync).await?;
            handle_response(res)?;
            println!("Synced successfully");
        }
        Commands::List { query, pinned } => {
            if *pinned {
                let res = client
                    .send(Action::GetSidebarEntries {
                        query: query.clone(),
                        entry_type,
                        only_pinned: true,
                        domain: None,
                    })
                    .await?;
                if let Response::SidebarEntries { entries } = res {
                    for entry in entries {
                        let info = entry
                            .username
                            .as_deref()
                            .map(|u| format!(" ({})", u))
                            .unwrap_or_default();
                        println!("{} | {}{}", entry.id, entry.name, info);
                    }
                } else {
                    handle_response(res)?;
                }
            } else {
                let res = client
                    .send(Action::GetEntries {
                        query: query.clone(),
                        entry_type,
                        only_pinned: false,
                    })
                    .await?;
                if let Response::Entries { entries } = res {
                    for entry in entries {
                        let info = match &entry.data {
                            cosmic_bwarden_core::db::EntryData::Login {
                                username: Some(u), ..
                            } => format!(" ({})", u),
                            _ => String::new(),
                        };
                        println!("{} | {}{}", entry.id, entry.name, info);
                    }
                } else {
                    handle_response(res)?;
                }
            }
        }
        Commands::Pin { id_or_name } => {
            let id = resolve_id(client, id_or_name, entry_type).await?;
            let res = client.send(Action::PinEntry { id }).await?;
            handle_response(res)?;
            println!("Pinned successfully");
        }
        Commands::Unpin { id_or_name } => {
            let id = resolve_id(client, id_or_name, entry_type).await?;
            let res = client.send(Action::UnpinEntry { id }).await?;
            handle_response(res)?;
            println!("Unpinned successfully");
        }
        Commands::Get {
            id_or_name,
            all,
            fields,
            show_secrets,
        } => {
            if let Some(id_or_name) = id_or_name {
                if *all {
                    let res = client
                        .send(Action::GetEntries {
                            query: Some(id_or_name.clone()),
                            entry_type,
                            only_pinned: false,
                        })
                        .await?;
                    if let Response::Entries { mut entries } = res {
                        if entries.is_empty() {
                            anyhow::bail!("Entry not found");
                        }
                        for entry in &mut entries {
                            let entry = if *show_secrets {
                                let res = client
                                    .send(Action::GetEntry {
                                        id: entry.id.clone(),
                                        password: None,
                                    })
                                    .await?;
                                if let Response::Entry { entry } = res {
                                    entry
                                } else {
                                    handle_response(res)?;
                                    unreachable!()
                                }
                            } else {
                                entry.clone()
                            };

                            output_entry(&entry, fields, *show_secrets)?;
                            println!("---");
                        }
                    } else {
                        handle_response(res)?;
                    }
                } else {
                    let id = resolve_id(client, id_or_name, entry_type).await?;
                    let res = client.send(Action::GetEntry { id, password: None }).await?;

                    let entry = match res {
                        Response::Entry { entry } => entry,
                        _ => {
                            handle_response(res)?;
                            unreachable!()
                        }
                    };

                    output_entry(&entry, fields, *show_secrets)?;
                }
            } else {
                // List with current filters if no ID provided
                let res = client
                    .send(Action::GetEntries {
                        query: None,
                        entry_type,
                        only_pinned: false,
                    })
                    .await?;
                if let Response::Entries { entries } = res {
                    for entry in entries {
                        let user = match &entry.data {
                            cosmic_bwarden_core::db::EntryData::Login {
                                username: Some(u), ..
                            } => format!(" ({})", u),
                            _ => String::new(),
                        };
                        println!("{} | {}{}", entry.id, entry.name, user);
                    }
                } else {
                    handle_response(res)?;
                }
            }
        }
        Commands::Add {
            name,
            args,
            field,
            secret_field,
            stdin,
            replace,
        } => {
            let t = entry_type.unwrap_or(ProtocolEntryType::Login);

            let duplicates = find_same_name_entries(client, name, t).await?;
            if *replace {
                for dup in &duplicates {
                    let res = client
                        .send(Action::DeleteEntry { id: dup.id.clone() })
                        .await?;
                    handle_response(res)?;
                    eprintln!("Replaced existing entry {} (\"{}\")", dup.id, dup.name);
                }
            } else {
                for dup in &duplicates {
                    eprintln!(
                        "Warning: a {} entry named \"{}\" already exists (id {}). Adding a new entry anyway.\n  Use --replace to replace it instead, or run: cosmic-bwarden-cli edit {} --delete",
                        entry_type_label(t),
                        dup.name,
                        dup.id,
                        dup.id
                    );
                }
            }

            let mut username = None;
            let mut password = None;
            let mut notes = None;
            let mut private_key = None;
            let mut public_key = None;

            for arg in args {
                if let Some((k, v)) = arg.split_once('=') {
                    match k {
                        "username" | "user" => username = Some(v.to_string()),
                        "password" | "pass" => password = Some(Secret::from(v.to_string())),
                        "notes" | "note" => {
                            if *stdin {
                                anyhow::bail!(
                                    "Cannot pass notes=... together with --stdin; choose one"
                                );
                            }
                            notes = Some(Secret::from(v.to_string()))
                        }
                        "private_key" | "private" => {
                            private_key = Some(Secret::from(v.to_string()))
                        }
                        "public_key" | "public" => public_key = Some(v.to_string()),
                        _ => (),
                    }
                }
            }

            if *stdin {
                use std::io::Read;
                let mut buf = String::new();
                std::io::stdin()
                    .read_to_string(&mut buf)
                    .map_err(|e| anyhow::anyhow!("Failed to read notes from stdin: {e}"))?;
                notes = Some(Secret::from(buf));
            }

            let mut fields = Vec::new();
            for f in field {
                if let Some((k, v)) = f.split_once('=') {
                    fields.push(Field {
                        name: Some(k.to_string()),
                        value: Some(Secret::from(v.to_string())),
                        ty: Some(cosmic_bwarden_core::api::FieldType::Text),
                        linked_id: None,
                    });
                }
            }
            for f in secret_field {
                if let Some((k, v)) = f.split_once('=') {
                    fields.push(Field {
                        name: Some(k.to_string()),
                        value: Some(Secret::from(v.to_string())),
                        ty: Some(cosmic_bwarden_core::api::FieldType::Hidden),
                        linked_id: None,
                    });
                }
            }

            let res = match t {
                ProtocolEntryType::Login => {
                    client
                        .send(Action::AddEntry {
                            name: name.clone(),
                            entry_type: t,
                            username,
                            password,
                            notes,
                            fields,
                            totp: None,
                            uris: Vec::new(),
                        })
                        .await?
                }
                ProtocolEntryType::SecureNote => {
                    client
                        .send(Action::AddSecureNote {
                            name: name.clone(),
                            notes: notes.unwrap_or_else(|| Secret::from("".to_string())),
                            fields,
                        })
                        .await?
                }
                ProtocolEntryType::SshKey => {
                    client
                        .send(Action::AddSshKey {
                            name: name.clone(),
                            private_key: private_key
                                .unwrap_or_else(|| Secret::from("".to_string())),
                            public_key,
                            notes,
                            fields,
                        })
                        .await?
                }
                _ => anyhow::bail!("Unsupported entry type for 'add'"),
            };
            handle_response(res)?;
            println!("Entry added successfully");
        }
        Commands::Edit {
            id_or_name,
            args,
            field,
            secret_field,
            stdin,
            delete,
        } => {
            let id = resolve_id(client, id_or_name, entry_type).await?;

            if *delete {
                if !args.is_empty() || !field.is_empty() || !secret_field.is_empty() || *stdin {
                    anyhow::bail!(
                        "Cannot combine --delete with other edit arguments (name=value, --field, --secret-field, --stdin)"
                    );
                }
                let res = client.send(Action::DeleteEntry { id }).await?;
                handle_response(res)?;
                println!("Entry deleted successfully");
                return Ok(());
            }

            let entry_res = client
                .send(Action::GetEntry {
                    id: id.clone(),
                    password: None,
                })
                .await?;

            let mut entry = match entry_res {
                Response::Entry { entry } => entry,
                _ => {
                    handle_response(entry_res)?;
                    unreachable!()
                }
            };

            for arg in args {
                if let Some((k, v)) = arg.split_once('=') {
                    match k {
                        "name" => entry.name = v.to_string(),
                        "notes" | "note" => {
                            if *stdin {
                                anyhow::bail!(
                                    "Cannot pass notes=... together with --stdin; choose one"
                                );
                            }
                            entry.notes = Some(Secret::from(v.to_string()))
                        }
                        _ => match &mut entry.data {
                            cosmic_bwarden_core::db::EntryData::Login {
                                username,
                                password,
                                ..
                            } => match k {
                                "username" | "user" => *username = Some(v.to_string()),
                                "password" | "pass" => {
                                    *password = Some(Secret::from(v.to_string()))
                                }
                                _ => (),
                            },
                            cosmic_bwarden_core::db::EntryData::SshKey {
                                private_key,
                                public_key,
                                ..
                            } => match k {
                                "private_key" | "private" => {
                                    *private_key = Some(Secret::from(v.to_string()))
                                }
                                "public_key" | "public" => *public_key = Some(v.to_string()),
                                _ => (),
                            },
                            _ => (),
                        },
                    }
                }
            }

            if *stdin {
                use std::io::Read;
                let mut buf = String::new();
                std::io::stdin()
                    .read_to_string(&mut buf)
                    .map_err(|e| anyhow::anyhow!("Failed to read notes from stdin: {e}"))?;
                entry.notes = Some(Secret::from(buf));
            }

            for f in field {
                if let Some((k, v)) = f.split_once('=') {
                    entry.set_field(k, v, cosmic_bwarden_core::api::FieldType::Text);
                }
            }
            for f in secret_field {
                if let Some((k, v)) = f.split_once('=') {
                    entry.set_field(k, v, cosmic_bwarden_core::api::FieldType::Hidden);
                }
            }

            let res = client.send(Action::UpdateEntry { entry }).await?;
            handle_response(res)?;
            println!("Entry updated successfully");
        }
        Commands::AddNote { name, notes } => {
            let res = client
                .send(Action::AddSecureNote {
                    name: name.clone(),
                    notes: Secret::from(notes.clone().unwrap_or_default()),
                    fields: Vec::new(),
                })
                .await?;
            handle_response(res)?;
            println!("Note added successfully");
        }
        Commands::AddSshKey {
            name,
            private_key,
            public_key,
            notes,
        } => {
            let res = client
                .send(Action::AddSshKey {
                    name: name.clone(),
                    private_key: Secret::from(private_key.clone().unwrap_or_default()),
                    public_key: public_key.clone(),
                    notes: notes.as_ref().map(|n| Secret::from(n.clone())),
                    fields: Vec::new(),
                })
                .await?;
            handle_response(res)?;
            println!("SSH key added successfully");
        }
        _ => unreachable!(),
    }
    Ok(())
}
