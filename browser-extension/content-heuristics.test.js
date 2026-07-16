// @vitest-environment jsdom
// Unit tests for the shared form heuristics used by fill and submit capture.
import { describe, it, expect, beforeEach } from 'vitest';
import { readFileSync } from 'fs';
import { fileURLToPath } from 'url';
import path from 'path';

const source = readFileSync(
    path.join(path.dirname(fileURLToPath(import.meta.url)), 'content-heuristics.js'),
    'utf8'
);
// Vitest files are ESM (strict mode), where eval() keeps function declarations
// local — load through new Function and return the helpers instead.
const { findUsernameInput, findUsernameOnlyInput, findSubmittedCredentials } = new Function(
    `${source}\nreturn { findUsernameInput, findUsernameOnlyInput, findSubmittedCredentials };`
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

    it('does not fall back to an unrelated field (e.g. a search box)', () => {
        form(`<input type="text" name="site-search" placeholder="Search...">`);
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
});
