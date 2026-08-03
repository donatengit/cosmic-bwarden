import { test as base, chromium, expect } from '@playwright/test';
import path from 'path';
import fs from 'fs';
import os from 'os';
import { execSync } from 'child_process';

const EXTENSION_PATH = path.resolve(__dirname, '../../../browser-extension');
const PROJECT_ROOT = path.resolve(__dirname, '../../..');
const AGENT_BIN = path.join(PROJECT_ROOT, 'target/debug/cosmic-bwarden-agent');
const CLI_BIN = path.join(PROJECT_ROOT, 'target/debug/cosmic-bwarden-cli');
const HOST_NAME = 'com.enikeev.cosmic_bwarden';
const VW_URL = process.env.VW_URL || 'http://localhost:8080';
const PASSWORD = process.env.VW_PASSWORD || 'password123';
const EMAIL = process.env.VW_EMAIL || 'test-chrome@example.com';

function cli(args) {
  return execSync(`"${CLI_BIN}" ${args}`, {
    env: { ...process.env },
    encoding: 'utf8',
    stdio: ['pipe', 'pipe', 'pipe'],
  });
}

function agentSocketPath() {
  const profile = process.env.COSMIC_BWARDEN_PROFILE
    ? `cosmic-bwarden-${process.env.COSMIC_BWARDEN_PROFILE}`
    : 'cosmic-bwarden';
  const runtimeDir = process.env.XDG_RUNTIME_DIR || `/run/user/${process.getuid()}`;
  return path.join(runtimeDir, profile, 'socket');
}

function registerNativeHost(extensionId, userDataDir) {
  const socketPath = agentSocketPath();

  const nmDirs = [
    userDataDir,
    ...['chromium', 'google-chrome', 'google-chrome-for-testing']
      .map(d => path.join(os.homedir(), '.config', d)),
  ].map(d => path.join(d, 'NativeMessagingHosts'));

  for (const nmDir of nmDirs) {
    fs.mkdirSync(nmDir, { recursive: true });
    const wrapperPath = path.join(nmDir, 'cosmic-bwarden-browser-host.sh');
    fs.writeFileSync(wrapperPath, [
      '#!/bin/bash',
      `echo "$(date) [$$] nmDir=${nmDir} native host started, args: $*" >> /tmp/native-host-debug.log`,
      // Chrome passes the calling extension's URL as a trailing arg; the agent doesn't expect it.
      `exec "${AGENT_BIN}" --socket "${socketPath}" browser-host`,
      '',
    ].join('\n'), { mode: 0o755 });
    fs.writeFileSync(path.join(nmDir, `${HOST_NAME}.json`), JSON.stringify({
      name: HOST_NAME,
      description: 'COSMIC BWarden Chrome Test Host',
      path: wrapperPath,
      type: 'stdio',
      allowed_origins: [`chrome-extension://${extensionId}/`],
    }, null, 2));
    console.log(`Wrote manifest to ${nmDir}`);
  }
}

// Inject a clipboard mock before page scripts run (navigator.clipboard is undefined in headless).
async function withClipboard(page) {
  await page.addInitScript(() => {
    let clipboardText = '';
    const mock = {
      writeText: async t => { clipboardText = t; window.__clipboardText = t; },
      readText: async () => clipboardText,
    };
    Object.defineProperty(navigator, 'clipboard', { get: () => mock, configurable: true });
  });
}

let sharedContext;
let extensionId;

const test = base.extend({
  context: async ({}, use) => { await use(sharedContext); },
  extensionId: async ({}, use) => { await use(extensionId); },
});

test.describe.configure({ mode: 'serial' });

test.describe('Chrome Extension Full E2E', () => {
  test.setTimeout(60000);

  test.beforeAll(async () => {
    const userDataDir = fs.mkdtempSync(path.join(os.tmpdir(), 'chromium-e2e-'));

    sharedContext = await chromium.launchPersistentContext(userDataDir, {
      headless: false,
      args: [
        `--disable-extensions-except=${EXTENSION_PATH}`,
        `--load-extension=${EXTENSION_PATH}`,
        '--no-sandbox',
        '--enable-logging=stderr',
        '--vmodule=native_messaging*=3,*extension*=1',
      ],
    });

    let sw = sharedContext.serviceWorkers()[0];
    if (!sw) sw = await sharedContext.waitForEvent('serviceworker', { timeout: 15000 });
    extensionId = sw.url().split('/')[2];
    console.log(`Extension ID: ${extensionId}`);
    sw.on('console', m => console.log(`[SW] ${m.type()}: ${m.text()}`));

    registerNativeHost(extensionId, userDataDir);

    try { cli(`register --server "${VW_URL}" --password "${PASSWORD}" "${EMAIL}"`); } catch (_) {}
    try { cli(`login --server "${VW_URL}" --password "${PASSWORD}" "${EMAIL}"`); } catch (_) {}
    cli(`unlock --password "${PASSWORD}"`);
  });

  test.afterAll(async () => {
    try { cli('lock'); } catch (_) {}
    const ctx = sharedContext;
    sharedContext = null;
    await ctx?.close();
  });

  // ── Popup helpers ──────────────────────────────────────────────────────────

  async function openPopup(page) {
    page.on('console', m => console.log(`[popup] ${m.text()}`));
    await page.goto(`chrome-extension://${extensionId}/popup/popup.html`);
  }

  async function openPopupWithClipboard() {
    const page = await sharedContext.newPage();
    await withClipboard(page);
    await openPopup(page);
    return page;
  }

  // ── 1. Basic vault list ────────────────────────────────────────────────────

  test('shows vault entries in popup', async () => {
    cli('add "Chrome E2E Login" username=chrome-user password=chrome-pass');
    const page = await sharedContext.newPage();
    await openPopup(page);
    await expect(page.locator('.entry-name', { hasText: 'Chrome E2E Login' }).first()).toBeVisible({ timeout: 15000 });
    await page.close();
  });

  // ── 2. Clipboard copy ─────────────────────────────────────────────────────

  test('copies password to clipboard via detail view', async () => {
    const page = await openPopupWithClipboard();
    await expect(page.locator('.entry-name', { hasText: 'Chrome E2E Login' }).first()).toBeVisible({ timeout: 10000 });
    await page.locator('.entry-name', { hasText: 'Chrome E2E Login' }).first().click();
    const copyBtn = page.locator('#view-detail button:has-text("Copy")').first();
    await expect(copyBtn).toBeVisible({ timeout: 10000 });
    await copyBtn.click();
    await page.waitForTimeout(200);
    expect(await page.evaluate(() => window.__clipboardText)).toBe('chrome-pass');
    await page.close();
  });

  // ── 3. Autofill ───────────────────────────────────────────────────────────

  test('autofills login form via content script', async () => {
    const formPage = await sharedContext.newPage();
    await formPage.goto(VW_URL);
    await formPage.evaluate(() => {
      document.body.innerHTML = '<form><input type="text" id="user"><input type="password" id="pass"></form>';
    });

    const popupPage = await sharedContext.newPage();
    await openPopup(popupPage);
    await expect(popupPage.locator('.entry-name', { hasText: 'Chrome E2E Login' }).first()).toBeVisible({ timeout: 10000 });
    await formPage.bringToFront();

    const fillBtn = popupPage.locator('.entry:has-text("Chrome E2E Login") button:has-text("Fill")').first();
    await expect(fillBtn).toBeVisible({ timeout: 5000 });
    await fillBtn.click();

    await expect(formPage.locator('#user')).toHaveValue('chrome-user', { timeout: 5000 });
    await expect(formPage.locator('#pass')).toHaveValue('chrome-pass');
    await popupPage.close();
    await formPage.close();
  });

  // ── 4. Unicode + emoji in Login fields ────────────────────────────────────

  test('handles non-latin and emoji in Login entry name and fields', async () => {
    // Cyrillic, Japanese, and emoji in the entry name and values.
    cli('add "🔐 Тест テスト Login" username="пользователь@test.com" password="Пароль123!"');

    const page = await openPopupWithClipboard();
    await expect(page.locator('.entry-name', { hasText: '🔐 Тест テスト Login' }).first()).toBeVisible({ timeout: 15000 });

    // Detail view shows username without double-encoding.
    await page.locator('.entry-name', { hasText: '🔐 Тест テスト Login' }).first().click();
    await expect(page.locator('#view-detail')).not.toHaveClass(/hidden/, { timeout: 5000 });
    await expect(page.locator('#detail-content')).toContainText('пользователь@test.com');

    // Copy password and verify correct Unicode value.
    const copyBtn = page.locator('#view-detail button:has-text("Copy")').first();
    await copyBtn.click();
    await page.waitForTimeout(200);
    expect(await page.evaluate(() => window.__clipboardText)).toBe('Пароль123!');

    await page.close();
  });

  // ── 5. Lock / unlock lifecycle ────────────────────────────────────────────

  test('lock/unlock cycle shows correct popup states', async () => {
    cli('lock');

    // Locked state: popup shows status message.
    const lockedPage = await sharedContext.newPage();
    await openPopup(lockedPage);
    await expect(lockedPage.locator('#status')).toHaveText('Vault is locked.', { timeout: 10000 });
    await lockedPage.close();

    cli(`unlock --password "${PASSWORD}"`);

    // Unlocked state: entries visible again.
    const unlockedPage = await sharedContext.newPage();
    await openPopup(unlockedPage);
    await expect(unlockedPage.locator('.entry-name').first()).toBeVisible({ timeout: 15000 });
    await unlockedPage.close();
  });

  // ── 6. Logout / login lifecycle ───────────────────────────────────────────

  test('logout/login cycle shows correct popup states', async () => {
    cli('logout');

    // Logged-out state.
    const loggedOutPage = await sharedContext.newPage();
    await openPopup(loggedOutPage);
    await expect(loggedOutPage.locator('#status')).toHaveText('Not logged in.', { timeout: 10000 });
    await loggedOutPage.close();

    cli(`login --server "${VW_URL}" --password "${PASSWORD}" "${EMAIL}"`);
    cli(`unlock --password "${PASSWORD}"`);

    // Logged-in + unlocked state: entries visible again.
    const loggedInPage = await sharedContext.newPage();
    await openPopup(loggedInPage);
    await expect(loggedInPage.locator('.entry-name').first()).toBeVisible({ timeout: 15000 });
    await loggedInPage.close();
  });

  // ── 7. Card entry (created via popup form) ────────────────────────────────

  test('creates Card entry with unicode cardholder via popup form and copies number', async () => {
    const page = await openPopupWithClipboard();
    await expect(page.locator('#add-btn')).toBeVisible({ timeout: 15000 });

    // Open add form.
    await page.locator('#add-btn').click();
    await expect(page.locator('#view-edit')).not.toHaveClass(/hidden/, { timeout: 5000 });

    // Select Card type (triggers renderDynamicFields).
    await page.locator('#edit-type').selectOption('Card');
    await expect(page.locator('#f-cardholder_name')).toBeVisible({ timeout: 3000 });

    await page.locator('#edit-name').fill('Виза Тест 💳');
    await page.locator('#f-cardholder_name').fill('Иван Иванов & テスト');
    await page.locator('#f-number').fill('4111111111111111');
    await page.locator('#f-brand').fill('Visa');
    await page.locator('#f-exp_month').fill('12');
    await page.locator('#f-exp_year').fill('2028');
    await page.locator('#f-code').fill('123');

    await page.locator('#save-btn').click();

    // Verify entry appears in list.
    await expect(page.locator('.entry-name', { hasText: 'Виза Тест 💳' })).toBeVisible({ timeout: 10000 });

    // Open detail view.
    await page.locator('.entry-name', { hasText: 'Виза Тест 💳' }).click();
    await expect(page.locator('#view-detail')).not.toHaveClass(/hidden/, { timeout: 5000 });

    // Cardholder name renders without double-encoding (& not shown as &amp;).
    await expect(page.locator('#detail-content')).toContainText('Иван Иванов & テスト');

    // Type field is populated.
    await expect(page.locator('#detail-content')).toContainText('Card');

    // Copy number.
    const copyBtn = page.locator('#view-detail button:has-text("Copy")').first();
    await expect(copyBtn).toBeVisible({ timeout: 5000 });
    await copyBtn.click();
    await page.waitForTimeout(200);
    expect(await page.evaluate(() => window.__clipboardText)).toBe('4111111111111111');

    await page.close();
  });

  // ── 8. Identity entry (created via popup form) ────────────────────────────

  test('creates Identity entry with emoji via popup form and shows all fields', async () => {
    const page = await openPopupWithClipboard();
    await expect(page.locator('#add-btn')).toBeVisible({ timeout: 15000 });

    await page.locator('#add-btn').click();
    await expect(page.locator('#view-edit')).not.toHaveClass(/hidden/, { timeout: 5000 });

    await page.locator('#edit-type').selectOption('Identity');
    await expect(page.locator('#f-first_name')).toBeVisible({ timeout: 3000 });

    await page.locator('#edit-name').fill('Identity Тест 🌍');
    await page.locator('#f-first_name').fill('Иван');
    await page.locator('#f-last_name').fill('山田 太郎');
    await page.locator('#f-email').fill('иван@example.com');
    await page.locator('#f-phone').fill('+7-999-123-4567');
    await page.locator('#f-address1').fill('ул. Тверская, 1');
    await page.locator('#f-city').fill('Москва');
    await page.locator('#f-state').fill('Московская обл.');
    await page.locator('#f-postal_code').fill('101000');
    await page.locator('#f-country').fill('Россия 🇷🇺');

    await page.locator('#save-btn').click();

    await expect(page.locator('.entry-name', { hasText: 'Identity Тест 🌍' })).toBeVisible({ timeout: 10000 });
    await page.locator('.entry-name', { hasText: 'Identity Тест 🌍' }).click();
    await expect(page.locator('#view-detail')).not.toHaveClass(/hidden/, { timeout: 5000 });

    const detail = page.locator('#detail-content');
    await expect(detail).toContainText('Иван');
    await expect(detail).toContainText('山田 太郎');
    await expect(detail).toContainText('иван@example.com');
    await expect(detail).toContainText('+7-999-123-4567');
    await expect(detail).toContainText('Москва');
    await expect(detail).toContainText('Россия 🇷🇺');
    await expect(detail).toContainText('Identity');

    await page.close();
  });

  // ── 9. SecureNote (created via CLI) ───────────────────────────────────────

  test('shows SecureNote with unicode content via CLI', async () => {
    cli('-t note add "Заметка 📝 テスト" notes="Секретная заметка. Japanese: 日本語"');

    const page = await openPopupWithClipboard();
    await expect(page.locator('.entry-name', { hasText: 'Заметка 📝 テスト' }).first()).toBeVisible({ timeout: 15000 });

    await page.locator('.entry-name', { hasText: 'Заметка 📝 テスト' }).first().click();
    await expect(page.locator('#view-detail')).not.toHaveClass(/hidden/, { timeout: 5000 });

    const detail = page.locator('#detail-content');
    // Notes field shows raw Unicode, not HTML-escaped.
    await expect(detail).toContainText('Секретная заметка');
    await expect(detail).toContainText('日本語');
    await expect(detail).toContainText('SecureNote');

    await page.close();
  });

  // ── 10. SshKey (created via CLI) ──────────────────────────────────────────

  test('shows SshKey entry with public key and copy button', async () => {
    cli('add-ssh-key "SSH Ключ 🔑" --private-key "ed25519-dummy-private-key" --public-key "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAITestKeyForE2E user@тест"');

    const page = await openPopupWithClipboard();
    await expect(page.locator('.entry-name', { hasText: 'SSH Ключ 🔑' }).first()).toBeVisible({ timeout: 15000 });

    await page.locator('.entry-name', { hasText: 'SSH Ключ 🔑' }).first().click();
    await expect(page.locator('#view-detail')).not.toHaveClass(/hidden/, { timeout: 5000 });

    const detail = page.locator('#detail-content');
    await expect(detail).toContainText('ssh-ed25519');
    await expect(detail).toContainText('SshKey');

    // Copy public key.
    const copyBtn = page.locator('#view-detail button:has-text("Copy")').first();
    await expect(copyBtn).toBeVisible({ timeout: 5000 });
    await copyBtn.click();
    await page.waitForTimeout(200);
    expect(await page.evaluate(() => window.__clipboardText)).toBe(
      'ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAITestKeyForE2E user@тест'
    );

    await page.close();
  });

  // ── 11. Edit entry via popup form ─────────────────────────────────────────

  test('edits Login entry name and username via popup form', async () => {
    // Create a dedicated entry for this test.
    cli('add "E2E Edit Target" username=before-edit password=edit-pass');

    const page = await openPopupWithClipboard();
    await expect(page.locator('.entry-name', { hasText: 'E2E Edit Target' }).first()).toBeVisible({ timeout: 15000 });

    // Open detail, then edit.
    await page.locator('.entry-name', { hasText: 'E2E Edit Target' }).first().click();
    await expect(page.locator('#view-detail')).not.toHaveClass(/hidden/, { timeout: 5000 });
    await page.locator('#edit-btn').click();
    await expect(page.locator('#view-edit')).not.toHaveClass(/hidden/, { timeout: 5000 });

    // Edit form should show Login fields (getEntryType fix).
    await expect(page.locator('#f-username')).toBeVisible({ timeout: 3000 });
    expect(await page.locator('#edit-name').inputValue()).toBe('E2E Edit Target');
    expect(await page.locator('#f-username').inputValue()).toBe('before-edit');

    await page.locator('#edit-name').fill('E2E Edited 🖊️');
    await page.locator('#f-username').fill('after-edit');
    await page.locator('#save-btn').click();

    // Back in list: updated name visible.
    await expect(page.locator('.entry-name', { hasText: 'E2E Edited 🖊️' }).first()).toBeVisible({ timeout: 10000 });

    // Detail view reflects changes.
    await page.locator('.entry-name', { hasText: 'E2E Edited 🖊️' }).first().click();
    await expect(page.locator('#detail-content')).toContainText('after-edit');

    await page.close();
  });

  // ── 12. Delete entry via popup ────────────────────────────────────────────

  test('deletes entry via popup detail view', async () => {
    cli('add "E2E Delete Target" username=delete-me password=delete-pass');

    const page = await openPopupWithClipboard();
    await expect(page.locator('.entry-name', { hasText: 'E2E Delete Target' }).first()).toBeVisible({ timeout: 15000 });

    await page.locator('.entry-name', { hasText: 'E2E Delete Target' }).first().click();
    await expect(page.locator('#view-detail')).not.toHaveClass(/hidden/, { timeout: 5000 });

    // Handle window.confirm() dialog that deleteBtn triggers.
    page.on('dialog', dialog => dialog.accept());
    await page.locator('#delete-btn').click();

    // Returns to list view with entry removed.
    await expect(page.locator('#view-list')).not.toHaveClass(/hidden/, { timeout: 5000 });
    await expect(page.locator('.entry-name', { hasText: 'E2E Delete Target' })).not.toBeVisible({ timeout: 5000 });

    await page.close();
  });

  // ── 13. Search filters entries ────────────────────────────────────────────

  test('search filters entries by name', async () => {
    // Ensure we have at least two distinctly-named entries.
    cli('add "SearchAlpha Entry" username=alpha password=alpha-pass');
    cli('add "SearchBeta Entry" username=beta password=beta-pass');

    const page = await sharedContext.newPage();
    await openPopup(page);
    await expect(page.locator('.entry-name').first()).toBeVisible({ timeout: 15000 });

    // Search for Alpha only.
    await page.locator('#search').fill('SearchAlpha');
    await page.waitForTimeout(300);
    await expect(page.locator('.entry-name', { hasText: 'SearchAlpha Entry' }).first()).toBeVisible({ timeout: 5000 });
    await expect(page.locator('.entry-name', { hasText: 'SearchBeta Entry' })).not.toBeVisible();

    // Search for Beta only.
    await page.locator('#search').fill('SearchBeta');
    await page.waitForTimeout(300);
    await expect(page.locator('.entry-name', { hasText: 'SearchBeta Entry' }).first()).toBeVisible({ timeout: 5000 });
    await expect(page.locator('.entry-name', { hasText: 'SearchAlpha Entry' })).not.toBeVisible();

    // Clear search — both visible.
    await page.locator('#search').fill('');
    await page.waitForTimeout(300);
    await expect(page.locator('.entry-name', { hasText: 'SearchAlpha Entry' }).first()).toBeVisible({ timeout: 5000 });
    await expect(page.locator('.entry-name', { hasText: 'SearchBeta Entry' }).first()).toBeVisible();

    await page.close();
  });
});
