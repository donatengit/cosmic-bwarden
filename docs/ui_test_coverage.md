# UI Test Coverage Tracker

This file tracks the testing progress for all screens, components, and interactions within the `cosmic-bwarden` UI.

## Views & Transitions

| Feature / Screen | State Logic (`update`) | Status |
| :--- | :---: | :--- |
| **Initial Loading** | [✅] | ✅ Complete |
| **Setup View** | [✅] | ✅ Complete |
| **Unlock View** | [✅] | ✅ Complete |
| **Vault View** | [✅] | ✅ Complete |
| **Settings View** | [✅] | ✅ Complete |

## Detailed Interactions

### Setup Screen
- [✅] Email input updates state | ✅ Complete
- [✅] Password input updates state | ✅ Complete
- [✅] Server URL input updates state | ✅ Complete
- [✅] Remember Email toggle | ✅ Complete
- [✅] Login button produces `LoginSubmitted` | ✅ Complete
- [✅] Auth failure displays error message | ✅ Complete
- [✅] Auth success transitions to Vault | ✅ Complete
- [✅] ConfigReceived handles loading transitions | ✅ Complete

### Unlock Screen
- [✅] Password input updates state | ✅ Complete
- [✅] Unlock button produces `UnlockSubmitted` | ✅ Complete
- [✅] "Use different account" (Logout) button | ✅ Complete
- [✅] Auth failure displays error message | ✅ Complete
- [✅] Auth success transitions to Vault | ✅ Complete

### Vault Screen (Main)
- [✅] Search input updates `search_query` | ✅ Complete
- [✅] Search submission triggers `fetch_entries` | ✅ Complete
- [✅] Filter buttons (All/Logins/Notes/SSH) trigger re-fetch | ✅ Complete
- [✅] Selecting an entry updates `selected_entry_id` | ✅ Complete
- [✅] Entries/TopEntries received updates state | ✅ Complete
- [✅] Edit button transitions to editing mode | ✅ Complete
- [✅] Edit field updates `editing_entry` | ✅ Complete
- [✅] Cancel edit resets state | ✅ Complete
- [✅] Lock/Logout clears sensitive data and revealed fields | ✅ Complete
- [✅] Secret masking with reveal toggle (eye icon) | ✅ Complete
- [✅] Support for SSH Keys, Cards, and Identities | ✅ Complete
- [✅] Notes field includes copy button | ✅ Complete

### Settings Screen
- [✅] Edit button enters editing mode | ✅ Complete
- [✅] Saving updates configuration | ✅ Complete
- [✅] Canceling discards changes | ✅ Complete
- [✅] Field inputs (Lock timeout, Popular count, etc.) | ✅ Complete
- [✅] Unified field widths in edit mode | ✅ Complete

### Applet (Popup)
- [✅] Popup closing resets state | ✅ Complete
- [✅] "Secrets Manager" opens main window | ✅ Complete
- [✅] "Settings" opens settings window | ✅ Complete
- [✅] Clicking a recent entry triggers `CopyPassword` | ✅ Complete
- [✅] Lock button triggers `LockClicked` | ✅ Complete
- [✅] Logout button triggers `LogoutClicked` | ✅ Complete
- [✅] Exit button terminates application | ✅ Complete (Path verified)

## Legend
- ✅ **Complete**: Fully tested and verified.
- 🚧 **Partial**: Some coverage, but critical paths missing.
- ⏳ **Pending**: No tests yet.
