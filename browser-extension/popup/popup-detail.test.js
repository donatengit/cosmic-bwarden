// @vitest-environment jsdom
// Unit tests for the detail-view pure helpers in popup-detail.js: field
// selection (only filled fields, masked secret rows) and per-key secret
// extraction. Mirrors the desktop UI's `detailFields`/`field_plaintext` logic.
import { describe, it, expect } from 'vitest';
import { readFileSync } from 'fs';
import { fileURLToPath } from 'url';
import path from 'path';

const source = readFileSync(
    path.join(path.dirname(fileURLToPath(import.meta.url)), 'popup-detail.js'),
    'utf8'
);

// popup-detail.js wires DOM elements and event listeners at load time; give it
// a document containing the ids it expects before evaluating.
document.body.innerHTML = `
    <div id="detail-content"></div>
    <button id="edit-btn"></button>
    <button id="delete-btn"></button>
    <button id="cancel-btn"></button>
`;
globalThis.browser = { runtime: { sendMessage: async () => ({}) } };
// popup-detail.js references these globals (defined in popup.js / popup-edit.js
// / icons.js when loaded as a real page); stub them for the pure-function test.
globalThis.getEntryType = (e) => e && e.data && e.data.Login ? 'Login' : 'Unknown';

const { detailFields, secretValue } = new Function(
    `${source}\nreturn { detailFields, secretValue };`
)();

// A login entry as the agent returns it *before* redaction (for secretValue)
// and *after* redaction (for detailFields, with filledSecrets separately).
// Values are plain JSON strings — the real native-messaging wire shape.
const loginValue = {
    id: 'e1', name: 'Site', data: {
        Login: { username: 'alice', password: 'pw1', totp: 'seed', uris: [{ uri: 'https://a.example' }, { uri: 'https://b.example' }] },
    },
    notes: 'note text',
    fields: [
        { name: 'server', value: 'us-east', ty: 0 },        // Text
        { name: 'secret', value: 'hidden1', ty: 1 }, // Hidden
    ],
};
const loginMeta = {
    id: 'e1', name: 'Site', data: {
        Login: { username: 'alice', password: null, totp: null, uris: [{ uri: 'https://a.example' }, { uri: 'https://b.example' }] },
    },
    notes: null,
    fields: [
        { name: 'server', value: 'us-east', ty: 0 },
        { name: 'secret', value: null, ty: 1 },
    ],
};
describe('detailFields', () => {
    it('lists only filled non-secret fields and filled secret slots', () => {
        const fields = detailFields(loginMeta, ['Password', 'TOTP', 'Notes', 'secret']);
        const keys = fields.map(f => f.key);
        const labels = fields.map(f => f.label);
        expect(keys).toContain('Username');
        expect(labels).toContain('URL');
        expect(labels).toContain('URL 2');
        expect(keys).toContain('server');
        // Secret slots appear but as masked entries, not with values.
        const pw = fields.find(f => f.key === 'Password');
        expect(pw).toMatchObject({ key: 'Password', secret: true });
        expect(pw.value).toBeUndefined();
        const totp = fields.find(f => f.key === 'TOTP');
        expect(totp).toMatchObject({ secret: true });
        const notes = fields.find(f => f.key === 'Notes');
        expect(notes).toMatchObject({ secret: true });
        // Hidden custom field: masked row.
        const hidden = fields.find(f => f.key === 'secret');
        expect(hidden).toMatchObject({ secret: true });
        // Visible custom field keeps its plaintext value.
        const vis = fields.find(f => f.key === 'server');
        expect(vis).toMatchObject({ value: 'us-east' });
    });

    it('omits secret rows the agent does not report as filled', () => {
        const fields = detailFields(loginMeta, []);
        expect(fields.find(f => f.key === 'Password')).toBeUndefined();
        expect(fields.find(f => f.key === 'TOTP')).toBeUndefined();
        expect(fields.find(f => f.key === 'Notes')).toBeUndefined();
        // Visible custom field still shows.
        expect(fields.find(f => f.key === 'server')).toBeDefined();
    });

    it('orders card type fields and masks the security code', () => {
        const redacted = {
            id: 'c1', name: 'Card', data: {
                Card: { cardholder_name: 'A B', brand: 'Visa', exp_month: '12', exp_year: '30', number: null, code: null },
            },
            fields: [],
        };
        const fields = detailFields(redacted, ['Card Number', 'Security Code']);
        const keys = fields.map(f => f.key);
        expect(keys.indexOf('Card Number')).toBeLessThan(keys.indexOf('Cardholder'));
        expect(keys.indexOf('Cardholder')).toBeLessThan(keys.indexOf('Brand'));
        expect(keys.indexOf('Brand')).toBeLessThan(keys.indexOf('Expiry'));
        expect(fields.find(f => f.key === 'Security Code')).toMatchObject({ secret: true });
        expect(fields.find(f => f.key === 'Card Number')).toMatchObject({ secret: true });
    });

    it('renders bank account / driver license / passport fields', () => {
        const bank = {
            id: 'b1', name: 'Bank', data: {
                BankAccount: { bank_name: 'Chase', account_type: 'Checking', account_number: null, routing_number: null, branch_number: null, pin: null, swift_code: null, iban: null, bank_contact_phone: '555-0100' },
            },
            fields: [],
        };
        const bf = detailFields(bank, ['Account Number', 'Routing Number', 'PIN']);
        expect(bf.find(f => f.key === 'Bank Name')).toMatchObject({ value: 'Chase' });
        expect(bf.find(f => f.key === 'Account Number')).toMatchObject({ secret: true });
        expect(bf.find(f => f.key === 'PIN')).toMatchObject({ secret: true });
        expect(bf.find(f => f.key === 'Bank Contact Phone')).toMatchObject({ value: '555-0100' });

        const dl = {
            id: 'd1', name: 'Lic', data: {
                DriversLicense: { first_name: 'Jane', last_name: 'Doe', license_number: null, issuing_state: 'CA', license_class: 'C' },
            },
            fields: [],
        };
        const df = detailFields(dl, ['License Number']);
        expect(df.find(f => f.key === 'First Name')).toMatchObject({ value: 'Jane' });
        expect(df.find(f => f.key === 'License Number')).toMatchObject({ secret: true });
        expect(df.find(f => f.key === 'Issuing State')).toMatchObject({ value: 'CA' });

        const pp = {
            id: 'p1', name: 'Pass', data: {
                Passport: { surname: 'Doe', given_name: 'Jane', passport_number: null, nationality: 'US' },
            },
            fields: [],
        };
        const pf = detailFields(pp, ['Passport Number']);
        expect(pf.find(f => f.key === 'Surname')).toMatchObject({ value: 'Doe' });
        expect(pf.find(f => f.key === 'Passport Number')).toMatchObject({ secret: true });
    });

    it('secure note shows notes as plaintext content', () => {
        const note = { id: 's1', name: 'Note', data: 'SecureNote', notes: 'the note', fields: [] };
        const fields = detailFields(note, []);
        expect(fields.find(f => f.key === 'Notes')).toMatchObject({ value: 'the note' });
    });
});

describe('secretValue', () => {
    it('recovers primary secrets from a full entry', () => {
        expect(secretValue(loginValue, 'Password')).toBe('pw1');
        expect(secretValue({ ...loginValue, data: { SshKey: { private_key: 'pk' } } }, 'Private Key')).toBe('pk');
        expect(secretValue({ ...loginValue, data: { Card: { number: '4111', code: '123' } } }, 'Card Number')).toBe('4111');
        expect(secretValue({ ...loginValue, data: { Card: { number: '4111', code: '123' } } }, 'Security Code')).toBe('123');
        // TOTP is never read from the entry — it is fetched live via GetTotp.
        expect(secretValue(loginValue, 'TOTP')).toBeNull();
    });

    it('recovers bank / identity / document secrets and notes', () => {
        expect(secretValue({ ...loginValue, data: { BankAccount: { account_number: '123', iban: 'XY' } } }, 'Account Number')).toBe('123');
        expect(secretValue({ ...loginValue, data: { BankAccount: { account_number: null, iban: 'XY' } } }, 'IBAN')).toBe('XY');
        expect(secretValue({ ...loginValue, data: { BankAccount: { ssn_unused: null, iban: 'XY' } } }, 'Routing Number')).toBe('');
        expect(secretValue({ ...loginValue, data: { Identity: { ssn: '000' } } }, 'SSN')).toBe('000');
        expect(secretValue({ ...loginValue, data: { Passport: { passport_number: 'P1' } } }, 'Passport Number')).toBe('P1');
        expect(secretValue({ ...loginValue, data: { Login: { username: 'a', password: null, totp: null, uris: [] } } }, 'Notes')).toBe('note text');
    });

    it('recovers hidden custom fields by name', () => {
        expect(secretValue(loginValue, 'secret')).toBe('hidden1');
        expect(secretValue(loginValue, 'server')).toBe('us-east');
    });
});
