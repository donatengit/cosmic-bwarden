app-title = COSMIC BWarden
protocol-version-mismatch = Protocol version mismatch, please restart agent and/or this app

# Generic actions / nouns
login = Login
unlock = Unlock
vault = Vault
settings = Settings
lock = Lock
logout = Logout
exit = Exit
quit = Quit
save = Save
cancel = Cancel
delete = Delete
edit = Edit
add = Add
sync = Sync
verify = Verify
enable = Enable
search = Search...
loading = Loading...
error = Error
# { $error } is the underlying error message.
error-fmt = Error: { $error }

# Account / server fields
email = Email
password = Password
server = Server URL
server-label = Server
account = Account
bitwarden-cloud = Bitwarden Cloud
remember-email = Remember email
advanced = Advanced

# Login / setup
welcome-title = Welcome to COSMIC BWarden
welcome-body = Sign in to your Bitwarden vault.
verification-code = Verification Code
new-device-verification = New device verification required. Please check your email.
show-advanced = Show Advanced
hide-advanced = Hide Advanced
server-url-optional = Server URL (optional)

# Unlock (master password + PIN)
vault-locked = Vault Locked
enter-master-password-to-unlock = Enter your master password to unlock.
enter-pin-to-unlock = Enter your PIN to unlock.
master-password = Master Password
master-password-required = Master Password Required
enter-master-password = Please enter your master password to view this sensitive entry.
locked-need-password = Locked: need password
locked-need-pin = Locked: enter PIN
use-master-password-instead = Use master password instead
# Shown when the TPM refuses to unseal because the PCR state changed (BIOS or
# firmware update, Secure Boot toggle). The PIN itself is still valid; it must
# be re-sealed against the new machine state via a master-password unlock.
tpm-state-changed = The TPM state changed (firmware or BIOS update). Unlock with your master password and set the PIN again to re-enable PIN unlock.
not-configured = Not logged in — open vault to sign in.

# PIN unlock (TPM 2.0)
enable-pin-after-login = Enable PIN unlock after login
# { $count } is the minimum PIN length.
pin-min-chars = PIN (min { $count } characters)
new-pin-min-chars = New PIN (min { $count } characters)
pin-tpm-note-login = Secured by your device's hardware chip (TPM 2.0) — your PIN only works on this computer.
pin-tpm-note-settings = Secured by your device's hardware chip — PIN only works on this computer.
pin-reenable-note = Re-enable PIN unlock: enter a new PIN, or leave empty to turn PIN unlock off.
pin-optional-note = Optionally set a PIN to unlock this device quickly — leave empty to skip.
pin-empty-to-disable = PIN — empty to disable
pin-reenable-note-short = Re-enable PIN unlock, or leave empty to turn it off.
pin-optional-note-short = Optionally set a PIN for quick unlock — empty to skip.
# { $count } is the minimum PIN length.
pin-too-short = PIN must be at least { $count } characters
pin-incorrect = Incorrect PIN
pin-unlock-title = PIN Unlock (TPM 2.0)
pin-unlock = PIN unlock
new-pin = New PIN
checking-tpm = Checking TPM availability…
tpm-not-accessible = TPM 2.0 is not accessible (hardware missing or no permission).
status-active = Active
status-not-configured = Not configured
disable-pin-unlock = Disable PIN unlock
pin-will-be-removed = PIN unlock will be removed from this device.
store-hashed-password-tpm = Store hashed password in this device TPM chip
store-hashed-password-tpm-note = Fewer master password prompts after PIN unlock. Trade-off: if your PIN is compromised, anyone with physical access to this device can modify your Bitwarden account without knowing your master password.

# TPM dictionary-attack lockout status
duration-moment = a moment
# { $time } is a human-readable duration (e.g. "2h").
tpm-lockout-wait = TPM is locked out after too many failed attempts — wait ~{ $time } before retrying.
tpm-lockout = TPM is locked out after too many failed attempts.
# { $rem } attempts left, { $max } total.
tpm-attempts-remaining = { $rem } of { $max } attempts remaining before TPM lockout (shared across the device).
tpm-attempts-remaining-simple = { $rem } attempts remaining before TPM lockout.

# Settings
auto-lock = Auto-lock
# { $minutes } is the auto-lock timeout in minutes.
autolock-minutes = Auto-lock: { $minutes } min
minutes-fmt = { $minutes } min

# Vault sidebar
filter-all = All
filter-logins = Logins
filter-notes = Notes
filter-ssh-keys = SSH
no-entries-found = No entries found
no-pinned-entries = No pinned entries
no-results = No results
session-expired = Session expired — log in
not-synced = ⚠ Not synced
select-entry = Select an entry

# Entry detail
name = Name
entry-type = Entry Type
entry-type-login = Login
entry-type-note = Note
entry-type-ssh-key = SSH Key
field-username = Username
field-password = Password
field-totp = TOTP
field-totp-seed = TOTP Seed
field-private-key = Private Key
field-public-key = Public Key
field-card-number = Card Number
field-cardholder = Cardholder
field-brand = Brand
field-email = Email
notes = Notes
delete-entry = Delete Entry
delete-entry-title = Delete Entry?
confirm-delete = Are you sure you want to delete this entry? This action cannot be undone.
# { $id } is the entry's unique identifier.
entry-id = ID: { $id }

# Applet menu / tray
open-vault-window = Open Vault
lock-and-quit = Lock and Quit
logout-and-quit = Logout and Quit
just-quit = Just Quit
add-new-entry = Add New Entry
copied-to-clipboard = Copied to clipboard
public-key-label = Public key
generate-password = Generate password
# Applet header tooltips shown while unlocked when the last sync failed; the
# icon button next to the tooltip performs the described action.
sync-session-expired-tooltip = Session expired — click to log in again
sync-not-synced-tooltip = Not synced — click to retry
# { $label } is the translated "Quit" label (`quit`).
quit-menu-expanded = ▾ { $label }
quit-menu-collapsed = ▸ { $label }

# Password generator
password-generator = Password Generator
uppercase = Uppercase (A-Z)
lowercase = Lowercase (a-z)
numbers = Numbers (0-9)
special-characters = Special characters
# { $length } is the generated password's length.
password-length = Length: { $length }
generate = Generate
reset = Reset
no-password-generated-yet = No password generated yet.
recent-passwords = Recent Passwords (last 7 days)
no-recent-passwords = No recent passwords.
delete-history-entry-title = Delete Password?
confirm-delete-history-entry = Remove this password from the recent-passwords history? This action cannot be undone.
settings-save-not-loaded = Settings were not saved: the agent's configuration has not loaded yet. Reopen this window once the vault is available.
settings-save-failed = Settings were not saved: { $error }
