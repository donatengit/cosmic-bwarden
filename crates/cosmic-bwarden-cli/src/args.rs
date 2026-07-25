use clap::{Parser, Subcommand, ValueEnum};
use cosmic_bwarden_core::protocol::EntryType as ProtocolEntryType;

#[derive(Parser)]
#[command(
    author,
    version = cosmic_bwarden_core::version(),
    about = "cosmic-bwarden: A secure COSMIC Bitwarden client",
    long_about = "A secure Bitwarden client for the COSMIC desktop, featuring a background agent and CLI.",
    after_help = "EXAMPLES:
  cosmic-bwarden-cli note ls
  cosmic-bwarden-cli get note \"My Note\"
  cosmic-bwarden-cli login (lists all logins)
  cosmic-bwarden-cli generate --length 20 --special --numbers

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
    #[command(after_help = "EXAMPLES:
  cosmic-bwarden-cli get \"My Account\" --show-secrets
  cosmic-bwarden-cli get \"AWS Creds\" --fields notes --show-secrets
  cosmic-bwarden-cli get \"AWS Creds\" --fields notes --show-secrets > credentials.yaml

`--fields notes` alone (no other field names) prints just the note body,
byte-for-byte, with no label \u{2014} pipe it straight to a file to restore
whatever was stored with `add --stdin` / `edit --stdin` (see `add --help`).")]
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
  cat credentials.yaml | cosmic-bwarden-cli add note \"AWS Creds\" --stdin
  cosmic-bwarden-cli add note \"AWS Creds\" --stdin < credentials.yaml
  cosmic-bwarden-cli add note \"AWS Creds\" --replace --stdin < credentials.yaml

  # ...later, restore the whole file from the note:
  cosmic-bwarden-cli get \"AWS Creds\" --fields notes --show-secrets > credentials.yaml

ENTRY TYPE DETAILS:
  For login:  username=X, password=Y, notes=N
  For note:   any key=value will be added to the note body.
  For sshkey: private_key=X, public_key=Y, notes=N

--stdin reads the entry's notes from standard input (via `cat file | ...`
or `... < file`) instead of a notes=/note= pair.
`get --fields notes --show-secrets` prints the note body back byte-for-byte
(no label), so the pair above round-trips a whole file through the vault.

Adding an entry never overwrites an existing one by name \u{2014} names are
not unique (Bitwarden allows e.g. two logins both called \"GitHub\"). If an
entry of the same name AND type already exists, `add` warns on stderr but
still creates a new entry, unless --replace is given, in which case the
existing entry (or entries) are deleted first."
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
        /// Read notes content from standard input instead of notes=/note=
        #[arg(long, action = clap::ArgAction::SetTrue)]
        stdin: bool,
        /// Delete any existing entry with the same name and type before adding
        #[arg(long, action = clap::ArgAction::SetTrue)]
        replace: bool,
    },
    /// Edit an existing entry
    #[command(after_help = "EXAMPLES:
  cosmic-bwarden-cli edit \"My Account\" password=newpass
  cat env_vars.sh | cosmic-bwarden-cli edit \"My Note\" --stdin
  cosmic-bwarden-cli edit \"My Note\" --stdin < env_vars.sh
  cosmic-bwarden-cli edit b837ec2d-1590-42cf-8a14-b48a0152fc9c --delete

  # ...later, restore the whole file from the note:
  cosmic-bwarden-cli get \"My Note\" --fields notes --show-secrets > env_vars.sh

--stdin reads the entry's notes from standard input (via `cat file | ...`
or `... < file`) instead of a notes=/note= pair. `get --fields notes
--show-secrets` prints it back byte-for-byte (no label) for the round trip
above. --delete removes the resolved entry instead of editing it (cannot be
combined with any other edit argument).")]
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
        /// Read notes content from standard input instead of notes=/note=
        #[arg(long, action = clap::ArgAction::SetTrue)]
        stdin: bool,
        /// Delete the resolved entry instead of editing it
        #[arg(long, action = clap::ArgAction::SetTrue)]
        delete: bool,
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
    /// Log out and clear local session data
    Logout,
    /// Check if vault is unlocked
    Unlocked,
    /// Stop the agent
    Quit,
    /// Show version information and check agent compatibility
    Version,
    /// Generate a password using (and updating) the shared generator settings
    #[command(
        long_about = "Generate a password using the device-wide generator settings shared \
with the desktop UI, applet, and browser extension.\n\n\
Passing no character-group/length flags reuses whatever was last saved \
(falling back to a sensible default on first use). Passing any one of \
-U/-l/-n/-s fully specifies this run's character groups (all four are \
independently on/off, not additive), and that becomes the new \"last used\" \
setting for every other surface.",
        after_help = "EXAMPLES:
  cosmic-bwarden-cli generate
  cosmic-bwarden-cli generate --length 20 --special --numbers
  cosmic-bwarden-cli generate --history"
    )]
    Generate {
        /// Include uppercase letters (A-Z)
        #[arg(short = 'U', long)]
        uppercase: bool,
        /// Include lowercase letters (a-z)
        #[arg(short = 'l', long)]
        lowercase: bool,
        /// Include numbers (0-9)
        #[arg(short = 'n', long)]
        numbers: bool,
        /// Include special characters
        #[arg(short = 's', long)]
        special: bool,
        /// Password length (8-32). Omit to reuse the last-saved length.
        #[arg(long, value_parser = clap::value_parser!(u8).range(8..=32))]
        length: Option<u8>,
        /// Show the last 7 days of generated passwords instead of generating a new one
        #[arg(long, conflicts_with_all = ["uppercase", "lowercase", "numbers", "special", "length"])]
        history: bool,
    },
}
