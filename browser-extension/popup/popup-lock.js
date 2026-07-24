// Locked-vault view: PIN unlock (TPM 2.0). Loaded *before* popup.js (see
// popup.html) even though its functions call popup.js globals (showView,
// browser) — popup.js's init IIFE awaits browser.tabs.query and can resume
// via an already-queued microtask before the parser reaches a script tag
// that comes after popup.js, so anything popup.js might call during init
// must be defined first. Only function *bodies* here reference popup.js's
// globals; nothing at this file's top level calls them, so load order is
// otherwise safe. Mirrors the desktop UI's PIN-unlock flow
// (crates/cosmic-bwarden-ui/src/view/auth.rs, app/update/auth.rs) but the
// extension only ever offers PIN entry here — master password unlock stays a
// desktop/CLI-only action, so the fallback just points the user there instead
// of collecting the master password in the popup.

const lockedMessage    = document.getElementById('locked-message');
const lockedPinGroup   = document.getElementById('locked-pin-group');
const lockedPinInput   = document.getElementById('locked-pin-input');
const lockedUnlockBtn  = document.getElementById('locked-unlock-btn');
const lockedFeedback   = document.getElementById('locked-feedback');
const lockedFallbackBtn = document.getElementById('locked-fallback-btn');

// Stable error string the agent uses for a failed TPM unseal (wrong PIN,
// changed PCRs, or DA lockout) — see cosmic_bwarden_core::protocol::ERR_TPM_UNSEAL_FAILED.
// Any other Error message is an environmental failure (no account, agent
// error, etc.) and is shown as-is rather than mislabeled as a wrong PIN.
const ERR_TPM_UNSEAL_FAILED = 'TPM unseal failed';

// Mirrors format_secs in crates/cosmic-bwarden-ui/src/view/mod.rs.
function formatSecs(secs) {
    if (secs === 0) return 'a moment';
    if (secs % 3600 === 0) return `${Math.floor(secs / 3600)}h`;
    if (secs >= 3600) return `${Math.floor(secs / 3600)}h${Math.floor((secs % 3600) / 60)}m`;
    if (secs >= 60) return `${Math.floor(secs / 60)}m`;
    return `${secs}s`;
}

// Mirrors CosmicBWardenApp::tpm_da_line — a one-line summary of the TPM
// dictionary-attack lockout state, or null if there is nothing to show.
function tpmDaLine(status) {
    if (!status || !status.available) return null;
    if (status.in_lockout) {
        return status.recovery_interval_secs != null
            ? `TPM is locked out after too many failed attempts — wait ~${formatSecs(status.recovery_interval_secs)} before retrying.`
            : 'TPM is locked out after too many failed attempts.';
    }
    if (status.remaining != null && status.max_tries != null) {
        return `${status.remaining} of ${status.max_tries} attempts remaining before TPM lockout (shared across the device).`;
    }
    if (status.remaining != null) {
        return `${status.remaining} attempts remaining before TPM lockout.`;
    }
    return null;
}

// Mirrors CosmicBWardenApp::pin_feedback_line — the DA line when known, else
// a plain "Incorrect PIN".
function pinFeedbackLine(status) {
    return tpmDaLine(status) || 'Incorrect PIN';
}

function showLockedFeedback(text) {
    lockedFeedback.textContent = text;
    lockedFeedback.classList.remove('hidden');
}

function hideLockedFeedback() {
    lockedFeedback.classList.add('hidden');
    lockedFeedback.textContent = '';
}

// No PIN configured (or TPM unavailable): same message as before this
// feature existed, plus no PIN input — unlocking needs the master password,
// which the extension never collects (thin-client invariant).
function showNoPinFallback() {
    lockedMessage.textContent = 'Vault is locked — unlock via the COSMIC app or applet.';
    lockedPinGroup.classList.add('hidden');
    lockedFallbackBtn.classList.add('hidden');
    hideLockedFeedback();
}

async function showLockedView() {
    showView('locked');
    lockedPinInput.value = '';
    hideLockedFeedback();

    let pinAvailable = false;
    try {
        const resp = await browser.runtime.sendMessage('CheckTpm');
        pinAvailable = !!(resp && resp.TpmStatus && resp.TpmStatus.available && resp.TpmStatus.configured);
    } catch { /* fall through to the no-PIN message below */ }

    if (pinAvailable) {
        lockedMessage.textContent = 'Vault is locked. Enter your PIN to unlock.';
        lockedPinGroup.classList.remove('hidden');
        lockedFallbackBtn.classList.remove('hidden');
        lockedPinInput.focus();
    } else {
        showNoPinFallback();
    }
}

const lockedUnlockLabel = lockedUnlockBtn.textContent;

async function submitPin() {
    const pin = lockedPinInput.value;
    if (!pin) return;

    lockedUnlockBtn.disabled = true;
    lockedUnlockBtn.textContent = '…';
    lockedPinInput.disabled = true;
    hideLockedFeedback();

    try {
        const resp = await browser.runtime.sendMessage({ UnlockWithPin: { pin } });
        lockedPinInput.value = '';

        // Response::Ack is a unit enum variant — serde serializes it as the
        // bare string "Ack", not { Ack: true } (only variants with fields,
        // like Error{message}, become an object). Every other agent-response
        // check in this extension already accounts for this (popup.js,
        // popup-detail.js, popup-edit.js, background-save.js); this one
        // previously didn't, so a *successful* unlock still showed
        // "Unexpected agent response." here.
        if (resp === 'Ack' || (resp && resp.Ack)) {
            showView('list');
            return;
        }

        const message = resp && resp.Error ? resp.Error.message : 'Unexpected agent response.';
        if (message === ERR_TPM_UNSEAL_FAILED) {
            let status = null;
            try {
                const daResp = await browser.runtime.sendMessage('GetTpmDaStatus');
                status = daResp && daResp.TpmDaStatus ? daResp.TpmDaStatus.status : null;
            } catch { /* show the plain incorrect-PIN fallback below */ }
            showLockedFeedback(pinFeedbackLine(status));
        } else {
            showLockedFeedback(message);
        }
    } catch (e) {
        lockedPinInput.value = '';
        showLockedFeedback(e.message || 'Failed to communicate with agent.');
    } finally {
        lockedUnlockBtn.disabled = false;
        lockedUnlockBtn.textContent = lockedUnlockLabel;
        lockedPinInput.disabled = false;
        lockedPinInput.focus();
    }
}

lockedUnlockBtn.onclick = submitPin;
lockedPinInput.addEventListener('keydown', (e) => { if (e.key === 'Enter') submitPin(); });
lockedFallbackBtn.onclick = showNoPinFallback;
