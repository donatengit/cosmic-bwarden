use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use cosmic_bwarden_core::agent_client::AgentClient;
use cosmic_bwarden_core::db::Secret;
use cosmic_bwarden_core::protocol::{Action, EntryType as ProtocolEntryType, Response};
use std::io::Write;

#[derive(Parser)]
#[command(
    author,
    version,
    about = "cosmic-bwarden: A secure COSMIC Bitwarden client",
    long_about = "A secure Bitwarden client for the COSMIC desktop, featuring a background agent and CLI.",
    after_help = "EXAMPLES:
  cosmic-bwarden-cli note ls
  cosmic-bwarden-cli get note \"My Note\"
  cosmic-bwarden-cli login (lists all logins)

Entry types (login, card, identity, note, sshkey) can be used as keywords
anywhere in the command line."
)]
struct Cli {
    /// Show secret fields (passwords, notes, hidden custom fields) in clear text
    // removed global show_secrets; use per-command flag
    /// Filter or specify entry type (Login, Card, Identity, Note, SshKey)
    #[arg(short, long, global = true, value_name = "TYPE")]
    type_: Option<CliEntryType>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
enum CliEntryType {
    Login,
    Card,
    Identity,
    Note,
    #[clap(name = "sshkey")]
    SshKey,
}

impl From<CliEntryType> for ProtocolEntryType {
    fn from(t: CliEntryType) -> Self {
        match t {
            CliEntryType::Login => ProtocolEntryType::Login,
            CliEntryType::Card => ProtocolEntryType::Card,
            CliEntryType::Identity => ProtocolEntryType::Identity,
            CliEntryType::Note => ProtocolEntryType::SecureNote,
            CliEntryType::SshKey => ProtocolEntryType::SshKey,
        }
    }
}

#[derive(Subcommand)]
enum Commands {
    /// Register a new account
    Register {
        /// Email address for the account
        email: String,
        /// Bitwarden server URL (e.g. https://vault.bitwarden.com)
        #[arg(short, long)]
        server: String,
        /// Master password
        #[arg(short, long)]
        password: Option<String>,
    },
    /// Log in to Bitwarden
    Login {
        /// Email address
        email: String,
        /// Bitwarden server URL (optional if already configured)
        #[arg(short, long)]
        server: Option<String>,
        /// Master password
        #[arg(short, long)]
        password: Option<String>,
    },
    /// Unlock the vault
    Unlock {
        /// Master password
        #[arg(short, long)]
        password: Option<String>,
    },
    /// Lock the vault
    Lock,
    /// Sync the vault
    Sync,
    /// List entries
    #[command(visible_alias = "ls")]
    List {
        /// Search query (ID, Name, or Username)
        query: Option<String>,
        /// Show only pinned entries
        #[arg(short, long)]
        pinned: bool,
    },
    /// Pin an entry for quick access
    Pin {
        /// Entry ID or Name
        id_or_name: String,
    },
    /// Unpin an entry
    Unpin {
        /// Entry ID or Name
        id_or_name: String,
    },
    /// Get details for an entry
    Get {
        /// Show secret fields
        #[arg(short = 'S', long = "show-secrets", action = clap::ArgAction::SetTrue)]
        show_secrets: bool,

        /// Entry ID or Name
        id_or_name: Option<String>,
        /// Show full entry details as JSON
        #[arg(short, long)]
        json: bool,
        /// Show all matching entries
        #[arg(short, long)]
        all: bool,
        /// Fields to output (comma separated or 'all')
        #[arg(short, long, default_value = "all")]
        fields: String,
    },
    /// Add a new entry
    #[command(
        long_about = "Add a new entry to the vault using key=value pairs.",
        after_help = "EXAMPLES:
  cosmic-bwarden-cli login add \"My Account\" username=user1
  cosmic-bwarden-cli add note \"My Note\" notes=\"Some text\"
  cosmic-bwarden-cli sshkey add \"Work Key\" private_key=X

ENTRY TYPE DETAILS:
  For login:  username=X, password=Y, notes=N
  For note:   any key=value will be added to the note body.
  For sshkey: private_key=X, public_key=Y, notes=N"
    )]
    Add {
        /// Name of the entry
        name: String,
        /// Key-value pairs (e.g., username=myuser password=mypass)
        #[arg(value_name = "KEY=VALUE")]
        args: Vec<String>,
        /// Custom fields (name=value)
        #[arg(short, long, value_name = "NAME=VALUE")]
        field: Vec<String>,
        /// Secret custom fields (name=value)
        #[arg(short = 's', long = "secret-field", value_name = "NAME=VALUE")]
        secret_field: Vec<String>,
    },
    /// Edit an existing entry
    Edit {
        /// Entry ID or Name
        id_or_name: String,
        /// Key-value pairs to update
        #[arg(value_name = "KEY=VALUE")]
        args: Vec<String>,
        /// Custom fields to add/update (name=value)
        #[arg(short, long, value_name = "NAME=VALUE")]
        field: Vec<String>,
        /// Secret custom fields to add/update (name=value)
        #[arg(short = 's', long = "secret-field", value_name = "NAME=VALUE")]
        secret_field: Vec<String>,
    },
    /// Add a new secure note (alias)
    #[command(hide = true)]
    AddNote {
        /// Name of the note
        name: String,
        /// Note content
        #[arg(short, long)]
        notes: Option<String>,
    },
    /// Add a new SSH key (alias)
    #[command(hide = true)]
    AddSshKey {
        /// Name of the entry
        name: String,
        /// Private key content
        #[arg(short, long)]
        private_key: Option<String>,
        /// Public key content
        #[arg(long)]
        public_key: Option<String>,
        /// Optional notes
        #[arg(short, long)]
        notes: Option<String>,
    },
    /// Check if vault is unlocked
    Unlocked,
    /// Stop the agent
    Quit,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = preprocess_args(std::env::args().collect());
    let cli = Cli::parse_from(args);
    let client = AgentClient::new();
    let entry_type = cli.type_.map(ProtocolEntryType::from);

    let res = run_command(&cli, &client, entry_type).await;

    if let Err(e) = &res {
        if e.to_string().contains("agent is locked") {
            let password = rpassword::prompt_password("Vault is locked. Master Password: ")
                .context("failed to read password")?;

            let unlock_res = client.send(Action::Unlock { password }).await?;
            handle_response(unlock_res)?;

            // Retry the command once
            return run_command(&cli, &client, entry_type).await;
        }
    }

    res
}

async fn run_command(
    cli: &Cli,
    client: &AgentClient,
    entry_type: Option<ProtocolEntryType>,
) -> Result<()> {
    match &cli.command {
        Commands::Register {
            email,
            server,
            password,
        } => {
            let password = if let Some(p) = password {
                p.clone()
            } else {
                rpassword::prompt_password("Master Password: ")
                    .context("failed to read password")?
            };

            let res = client
                .send(Action::Register {
                    email: email.clone(),
                    password,
                    server_url: server.clone(),
                })
                .await
                .context("failed to talk to agent")?;

            handle_response(res)?;
            println!("Account created successfully");
        }
        Commands::Login {
            email,
            server,
            password,
        } => {
            let password = if let Some(p) = password {
                p.clone()
            } else {
                rpassword::prompt_password("Master Password: ")
                    .context("failed to read password")?
            };

            let mut res = client
                .send(Action::Login {
                    email: email.clone(),
                    password: password.clone(),
                    server_url: server.clone(),
                    remember_me: true,
                    two_factor_token: None,
                    two_factor_provider: None,
                    two_factor_code: None,
                    device_verification_code: None,
                })
                .await
                .context("failed to talk to agent")?;

            if let Response::Error { message } = &res {
                if message == "new_device_verification_required" {
                    println!("A verification code has been sent to your email.");
                    let code = rpassword::prompt_password("Verification code: ")
                        .context("failed to read verification code")?;

                    res = client
                        .send(Action::Login {
                            email: email.clone(),
                            password: password.clone(),
                            server_url: server.clone(),
                            remember_me: true,
                            two_factor_token: None,
                            two_factor_provider: None,
                            two_factor_code: None,
                            device_verification_code: Some(code),
                        })
                        .await
                        .context("failed to talk to agent")?;
                }
            }

            if let Response::Error { message } = &res {
                if message.starts_with("two_factor_required:") {
                    let parts: Vec<&str> = message.split(':').collect();
                    let token = parts[1].to_string();
                    let providers_json = parts[2];
                    let providers: Vec<u32> = serde_json::from_str(providers_json)?;

                    println!("Two-factor authentication required.");
                    if providers.contains(&1) {
                        println!("1. Email");
                    }
                    let provider = if providers.contains(&1) {
                        1
                    } else {
                        providers[0]
                    };

                    let code = rpassword::prompt_password("Two-factor code: ")
                        .context("failed to read code")?;

                    res = client
                        .send(Action::Login {
                            email: email.clone(),
                            password: password.clone(),
                            server_url: server.clone(),
                            remember_me: true,
                            two_factor_token: Some(token),
                            two_factor_provider: Some(provider),
                            two_factor_code: Some(code),
                            device_verification_code: None,
                        })
                        .await
                        .context("failed to talk to agent")?;
                }
            }

            handle_response(res)?;
            println!("Logged in successfully");
        }
        Commands::Unlock { password } => {
            let password = if let Some(p) = password {
                p.clone()
            } else {
                rpassword::prompt_password("Master Password: ")
                    .context("failed to read password")?
            };

            let res = client.send(Action::Unlock { password }).await?;
            handle_response(res)?;
            println!("Unlocked successfully");
        }
        Commands::Lock => {
            let res = client.send(Action::Lock).await?;
            handle_response(res)?;
            println!("Locked successfully");
        }
        Commands::Sync => {
            let res = client.send(Action::Sync).await?;
            handle_response(res)?;
            println!("Synced successfully");
        }
        Commands::List { query, pinned } => {
            if *pinned {
                let res = client
                    .send(Action::GetTopFrequent {
                        limit: 100,
                        days: None,
                    })
                    .await?;
                match res {
                    Response::SidebarEntries { entries } => {
                        for entry in entries {
                            println!("{} | {} | Pinned", entry.id, entry.name);
                        }
                    }
                    _ => handle_response(res)?,
                }
                return Ok(());
            }

            let res = client
                .send(Action::GetEntries {
                    query: query.clone(),
                    entry_type,
                })
                .await?;
            match res {
                Response::Entries { entries } => {
                    for entry in entries {
                        let info = match &entry.data {
                            cosmic_bwarden_core::db::EntryData::Login { username, .. } => {
                                format!("Login | {}", username.as_deref().unwrap_or(""))
                            }
                            cosmic_bwarden_core::db::EntryData::Card { .. } => "Card".to_string(),
                            cosmic_bwarden_core::db::EntryData::Identity { .. } => {
                                "Identity".to_string()
                            }
                            cosmic_bwarden_core::db::EntryData::SecureNote => "Note".to_string(),
                            cosmic_bwarden_core::db::EntryData::SshKey { .. } => {
                                "SSH Key".to_string()
                            }
                        };
                        println!("{} | {} | {}", entry.id, entry.name, info);
                    }
                }
                _ => handle_response(res)?,
            }
        }
        Commands::Pin { id_or_name } => {
            let id = resolve_id(client, id_or_name, entry_type).await?;
            let res = client.send(Action::PinEntry { id }).await?;
            handle_response(res)?;
            println!("Entry pinned successfully");
        }
        Commands::Unpin { id_or_name } => {
            let id = resolve_id(client, id_or_name, entry_type).await?;
            let res = client.send(Action::UnpinEntry { id }).await?;
            handle_response(res)?;
            println!("Entry unpinned successfully");
        }
        Commands::Get {
            show_secrets,
            id_or_name,
            json,
            all,
            fields,
        } => {
            // show_secrets comes from the command flag
            let entries = if let Some(ref q) = id_or_name {
                let search_res = client
                    .send(Action::GetEntries {
                        query: Some(q.clone()),
                        entry_type,
                    })
                    .await?;
                if let Response::Entries { entries } = search_res {
                    if entries.is_empty() {
                        // Try getting by ID directly
                        let res = client
                            .send(Action::GetEntry {
                                id: q.clone(),
                                password: None,
                            })
                            .await?;
                        if let Response::Entry { entry } = res {
                            vec![entry]
                        } else {
                            Vec::new()
                        }
                    } else {
                        entries
                    }
                } else if let Response::Error { message } = &search_res {
                    if message == "agent is locked" {
                        return Err(anyhow::anyhow!("agent is locked"));
                    }
                    Vec::new()
                } else {
                    Vec::new()
                }
            } else {
                // No name provided, list all of type (or all if no type)
                let res = client
                    .send(Action::GetEntries {
                        query: None,
                        entry_type,
                    })
                    .await?;
                if let Response::Entries { entries } = res {
                    entries
                } else if let Response::Error { message } = &res {
                    if message == "agent is locked" {
                        return Err(anyhow::anyhow!("agent is locked"));
                    }
                    Vec::new()
                } else {
                    Vec::new()
                }
            };

            if entries.is_empty() {
                if let Some(q) = cli.type_ {
                    println!("No entries found for type {:?}", q);
                } else {
                    println!("No entries found.");
                }
                return Ok(());
            }

            let selected_entries = if *all || id_or_name.is_none() {
                if id_or_name.is_none() {
                    // Just list them
                    for entry in &entries {
                        let info = match &entry.data {
                            cosmic_bwarden_core::db::EntryData::Login { username, .. } => {
                                format!("Login | {}", username.as_deref().unwrap_or(""))
                            }
                            cosmic_bwarden_core::db::EntryData::Card { .. } => "Card".to_string(),
                            cosmic_bwarden_core::db::EntryData::Identity { .. } => {
                                "Identity".to_string()
                            }
                            cosmic_bwarden_core::db::EntryData::SecureNote => "Note".to_string(),
                            cosmic_bwarden_core::db::EntryData::SshKey { .. } => {
                                "SSH Key".to_string()
                            }
                        };
                        println!("{} | {} | {}", entry.id, entry.name, info);
                    }
                    return Ok(());
                }
                entries
            } else if entries.len() > 1 {
                println!("Multiple entries found:");
                for (i, entry) in entries.iter().enumerate() {
                    let user = match &entry.data {
                        cosmic_bwarden_core::db::EntryData::Login { username, .. } => {
                            username.as_deref().unwrap_or("")
                        }
                        _ => "",
                    };
                    println!("{}: {} | {} | {}", i + 1, entry.id, entry.name, user);
                }
                print!("Select entry (1-{}): ", entries.len());
                std::io::stdout().flush()?;
                let mut input = String::new();
                std::io::stdin().read_line(&mut input)?;
                let idx: usize = input.trim().parse::<usize>().context("invalid selection")?;
                if idx == 0 || idx > entries.len() {
                    anyhow::bail!("invalid selection");
                }
                vec![entries[idx - 1].clone()]
            } else {
                entries
            };

            for entry in selected_entries {
                let entry_res = if entry.master_password_reprompt() {
                    let password = rpassword::prompt_password("Master Password (reprompt): ")
                        .context("failed to read password")?;
                    client
                        .send(Action::GetEntry {
                            id: entry.id.clone(),
                            password: Some(password),
                        })
                        .await?
                } else {
                    // Fetch full entry details (including decrypted fields if any)
                    client
                        .send(Action::GetEntry {
                            id: entry.id.clone(),
                            password: None,
                        })
                        .await?
                };

                match entry_res {
                    Response::Entry { entry } => {
                        if *json {
                            println!("{}", serde_json::to_string_pretty(&entry)?);
                        } else {
                            output_entry(&entry, fields, *show_secrets)?;
                        }
                    }
                    _ => handle_response(entry_res)?,
                }
            }
        }
        Commands::Add {
            name,
            args,
            field,
            secret_field,
        } => {
            let t = cli.type_.unwrap_or(CliEntryType::Login);

            let mut kvs = std::collections::HashMap::new();
            for arg in args {
                if let Some((k, v)) = arg.split_once('=') {
                    kvs.insert(k.to_string(), v.into());
                }
            }

            let mut fields = Vec::new();
            for f in field {
                if let Some((name, value)) = f.split_once('=') {
                    fields.push(cosmic_bwarden_core::db::Field {
                        name: Some(name.to_string()),
                        value: Some(value.into()),
                        ty: Some(cosmic_bwarden_core::api::FieldType::String),
                        linked_id: None,
                    });
                }
            }
            for f in secret_field {
                if let Some((name, value)) = f.split_once('=') {
                    fields.push(cosmic_bwarden_core::db::Field {
                        name: Some(name.to_string()),
                        value: Some(value.into()),
                        ty: Some(cosmic_bwarden_core::api::FieldType::Hidden),
                        linked_id: None,
                    });
                }
            }

            let res = match t {
                CliEntryType::Login => {
                    let username: Option<String> =
                        kvs.get("username").map(|v: &Secret| v.expose().to_string());
                    let password = if let Some(p) = kvs.get("password") {
                        p.clone()
                    } else {
                        Secret::from(rpassword::prompt_password("Password: ")?)
                    };
                    let notes: Option<Secret> = kvs.get("notes").cloned();
                    client
                        .send(Action::AddEntry {
                            name: name.clone(),
                            entry_type: cosmic_bwarden_core::protocol::EntryType::Login,
                            username,
                            password: Some(password),
                            notes,
                            fields,
                        })
                        .await?
                }
                CliEntryType::Note => {
                    let notes = if let Some(n) = kvs.get("notes") {
                        n.clone()
                    } else if !kvs.is_empty() {
                        let mut keys: Vec<&String> = kvs.keys().collect();
                        keys.sort();
                        let mut notes_str = String::new();
                        for k in keys {
                            notes_str.push_str(&format!(
                                "{}={}\n",
                                k,
                                kvs.get(k).unwrap().expose()
                            ));
                        }
                        Secret::from(notes_str)
                    } else {
                        Secret::from(rpassword::prompt_password("Note Content: ")?)
                    };
                    client
                        .send(Action::AddSecureNote {
                            name: name.clone(),
                            notes,
                            fields,
                        })
                        .await?
                }
                CliEntryType::SshKey => {
                    let private_key = if let Some(pk) = kvs.get("private_key") {
                        pk.clone()
                    } else {
                        Secret::from(rpassword::prompt_password("Private Key: ")?)
                    };
                    let public_key = kvs.get("public_key").map(|v| v.expose().to_string());
                    let notes: Option<Secret> = kvs.get("notes").cloned();
                    client
                        .send(Action::AddSshKey {
                            name: name.clone(),
                            private_key,
                            public_key,
                            notes,
                            fields,
                        })
                        .await?
                }
                _ => anyhow::bail!("Unsupported entry type for Add"),
            };

            handle_response(res)?;
            println!("Entry added successfully");
        }
        Commands::Edit {
            id_or_name,
            args,
            field,
            secret_field,
        } => {
            let id = resolve_id(client, id_or_name, entry_type).await?;
            let res = client
                .send(Action::GetEntry {
                    id: id.clone(),
                    password: None,
                })
                .await?;
            let mut entry = match res {
                Response::Entry { entry } => entry,
                Response::Error { message } => anyhow::bail!("failed to fetch entry: {}", message),
                _ => anyhow::bail!("unexpected response from agent"),
            };

            for arg in args {
                if let Some((k, v)) = arg.split_once('=') {
                    match k {
                        "name" => entry.name = v.to_string(),
                        "username" => {
                            if let cosmic_bwarden_core::db::EntryData::Login {
                                ref mut username,
                                ..
                            } = entry.data
                            {
                                *username = Some(v.into());
                            }
                        }
                        "password" => {
                            if let cosmic_bwarden_core::db::EntryData::Login {
                                ref mut password,
                                ..
                            } = entry.data
                            {
                                *password = Some(Secret::from(v.to_string()));
                            }
                        }
                        "notes" => entry.notes = Some(v.into()),
                        _ => {
                            // Try updating custom fields if it matches a field name
                            if let Some(f) = entry
                                .fields
                                .iter_mut()
                                .find(|f| f.name.as_deref() == Some(k))
                            {
                                f.value = Some(v.into());
                            } else {
                                // Default to adding as string field if not found and not a built-in
                                entry.set_field(k, v, cosmic_bwarden_core::api::FieldType::String);
                            }
                        }
                    }
                }
            }

            for f in field {
                if let Some((name, value)) = f.split_once('=') {
                    entry.set_field(name, value, cosmic_bwarden_core::api::FieldType::String);
                }
            }
            for f in secret_field {
                if let Some((name, value)) = f.split_once('=') {
                    entry.set_field(name, value, cosmic_bwarden_core::api::FieldType::Hidden);
                }
            }

            let res = client.send(Action::UpdateEntry { entry }).await?;
            handle_response(res)?;
            println!("Entry updated successfully");
        }
        Commands::AddNote { name, notes } => {
            let notes = if let Some(n) = notes {
                n.clone()
            } else {
                rpassword::prompt_password("Note Content: ").context("failed to read notes")?
            };

            let res = client
                .send(Action::AddSecureNote {
                    name: name.clone(),
                    notes: notes.into(),
                    fields: Vec::new(),
                })
                .await?;

            handle_response(res)?;
            println!("Secure note added successfully");
        }
        Commands::AddSshKey {
            name,
            private_key,
            public_key,
            notes,
        } => {
            let private_key = if let Some(pk) = private_key {
                pk.clone()
            } else {
                rpassword::prompt_password("Private Key: ").context("failed to read private key")?
            };

            let res = client
                .send(Action::AddSshKey {
                    name: name.clone(),
                    private_key: private_key.into(),
                    public_key: public_key.clone(),
                    notes: notes.clone().map(Into::into),
                    fields: Vec::new(),
                })
                .await?;

            handle_response(res)?;
            println!("SSH key added successfully");
        }
        Commands::Unlocked => {
            let res = client.send(Action::Version).await;
            match res {
                Ok(_) => println!("Agent is running and connected."),
                Err(e) => anyhow::bail!("Agent is not running or not reachable: {}", e),
            }
        }
        Commands::Quit => {
            let res = client.send(Action::Quit).await?;
            handle_response(res)?;
            println!("Agent quit successfully");
        }
    }

    Ok(())
}

async fn resolve_id(
    client: &AgentClient,
    id_or_name: &str,
    entry_type: Option<ProtocolEntryType>,
) -> Result<String> {
    let search_res = client
        .send(Action::GetEntries {
            query: Some(id_or_name.to_string()),
            entry_type,
        })
        .await?;
    let entries = if let Response::Entries { entries } = search_res {
        if entries.is_empty() {
            // Try getting by ID directly
            let res = client
                .send(Action::GetEntry {
                    id: id_or_name.to_string(),
                    password: None,
                })
                .await?;
            if let Response::Entry { entry } = res {
                vec![entry]
            } else {
                Vec::new()
            }
        } else {
            entries
        }
    } else if let Response::Error { message } = &search_res {
        if message == "agent is locked" {
            return Err(anyhow::anyhow!("agent is locked"));
        }
        Vec::new()
    } else {
        Vec::new()
    };

    if entries.is_empty() {
        anyhow::bail!("Entry not found");
    } else if entries.len() > 1 {
        println!("Multiple entries found:");
        for (i, entry) in entries.iter().enumerate() {
            println!("{}: {} | {}", i + 1, entry.id, entry.name);
        }
        print!("Select entry (1-{}): ", entries.len());
        std::io::stdout().flush()?;
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        let idx: usize = input.trim().parse::<usize>().context("invalid selection")?;
        if idx == 0 || idx > entries.len() {
            anyhow::bail!("invalid selection");
        }
        Ok(entries[idx - 1].id.clone())
    } else {
        Ok(entries[0].id.clone())
    }
}

fn handle_response(res: Response) -> Result<()> {
    match res {
        Response::Error { message } => anyhow::bail!("Agent error: {}", message),
        _ => Ok(()),
    }
}

fn output_entry(
    entry: &cosmic_bwarden_core::db::Entry,
    fields_str: &str,
    show_secrets: bool,
) -> Result<()> {
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

fn preprocess_args(args: Vec<String>) -> Vec<String> {
    if args.len() < 2 {
        return args;
    }

    let type_keywords = ["login", "card", "identity", "note", "sshkey", "ssh-key"];
    let subcommands = [
        "register", "login", "unlock", "lock", "sync", "list", "ls", "get", "add", "unlocked",
        "quit", "pin", "unpin",
    ];

    let mut found_type = None;
    let mut found_idx = None;

    // Check if any arg is a type keyword
    for (i, arg) in args.iter().enumerate().skip(1) {
        let arg_low = arg.to_lowercase();
        if type_keywords.contains(&arg_low.as_str()) {
            // Avoid capturing if preceded by -t or --type
            if i > 1 && (args[i - 1] == "-t" || args[i - 1] == "--type") {
                continue;
            }

            // Ambiguity for 'login'
            if arg_low == "login" {
                let has_other_cmd = args
                    .iter()
                    .enumerate()
                    .skip(1)
                    .any(|(j, a)| i != j && subcommands.contains(&a.to_lowercase().as_str()));
                if has_other_cmd {
                    found_type = Some(arg_low);
                    found_idx = Some(i);
                    break;
                }
            } else {
                found_type = Some(arg_low);
                found_idx = Some(i);
                break;
            }
        }
    }

    if let (Some(t), Some(idx)) = (found_type, found_idx) {
        let mut new_args = Vec::new();
        new_args.push(args[0].clone());
        new_args.push("--type".into());
        new_args.push(t);
        let mut command_found = false;
        for (i, arg) in args.iter().enumerate().skip(1) {
            if i != idx {
                if subcommands.contains(&arg.to_lowercase().as_str()) {
                    command_found = true;
                }
                new_args.push(arg.clone());
            }
        }
        if !command_found {
            new_args.push("ls".into());
        }
        return new_args;
    }

    args
}
