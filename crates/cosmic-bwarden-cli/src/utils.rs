use anyhow::{Context, Result};
use cosmic_bwarden_core::agent_client::AgentClient;
use cosmic_bwarden_core::db::Entry;
use cosmic_bwarden_core::protocol::{Action, EntryType as ProtocolEntryType, Response};
use std::io::Write;

pub async fn resolve_id(
    client: &AgentClient,
    id_or_name: &str,
    entry_type: Option<ProtocolEntryType>,
) -> Result<String> {
    let search_res = client
        .send(Action::GetEntries {
            query: Some(id_or_name.to_string()),
            entry_type,
            only_pinned: false,
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

/// Entries whose name matches `name` exactly (case-sensitive) and whose type
/// matches `entry_type`. Used by `add` to warn about / replace pre-existing
/// same-name entries — `GetEntries`'s own `query` match is a case-insensitive
/// substring search, so results are filtered down to exact matches here.
pub async fn find_same_name_entries(
    client: &AgentClient,
    name: &str,
    entry_type: ProtocolEntryType,
) -> Result<Vec<Entry>> {
    let res = client
        .send(Action::GetEntries {
            query: Some(name.to_string()),
            entry_type: Some(entry_type),
            only_pinned: false,
        })
        .await?;
    match res {
        Response::Entries { entries } => {
            Ok(entries.into_iter().filter(|e| e.name == name).collect())
        }
        Response::Error { message } => anyhow::bail!("Agent error: {}", message),
        _ => Ok(Vec::new()),
    }
}

pub fn preprocess_args(args: Vec<String>) -> Vec<String> {
    if args.len() < 2 {
        return args;
    }

    let type_keywords = ["login", "card", "identity", "note", "sshkey", "ssh-key"];
    let subcommands = [
        "register", "login", "logout", "unlock", "lock", "sync", "list", "ls", "get", "add",
        "unlocked", "quit", "pin", "unpin", "edit", "version", "generate",
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
