// @vitest-environment jsdom
// Unit tests for the shared form heuristics used by fill and submit capture.
import { describe, it, expect } from 'vitest';
import { readFileSync } from 'fs';
import { fileURLToPath } from 'url';
import path from 'path';

const source = readFileSync(
    path.join(path.dirname(fileURLToPath(import.meta.url)), 'content-heuristics.js'),
    'utf8'
);
// Vitest files are ESM (strict mode), where eval() keeps function declarations
// local — load through new Function and return the helpers instead.
const { findUsernameInput, findUsernameOnlyInput, findPasswordInput, findSubmittedCredentials } = new Function(
    `${source}\nreturn { findUsernameInput, findUsernameOnlyInput, findPasswordInput, findSubmittedCredentials };`
)();

function form(html) {
    document.body.innerHTML = `<form id="f">${html}</form>`;
    return document.getElementById('f');
}

describe('findUsernameInput', () => {
    it('prefers inputs whose name/id/placeholder look like a username', () => {
        const f = form(`
            <input type="text" name="captcha">
            <input type="text" name="login-user">
        `);
        expect(findUsernameInput(f).name).toBe('login-user');
    });

    it('matches email-type inputs by name', () => {
        const f = form(`<input type="email" name="email"><input type="password" name="p">`);
        expect(findUsernameInput(f).name).toBe('email');
    });

    it('falls back to the first text input when nothing matches', () => {
        const f = form(`<input type="text" name="first"><input type="text" name="second">`);
        expect(findUsernameInput(f).name).toBe('first');
    });

    it('returns null when the form has no text inputs', () => {
        const f = form(`<input type="password" name="p">`);
        expect(findUsernameInput(f)).toBeNull();
    });

    it('matches a hyphenated compound at full weight', () => {
        const f = form(`
            <input type="text" name="first">
            <input type="text" name="ldap-username">
        `);
        expect(findUsernameInput(f).name).toBe('ldap-username');
    });

    it('splits camelCase before matching (idLogin)', () => {
        const f = form(`
            <input type="text" name="first">
            <input type="text" id="idLogin">
        `);
        expect(findUsernameInput(f).id).toBe('idLogin');
    });

    it('matches a fused lowercase keyword at half weight', () => {
        const f = form(`
            <input type="text" name="first">
            <input type="text" name="idlogin">
        `);
        expect(findUsernameInput(f).name).toBe('idlogin');
    });

    it('uses required only as a tie-break, never as a gate', () => {
        const f = form(`
            <input type="text" name="user_a">
            <input type="text" name="user_b" required>
        `);
        expect(findUsernameInput(f).name).toBe('user_b');
    });

    it('finds a plain login field that is not required', () => {
        const f = form(`<input type="text" name="login">`);
        expect(findUsernameInput(f).name).toBe('login');
    });

    it('prefers a username field over a newsletter or search field', () => {
        const f = form(`
            <input type="email" name="newsletter-email" required>
            <input type="text" name="username">
        `);
        expect(findUsernameInput(f).name).toBe('username');
    });

    it('reads the data-testid hint', () => {
        const f = form(`<input type="text" name="first" data-testid="username">`);
        expect(findUsernameInput(f).name).toBe('first');
    });

    it('reads the aria-label hint', () => {
        const f = form(`<input type="text" name="first" aria-label="Login">`);
        expect(findUsernameInput(f).name).toBe('first');
    });

    it('reads the text of an associated label', () => {
        const f = form(`
            <input type="text" name="first">
            <label for="uid">Email address</label>
            <input type="text" id="uid">
        `);
        expect(findUsernameInput(f).id).toBe('uid');
    });

    it('does not fall back to a search box in a password form', () => {
        const f = form(`
            <input type="text" name="site-search" placeholder="Search">
            <input type="password" name="password">
        `);
        expect(findUsernameInput(f)).toBeNull();
    });
});

describe('findUsernameOnlyInput', () => {
    it('finds a username field by autocomplete when no password field exists yet', () => {
        // Multi-step logins (Google/Microsoft/SSO-style): the first screen
        // only has an email/username field, no password field.
        form(`<input type="email" name="identifier" autocomplete="username">`);
        expect(findUsernameOnlyInput().name).toBe('identifier');
    });

    it('finds a username field by keyword when no password field exists yet', () => {
        form(`<input type="text" name="login-user">`);
        expect(findUsernameOnlyInput().name).toBe('login-user');
    });

    it('accepts a bare email-type field as a username signal', () => {
        // Providers routinely leave name/id empty and mark the field only
        // with type="email".
        form(`<input type="email">`);
        expect(findUsernameOnlyInput()).not.toBeNull();
    });

    it('reads an associated label when name and id are opaque', () => {
        form(`<label for="uid">Email address</label><input type="text" id="uid">`);
        expect(findUsernameOnlyInput().id).toBe('uid');
    });

    it('matches a German benutzername field', () => {
        form(`<input type="text" name="benutzername">`);
        expect(findUsernameOnlyInput().name).toBe('benutzername');
    });

    it('accepts autocomplete="email" as a username signal', () => {
        form(`<input type="text" name="q" autocomplete="email">`);
        expect(findUsernameOnlyInput().name).toBe('q');
    });

    it('does not fall back to an unrelated field (e.g. a search box)', () => {
        form(`<input type="text" name="site-search" placeholder="Search...">`);
        expect(findUsernameOnlyInput()).toBeNull();
    });

    it('rejects a required newsletter field when no login field is present', () => {
        form(`<input type="email" name="newsletter-email" required autocomplete="email">`);
        expect(findUsernameOnlyInput()).toBeNull();
    });

    it('rejects a field that is also required', () => {
        form(`<input type="text" name="sms-code" required>`);
        expect(findUsernameOnlyInput()).toBeNull();
    });

    it.each(['verification-code', 'pin', 'captcha', 'site-search'])(
        'does not fill a %s field',
        (name) => {
            form(`<input type="text" name="${name}" required>`);
            expect(findUsernameOnlyInput()).toBeNull();
        }
    );

    it('does not treat required as a signal on its own', () => {
        form(`<input type="text" name="promo" required>`);
        expect(findUsernameOnlyInput()).toBeNull();
    });

    it('does not fill a one-time-code field named otc-confirmation', () => {
        // Microsoft marks its TOTP box `otc-confirmation-input`; filling the
        // stored username into it at the second step is a visible bug.
        form(`<input type="text" id="otc-confirmation-input" required>`);
        expect(findUsernameOnlyInput()).toBeNull();
    });

    it('does not fill a field described by aria-describedby as a one-time code', () => {
        form(`<input type="text" name="q" aria-describedby="oneTimeCodeDescription" required>`);
        expect(findUsernameOnlyInput()).toBeNull();
    });

    it('does not fill a field inside a newsletter signup form', () => {
        document.body.innerHTML = `
            <form id="newsletter-signup"><input type="email"></form>
        `;
        expect(findUsernameOnlyInput()).toBeNull();
    });

    it('returns null on a page with no text inputs', () => {
        form(`<input type="password" name="password">`);
        expect(findUsernameOnlyInput()).toBeNull();
    });

    it('skips hidden fields such as honeypots', () => {
        form(`
            <input type="text" name="user" style="display: none">
            <input type="email" name="email-real" autocomplete="username">
        `);
        expect(findUsernameOnlyInput().name).toBe('email-real');
    });
});

describe('findPasswordInput', () => {
    it('prefers an explicit current-password field', () => {
        const f = form(`
            <input type="password" name="new" autocomplete="new-password">
            <input type="password" name="current" autocomplete="current-password">
        `);
        expect(findPasswordInput(f).name).toBe('current');
    });

    it('skips a confirmation field when a plain password field exists', () => {
        const f = form(`
            <input type="password" name="password">
            <input type="password" name="confirmpassword">
        `);
        expect(findPasswordInput(f).name).toBe('password');
    });

    it('falls back to the first field when every field is new or confirmation', () => {
        const f = form(`
            <input type="password" name="new">
            <input type="password" name="confirm">
        `);
        expect(findPasswordInput(f).name).toBe('new');
    });

    it('returns null when the form has no password fields', () => {
        const f = form(`<input type="text" name="username">`);
        expect(findPasswordInput(f)).toBeNull();
    });

    it.each(['pin', 'verification-code'])('refuses to fill a %s field', (name) => {
        const f = form(`<input type="password" name="${name}" required>`);
        expect(findPasswordInput(f)).toBeNull();
    });

    it('refuses a field explicitly marked one-time-code', () => {
        const f = form(`<input type="password" name="p" autocomplete="one-time-code">`);
        expect(findPasswordInput(f)).toBeNull();
    });

    it('refuses a hidden password field', () => {
        const f = form(`<input type="password" name="password" style="display: none">`);
        expect(findPasswordInput(f)).toBeNull();
    });

    it('picks the real password when an OTP field shares the form', () => {
        const f = form(`
            <input type="password" name="verification-code" autocomplete="one-time-code">
            <input type="password" name="password">
        `);
        expect(findPasswordInput(f).name).toBe('password');
    });

    it('accepts a single input as its own scope', () => {
        document.body.innerHTML = `<input id="p" type="password" name="password">`;
        const input = document.getElementById('p');
        expect(findPasswordInput(input)).toBe(input);
    });
});

describe('findSubmittedCredentials', () => {
    it('captures username and password from a plain login form', () => {
        const f = form(`
            <input type="text" name="username" value="alice">
            <input type="password" name="password" value="secret1">
        `);
        expect(findSubmittedCredentials(f)).toEqual({ username: 'alice', password: 'secret1' });
    });

    it('trims whitespace around the username', () => {
        const f = form(`
            <input type="text" name="username" value="  alice  ">
            <input type="password" name="password" value="secret1">
        `);
        expect(findSubmittedCredentials(f).username).toBe('alice');
    });

    it('picks the last non-empty password (registration confirm field)', () => {
        const f = form(`
            <input type="email" name="email" value="alice@example.com">
            <input type="password" name="new-password" value="chosen-pw">
            <input type="password" name="confirm-password" value="chosen-pw">
        `);
        expect(findSubmittedCredentials(f)).toEqual({
            username: 'alice@example.com',
            password: 'chosen-pw',
        });
    });

    it('picks the new password on a change-password form, not the current one', () => {
        const f = form(`
            <input type="text" name="username" value="alice">
            <input type="password" name="current" value="old-pw">
            <input type="password" name="new" value="new-pw">
            <input type="password" name="confirm" value="new-pw">
        `);
        expect(findSubmittedCredentials(f).password).toBe('new-pw');
    });

    it('skips empty password inputs when picking the last one', () => {
        const f = form(`
            <input type="text" name="username" value="alice">
            <input type="password" name="password" value="secret1">
            <input type="password" name="unused" value="">
        `);
        expect(findSubmittedCredentials(f).password).toBe('secret1');
    });

    it('returns empty strings when the form is empty', () => {
        const f = form(`<input type="password" name="password" value="">`);
        expect(findSubmittedCredentials(f)).toEqual({ username: '', password: '' });
    });

    it('reports no password when the only password field is a PIN', () => {
        const f = form(`
            <input type="text" name="username" value="alice">
            <input type="password" name="pin" value="1234">
        `);
        expect(findSubmittedCredentials(f)).toEqual({ username: 'alice', password: '' });
    });

    it('captures the real password when an OTP field is also present', () => {
        const f = form(`
            <input type="text" name="username" value="alice">
            <input type="password" name="password" value="secret1">
            <input type="password" name="verification-code" value="123456">
        `);
        expect(findSubmittedCredentials(f).password).toBe('secret1');
    });
});
