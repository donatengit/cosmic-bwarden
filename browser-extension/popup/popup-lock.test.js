// @vitest-environment jsdom
// Unit tests for the pure TPM dictionary-attack feedback formatting in
// popup-lock.js — mirrors CosmicBWardenApp::tpm_da_line/pin_feedback_line
// and view/mod.rs's format_secs (crates/cosmic-bwarden-ui).
import { describe, it, expect } from 'vitest';
import { readFileSync } from 'fs';
import { fileURLToPath } from 'url';
import path from 'path';

const source = readFileSync(
    path.join(path.dirname(fileURLToPath(import.meta.url)), 'popup-lock.js'),
    'utf8'
);

// popup-lock.js wires DOM elements and event listeners at load time; give it
// a document containing the ids it expects before evaluating.
document.body.innerHTML = `
    <div id="locked-message"></div>
    <div id="locked-pin-group"><input id="locked-pin-input"><button id="locked-unlock-btn"></button></div>
    <div id="locked-feedback"></div>
    <button id="locked-fallback-btn"></button>
`;
globalThis.browser = { runtime: { sendMessage: async () => ({}) } };

const { formatSecs, tpmDaLine, pinFeedbackLine, unlockErrorMessage } = new Function(
    `${source}\nreturn { formatSecs, tpmDaLine, pinFeedbackLine, unlockErrorMessage };`
)();

describe('formatSecs', () => {
    it('formats exact hours', () => expect(formatSecs(7200)).toBe('2h'));
    it('formats hours and minutes', () => expect(formatSecs(5400)).toBe('1h30m'));
    it('formats minutes', () => expect(formatSecs(90)).toBe('1m'));
    it('formats seconds', () => expect(formatSecs(45)).toBe('45s'));
    it('formats zero as "a moment"', () => expect(formatSecs(0)).toBe('a moment'));
});

describe('tpmDaLine', () => {
    it('returns null when status is missing', () => expect(tpmDaLine(null)).toBeNull());

    it('returns null when the TPM is unavailable', () => {
        expect(tpmDaLine({ available: false })).toBeNull();
    });

    it('reports in-lockout with a wait time', () => {
        const line = tpmDaLine({ available: true, in_lockout: true, recovery_interval_secs: 3600 });
        expect(line).toContain('locked out');
        expect(line).toContain('1h');
    });

    it('reports in-lockout without a known wait time', () => {
        const line = tpmDaLine({ available: true, in_lockout: true, recovery_interval_secs: null });
        expect(line).toBe('TPM is locked out after too many failed attempts.');
    });

    it('reports remaining/max attempts', () => {
        const line = tpmDaLine({ available: true, in_lockout: false, remaining: 29, max_tries: 32 });
        expect(line).toBe('29 of 32 attempts remaining before TPM lockout (shared across the device).');
    });

    it('reports remaining attempts without a known max', () => {
        const line = tpmDaLine({ available: true, in_lockout: false, remaining: 5, max_tries: null });
        expect(line).toBe('5 attempts remaining before TPM lockout.');
    });

    it('returns null when neither remaining nor lockout is known', () => {
        const line = tpmDaLine({ available: true, in_lockout: false, remaining: null, max_tries: null });
        expect(line).toBeNull();
    });
});

describe('pinFeedbackLine', () => {
    it('falls back to a plain "Incorrect PIN" when no DA line is available', () => {
        expect(pinFeedbackLine(null)).toBe('Incorrect PIN');
        expect(pinFeedbackLine({ available: false })).toBe('Incorrect PIN');
    });

    it('prefers the DA attempts-remaining line when available', () => {
        const status = { available: true, in_lockout: false, remaining: 10, max_tries: 32 };
        expect(pinFeedbackLine(status)).toContain('10 of 32');
    });
});

describe('unlockErrorMessage', () => {
    it('never shows a PCR-state change as an incorrect PIN', () => {
        const line = unlockErrorMessage('TPM state changed', null);
        expect(line).toContain('TPM state changed');
        expect(line).toContain('master password');
        expect(line).not.toContain('Incorrect PIN');
    });

    it('maps the unseal error to DA/incorrect-PIN feedback', () => {
        expect(unlockErrorMessage('TPM unseal failed', null)).toBe('Incorrect PIN');
        const status = { available: true, in_lockout: false, remaining: 9, max_tries: 32 };
        expect(unlockErrorMessage('TPM unseal failed', status)).toContain('9 of 32');
    });

    it('passes environmental errors through verbatim', () => {
        expect(unlockErrorMessage('no account configured', null)).toBe('no account configured');
        expect(unlockErrorMessage('Unexpected agent response.', null)).toBe('Unexpected agent response.');
    });
});
