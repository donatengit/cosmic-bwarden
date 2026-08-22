import { describe, it, expect } from 'vitest';
import { readFileSync } from 'fs';
import { fileURLToPath } from 'url';
import path from 'path';

const source = readFileSync(
    path.join(path.dirname(fileURLToPath(import.meta.url)), 'popup.js'),
    'utf8'
);
const match = source.match(/function hostsMatchFill\([\s\S]*?\n\}/);
if (!match) throw new Error('hostsMatchFill not found');
const hostsMatchFill = new Function(`${match[0]}\nreturn hostsMatchFill;`)();

describe('hostsMatchFill', () => {
    it('matches exact hosts', () => {
        expect(hostsMatchFill('example.com', 'example.com')).toBe(true);
    });
    it('matches label-boundary subdomains', () => {
        expect(hostsMatchFill('www.example.com', 'example.com')).toBe(true);
        expect(hostsMatchFill('example.com', 'login.example.com')).toBe(true);
    });
    it('does not match sibling domains', () => {
        expect(hostsMatchFill('evil.co.uk', 'mybank.co.uk')).toBe(false);
        expect(hostsMatchFill('notexample.com', 'example.com')).toBe(false);
    });
});
