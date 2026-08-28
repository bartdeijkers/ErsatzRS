import { expect, Page, TestInfo, test } from '@playwright/test';
import path from 'node:path';

function screenshotPath(testInfo: TestInfo, name: string): string {
  return `test-results/${name}-${testInfo.project.name}.png`;
}

async function forceEnglish(page: Page): Promise<void> {
  await page.request.post('/api/settings/ui/save', {
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

    await page.goto('/system/health');
    await expect(page.getByRole('heading', { name: 'Health Checks' })).toBeVisible();
    await expect(page.getByText('Support')).toBeVisible();
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
});
