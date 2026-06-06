# Plan: COSMIC UI Redesign & Binary Merge

Redesign the COSMIC UI, merge the applet and main UI into a single binary, and improve secret handling and usage tracking.

## Objectives
- [ ] Merge `cosmic-bwarden-ui` and `cosmic-bwarden-ui` into a single COSMIC applet binary.
- [ ] Redesign Login/Unlock layout (narrower, hidden passwords).
- [ ] Redesign Main Window (search, sidebar with type filter, detail panel with copy buttons and edit mode).
- [ ] Implement Settings screen.
- [ ] Improve usage tracking (top entries in last N days).

## Phase 1: Core & Agent Enhancements
### 1.1. `cosmic-bwarden-core`: Configuration & Protocol
- [x] **`crates/cosmic-bwarden-core/src/config.rs`**:
    - Add `top_popular_count: u32` (default 5).
    - Add `top_popular_days: u32` (default 7).
- [x] **`crates/cosmic-bwarden-core/src/protocol.rs`**:
    - Update `Action::GetTopFrequent` to include `days: Option<u32>`.

### 1.2. `cosmic-bwarden-agent`: Usage Tracking
- [x] **`crates/cosmic-bwarden-agent/src/state.rs`**:
    - Change `frequency` from `HashMap<String, u32>` to `HashMap<String, Vec<i64>>` (storing unix timestamps).
    - Update `record_copy` to append the current timestamp.
    - Update `top_frequent` to filter by the provided `days` (or default from config).
- [x] **`crates/cosmic-bwarden-agent/src/main.rs`**:
    - Update the handler for `Action::GetTopFrequent` to use the new `days` parameter.

## Phase 2: UI Merging & Redesign
### 2.1. Binary Merge
- [x] Move `cosmic-bwarden-ui` logic into `cosmic-bwarden-ui`.
- [x] Update `crates/cosmic-bwarden-ui/Cargo.toml` to include `applet` feature for `libcosmic`.
- [x] Change `crates/cosmic-bwarden-ui/src/main.rs` to use `cosmic::applet::run`.
- [x] Remove `crates/cosmic-bwarden-ui` directory.
- [x] Update root `Cargo.toml` to remove `crates/cosmic-bwarden-ui` from workspace members.

### 2.2. Applet Implementation
- [x] Implement `view()` to return the applet icon button.
- [x] Add a context menu to the applet icon:
    - "Secrets Manager" (Opens/Focuses Main Window)
    - "Settings" (Opens/Focuses Settings Window)
    - Divider
    - Top 5 used entries (with copy action)
    - Divider
    - "Logout"

### 2.3. Window Management
- [x] Use `HashMap<window::Id, WindowState>` in the app state to manage multiple windows.
- [x] Implement `view_window(id)` to render the appropriate window based on `WindowState`.

### 2.4. View Redesigns
- [x] **Login/Unlock**:
    - Centered narrow container (~360px).
    - Fix `secure_input` to hide password characters (`hidden: true`).
- [x] **Main Window**:
    - Top: Search input.
    - Layout: `Row` with two columns.
    - Left Column (Sidebar):
        - Dropdown for entry type (All, Login, Note, Card, Identity).
        - Scrollable list of entries matching search and type.
    - Right Column (Details):
        - If selected: Field-by-field display with copy buttons.
        - `Notes` taking remaining space.
        - "Edit" button in top-right.
    - Edit Mode:
        - Switch fields to `text_input`.
        - "Save" and "Cancel" buttons.
- [x] **Settings**:
    - Fields for Email, Server, Auto-lock timeout.
    - Fields for Top Popular entries count and days.

## Phase 3: Verification
- [ ] Verify the new UI layout manually.
- [ ] Test "Top 5 most used in last 7 days" by copying entries and checking the applet menu.
- [ ] Ensure "Edit" mode correctly updates entries via the agent.
- [ ] Verify that Logout correctly locks the vault and clears sensitive state.

## Considerations
- **Persistence of Frequency**: Usage frequency is currently in-memory in the agent. If the agent restarts, it's lost. We might want to persist it to a file eventually, but for now, in-memory is fine as requested.
- **Multiple Windows**: Managing multiple windows in `iced`/`cosmic` requires careful tracking of `window::Id`.
