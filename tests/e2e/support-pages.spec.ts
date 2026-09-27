import { expect, Page, TestInfo, test } from '@playwright/test';
import path from 'node:path';

function screenshotPath(testInfo: TestInfo, name: string): string {
  return `test-results/${name}-${testInfo.project.name}.png`;
}

async function forceEnglish(page: Page): Promise<void> {
  await page.request.post('/admin-api/settings/ui/save', {
    form: {
      is_dark_mode: 'true',
      language: 'en-us',
    },
    maxRedirects: 0,
  });
}

// Error surfaces are `role="alert"`, but so are toasts. Benign info/success
// toasts are not page errors and can appear unbidden - the startup "FFmpeg
// runtime downloaded" notification fires once per browser session whenever the
// host acquired a managed runtime instead of using an operator override. Keep
// failing on error/warning toasts and on any non-toast alert.
const ERROR_SURFACE_SELECTOR =
  '[role="alert"]:not([data-toast-id]), [role="alert"][data-toast-severity="error"], [role="alert"][data-toast-severity="warning"]';

async function expectNoRuntimeErrors(page: Page, runtimeErrors: string[]): Promise<void> {
  await expect(page.locator(ERROR_SURFACE_SELECTOR)).toHaveCount(0);
  expect(runtimeErrors).toEqual([]);
}

test.describe('support route-family parity', () => {
  for (const javaScriptEnabled of [true, false]) {
    test(`General diagnostics expose detected runtime versions (JavaScript ${javaScriptEnabled})`, async ({ browser, baseURL }) => {
      const context = await browser.newContext({ baseURL, javaScriptEnabled });
      try {
        const page = await context.newPage();
        await forceEnglish(page);
        await page.goto('/system/troubleshooting#general-panel');
        const report = page.locator('#general-panel code');
        await expect(report).toBeVisible();
        await expect(report).toContainText('FFmpegVersion');
        const info = JSON.parse(await report.innerText());
        expect(info.Version).toMatch(/^\d+\.\d+\.\d+/);
        expect(info.FFmpegVersion).toMatch(/^9\.\d+\.\d+$/);
        const next = (await page.locator('[data-next-version] span').last().innerText()).trim();
        expect(info.NextVersion).toBe(next === 'Not available' ? 'unknown' : next);
      } finally {
        await context.close();
      }
    });
  }

  test('release notes fit the home and health routes at 320px', async ({ page }, testInfo) => {
    test.skip(testInfo.project.name !== 'chromium', 'Capture the narrow layout once.');
    await forceEnglish(page);
    await page.setViewportSize({ width: 320, height: 720 });

    for (const route of ['/', '/system/health']) {
      await page.goto(route);
      const card = page.locator('[data-release-notes]');
      await card.scrollIntoViewIfNeeded();
      await expect(card).toBeVisible();
      const box = await card.boundingBox();
      expect(box).not.toBeNull();
      expect(box!.x).toBeGreaterThanOrEqual(0);
      expect(box!.x + box!.width).toBeLessThanOrEqual(320);
      await page.screenshot({
        path: screenshotPath(testInfo, route === '/' ? 'home-release-notes' : 'health-release-notes'),
      });
    }
  });

  test('health, logs, troubleshooting, validation, block playout, and playback fallbacks render hydrated controls', async ({
    page,
  }, testInfo) => {
    test.skip(
      testInfo.project.name !== 'chromium',
      'This route-family proof captures screenshots once to avoid cross-project app-state churn.',
    );

    const runtimeErrors: string[] = [];
    page.on('console', (message) => {
      if (message.type() === 'error') {
        runtimeErrors.push(message.text());
      }
    });
    page.on('pageerror', (error) => runtimeErrors.push(error.message));
    await forceEnglish(page);

    await page.goto('/');
    await expect(page.locator('[data-release-notes]')).toHaveCount(1);
    await expect(page.locator('[data-release-notes] a').first()).toHaveAttribute('href', '/changelog.md');
    const changelog = await page.request.get('/changelog.md');
    expect(changelog.ok()).toBe(true);
    expect(changelog.headers()['content-type']).toContain('text/markdown');
    const changelogText = await changelog.text();
    expect(changelogText).toContain('## [Unreleased]');
    const latestRelease = changelogText.match(/^## \[([^\]]+)\] - /m)?.[1];
    expect(latestRelease).toBeDefined();
    await expect(page.locator('[data-release-notes]')).toContainText(latestRelease!);

    await page.goto('/system/health');
    await expect(page.getByRole('heading', { name: 'Health Checks' })).toBeVisible();
    await expect(page.locator('[data-release-notes]')).toHaveCount(1);
    await expect(page.locator('[data-release-notes]')).toContainText(latestRelease!);
    // Scoped to the drawer: health-check rows can contain the word "unsupported",
    // which an unscoped text match also resolves to.
    await expect(
      page.locator('#admin-navigation-drawer a[href="/system/health"]'),
    ).toContainText('Support');
    await expect(page.locator('[data-health-error-badge]')).toHaveCount(1);
    await expect(page.locator('[data-health-warning-badge]')).toHaveCount(1);
    await page.screenshot({ path: screenshotPath(testInfo, 'support-health'), fullPage: true });

    await page.goto('/system/logs');
    await expect(page.getByRole('heading', { name: 'Logs' })).toBeVisible();
    await expect(page.locator('#support-log-search')).toBeVisible();
    await expect(page.getByRole('button', { name: 'Sort by Timestamp' })).toBeVisible();
    await expect(page.getByRole('button', { name: 'Sort by Level' })).toBeVisible();
    await page.getByRole('button', { name: 'Sort by Level' }).click();
    await page.locator('#support-log-search').fill('warn');
    await page.getByRole('combobox', { name: 'Rows per page' }).selectOption('25');
    await expect(page.getByRole('button', { name: 'Previous page' })).toBeVisible();
    await expect(page.getByRole('button', { name: 'Next page' })).toBeVisible();
    await expect(page.locator('[data-pagination]')).toBeVisible();
    await expect(page.locator('body')).not.toContainText('last_page on');
    await expect(page.locator('body')).not.toContainText('u32::try_from');
    await page.screenshot({ path: screenshotPath(testInfo, 'support-logs'), fullPage: true });

    await page.goto('/system/troubleshooting');
    await expect(page.getByRole('heading', { name: 'Troubleshooting' })).toBeVisible();
    await expect(page.getByRole('tab', { name: 'Tools' })).toBeVisible();
    await expect(page.getByRole('tab', { name: 'General' })).toBeVisible();
    await expect(
      page.getByRole('link', { name: 'Sequential Schedule Validation' }),
    ).toBeVisible();
    await expect(page.getByRole('link', { name: 'Block Playout Troubleshooting' })).toBeVisible();
    await page.screenshot({
      path: screenshotPath(testInfo, 'support-troubleshooting'),
      fullPage: true,
    });

    const sequentialFixture = path.join(process.cwd(), 'tests/fixtures/sequential-schedule-valid.yml');
    await page.goto(
      `/system/troubleshooting/sequential-schedule?yamlFile=${encodeURIComponent(sequentialFixture)}`,
    );
    await expect(
      page.getByRole('heading', { name: 'Sequential Schedule Validation' }),
    ).toBeVisible();
    await expect(page.getByLabel('YAML File')).toHaveValue(sequentialFixture);
    await expect(page.getByLabel('Schema')).toBeVisible();
    await expect(page.getByRole('button', { name: 'Validate' })).toBeVisible();
    await expect(page.getByRole('tab', { name: 'YAML' })).toBeVisible();
    await expect(page.getByRole('tab', { name: 'JSON' })).toBeVisible();
    await expect(page.getByRole('tab', { name: 'Messages' })).toBeVisible();

    await page.goto('/system/troubleshooting/block-playout');
    await expect(page.getByRole('heading', { name: 'Block Playout Troubleshooting' })).toBeVisible();
    await expect(page.getByLabel('Block Playout')).toBeVisible();
    await expect(page.getByLabel('Search for blocks')).toBeVisible();
    await expect(page.getByText('Select a block playout to inspect its blocks.')).toBeVisible();

    await page.goto('/system/troubleshooting/playback');
    await expect(page.getByText('Missing mediaItem query parameter')).toBeVisible();

    await expectNoRuntimeErrors(page, runtimeErrors);
  });

  // ADR 0089: the drawer exposes one badged Support destination and the three
  // support pages navigate through in-page route tabs, mirroring Settings.
  test('one drawer Support link drives the system route tabs', async ({ page }, testInfo) => {
    test.skip(
      testInfo.project.name !== 'chromium',
      'The navigation contract is language-independent; Chromium avoids repeating the global language write.',
    );

    await forceEnglish(page);
    await page.goto('/system/health');

    // One Support destination. The link is located by href, not by accessible
    // name: the health badge count is part of the name whenever it is showing.
    const drawer = page.locator('#admin-navigation-drawer');
    const supportLink = drawer.locator('a[href="/system/health"]');
    await expect(drawer.locator('a[href^="/system/"]')).toHaveCount(1);
    await expect(supportLink).toContainText('Support');
    // The health badge rides on the drawer link, not on a group header.
    await expect(supportLink.locator('[data-health-badges]')).toHaveCount(1);

    const tabs = page.locator('[data-system-route-tabs]');
    await expect(tabs.getByRole('link')).toHaveCount(3);
    await expect(tabs.getByRole('link', { name: 'Health Checks', exact: true })).toHaveAttribute(
      'aria-current',
      'page',
    );

    await tabs.getByRole('link', { name: 'Logs', exact: true }).click();
    await expect(page).toHaveURL(/\/system\/logs$/);
    await expect(
      page.locator('[data-system-route-tabs]').getByRole('link', { name: 'Logs', exact: true }),
    ).toHaveAttribute('aria-current', 'page');
    await expect(supportLink).toHaveClass(/nav-link-active/);

    // Detail routes keep the tab strip and hold the Troubleshooting tab active.
    await page.goto('/system/troubleshooting/block-playout');
    await expect(
      page
        .locator('[data-system-route-tabs]')
        .getByRole('link', { name: 'Troubleshooting', exact: true }),
    ).toHaveAttribute('aria-current', 'page');
  });
});
