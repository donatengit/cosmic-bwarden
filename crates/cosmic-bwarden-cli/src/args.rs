use clap::{Parser, Subcommand, ValueEnum};
use cosmic_bwarden_core::protocol::EntryType as ProtocolEntryType;

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
pub struct Cli {
    /// Path to the configuration file. Overrides default and environment.
    #[arg(long, global = true, env = "COSMIC_BWARDEN_CONFIG")]
    pub config: Option<std::path::PathBuf>,

    /// Path to the Unix socket for IPC. Overrides config, default and environment.
    #[arg(long, global = true, env = "COSMIC_BWARDEN_SOCKET")]
    pub socket: Option<std::path::PathBuf>,

    /// Filter or specify entry type (Login, Card, Identity, Note, SshKey)
    #[arg(short, long, global = true, value_name = "TYPE")]
    pub type_: Option<CliEntryType>,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum CliEntryType {
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
pub enum Commands {
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
