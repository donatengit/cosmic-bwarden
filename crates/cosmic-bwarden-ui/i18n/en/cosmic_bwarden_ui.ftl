app-title = CosmicBWarden
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
welcome-title = Welcome to CosmicBWarden
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
filter-ssh-keys = SSH Keys
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
