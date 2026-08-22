import { describe, it, expect } from 'vitest';
import { readFileSync } from 'fs';
import { fileURLToPath } from 'url';
import path from 'path';

const source = readFileSync(
    path.join(path.dirname(fileURLToPath(import.meta.url)), 'popup-list-actions.js'),
    'utf8'
);
const match = source.match(/function vaultUriToTabUrl\([\s\S]*?\n\}/);
if (!match) throw new Error('vaultUriToTabUrl not found in popup-list-actions.js');
const vaultUriToTabUrl = new Function(`${match[0]}\nreturn vaultUriToTabUrl;`)();

describe('vaultUriToTabUrl', () => {
    it('passes through https', () => {
        expect(vaultUriToTabUrl('https://example.com/login')).toBe('https://example.com/login');
    });
    it('prefixes https when there is no scheme', () => {
        expect(vaultUriToTabUrl('example.com')).toBe('https://example.com');
    });
    it('rejects javascript:', () => {
        expect(vaultUriToTabUrl('javascript:alert(1)')).toBeNull();
    });
    it('rejects data:', () => {
        expect(vaultUriToTabUrl('data:text/html,hi')).toBeNull();
    });
    it('allows http', () => {
        expect(vaultUriToTabUrl('http://localhost/login')).toBe('http://localhost/login');
    });
});
