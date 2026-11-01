// Unit tests for the host matching shared by the popup (tab vs entry) and the
// content script (frame vs entry). The content-script side decides whether a
// password may be written into a frame, so these cases are the security
// boundary, not a convenience check.
import { describe, it, expect } from 'vitest';
import { readFileSync } from 'fs';
import { fileURLToPath } from 'url';
import path from 'path';

const dir = path.dirname(fileURLToPath(import.meta.url));
const hostSource = readFileSync(path.join(dir, 'host-match.js'), 'utf8');
const { hostMatches, hostMatchesAny } = new Function(
    `${hostSource}\nreturn { hostMatches, hostMatchesAny };`
)();

// popup-fill.js is loaded with the shared matcher in scope, the way
// popup.html loads it after host-match.js.
const fillSource = readFileSync(path.join(dir, 'popup', 'popup-fill.js'), 'utf8');
const { entryMatchesTab } = new Function(
    'browser', 'showStatus',
    `${hostSource}\n${fillSource}\nreturn { entryMatchesTab };`
)({}, () => {});

describe('hostMatches', () => {
    it('matches exact hosts', () => {
        expect(hostMatches('example.com', 'example.com')).toBe(true);
    });

    it('is case-insensitive', () => {
        expect(hostMatches('Example.COM', 'example.com')).toBe(true);
    });

    it('matches label-boundary subdomains in both directions', () => {
        expect(hostMatches('www.example.com', 'example.com')).toBe(true);
        expect(hostMatches('example.com', 'login.example.com')).toBe(true);
    });

    it('does not match sibling or suffixed domains', () => {
        expect(hostMatches('evil.co.uk', 'mybank.co.uk')).toBe(false);
        expect(hostMatches('notexample.com', 'example.com')).toBe(false);
    });

    it('does not match an empty host', () => {
        expect(hostMatches('', 'example.com')).toBe(false);
        expect(hostMatches('example.com', '')).toBe(false);
    });
});

describe('hostMatchesAny', () => {
    it('matches when any entry host matches', () => {
        expect(hostMatchesAny('login.example.com', ['other.com', 'example.com'])).toBe(true);
    });

    it('refuses when no entry host matches', () => {
        expect(hostMatchesAny('evil.com', ['example.com'])).toBe(false);
    });

    // A frame whose origin cannot be named (file://, about:blank, a sandboxed
    // frame) must not receive a password stored for a real site.
    it('refuses an unnamed frame host when the entry has a host', () => {
        expect(hostMatchesAny('', ['example.com'])).toBe(false);
    });

    it('allows an unnamed frame host when the entry has no host either', () => {
        expect(hostMatchesAny('', [])).toBe(true);
    });

    it('refuses a named frame host when the entry has no host', () => {
        expect(hostMatchesAny('example.com', [])).toBe(false);
    });

    it('ignores null entries in the host list', () => {
        expect(hostMatchesAny('example.com', [null, 'example.com'])).toBe(true);
    });
});

describe('entryMatchesTab', () => {
    const login = (uris) => ({ username: 'alice', uris });

    it('matches an entry stored for the tab host', () => {
        expect(entryMatchesTab(login([{ uri: 'https://example.com' }]), 'https://example.com/login')).toBe(true);
    });

    it('matches a bare-host URI', () => {
        expect(entryMatchesTab(login([{ uri: 'example.com' }]), 'https://www.example.com/')).toBe(true);
    });

    it('refuses an entry for another site', () => {
        expect(entryMatchesTab(login([{ uri: 'https://example.com' }]), 'https://evil.com/')).toBe(false);
    });

    it('refuses a login with no URIs', () => {
        expect(entryMatchesTab(login([]), 'https://example.com/')).toBe(false);
    });
});
