import { defineConfig, devices } from '@playwright/test';
import path from 'path';

export default defineConfig({
  testDir: '.',
  fullyParallel: false,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 2 : 0,
  workers: 1,
  reporter: 'list',
  use: {
    trace: 'on-first-retry',
  },
  projects: [
    {
      name: 'chrome-full',
      testMatch: /.*chrome-full\.spec\.js/,
      use: {
        channel: 'chromium',
      },
    },
    {
      name: 'firefox-mock',
      testIgnore: /.*full\.spec\.js|.*chrome-full\.spec\.js/,
      use: {
        ...devices['Desktop Firefox'],
        headless: false,
      },
    },
    {
      name: 'firefox-full',
      testMatch: /.*full\.spec\.js/,
      use: {
        ...devices['Desktop Firefox'],
        headless: false,
        launchOptions: {
          args: ['--start-debugger-server', '12345'],
          firefoxUserPrefs: {
            'devtools.debugger.remote-enabled': true,
            'devtools.debugger.prompt-connection': false,
            'extensions.autoDisableScopes': 0,
            'extensions.enabledScopes': 15,
            'xpinstall.signatures.required': false,
            'browser.disable_welcome_page': true,
            'browser.shell.checkDefaultBrowser': false,
            // Set fixed UUID for the extension to avoid about:debugging
            'extensions.webextensions.uuids': '{"cosmic-bwarden@enikeev.com":"e2e-test-uuid-1234"}',
          },
        },
      },
    },
  ],
});
