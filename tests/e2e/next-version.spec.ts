import { expect, test } from '@playwright/test';
import { readFileSync } from 'fs';
import { waitForHydration } from './hydration';

// Startup acquires the pinned runtime unless an override supplies it. Keep the
// expected version aligned with that pin; alternate runtimes can opt in explicitly.
const expectedVersion = process.env.ERSATZRS_E2E_NEXT_VERSION
  ?? JSON.parse(readFileSync('scripts/release/next-patches/manifest.json', 'utf8')).distribution_version;

test('Next version is cached SSR content in the responsive navigation drawer', async ({ browser }, testInfo) => {
  for (const javaScriptEnabled of [true, false]) {
    const context = await browser.newContext({
      javaScriptEnabled,
      baseURL: testInfo.project.use.baseURL,
      viewport: { width: 1280, height: 900 },
    });
    const page = await context.newPage();
    const errors: string[] = [];
    page.on('pageerror', error => errors.push(error.message));
    page.on('console', message => {
      if (message.type() === 'error') errors.push(message.text());
    });
    const saved = await page.request.post('/admin-api/settings/ui/save', {
      form: { is_dark_mode: 'true', language: 'en-us' }, maxRedirects: 0,
    });
    expect(saved.status()).toBe(303);
    await page.goto('/channels');
    await expect(page.locator('[data-not-found]')).toHaveCount(0);
    const version = page.locator('[data-next-version]');
    await expect(version).toHaveText(`Next engine version${expectedVersion}`);
    await expect(version).toBeVisible();
    expect(await version.evaluate(element => getComputedStyle(element).flexDirection)).toBe('column');
    if (javaScriptEnabled) {
      await waitForHydration(page);
      await page.setViewportSize({ width: 390, height: 844 });
      await page.reload();
      await waitForHydration(page);
      const toggle = page.locator('[data-nav-drawer-toggle]');
      await toggle.click();
      await expect(toggle).toHaveAttribute('aria-expanded', 'true');
      await version.scrollIntoViewIfNeeded();
      await expect(version).toBeInViewport();
      expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(390);
      await page.screenshot({ path: testInfo.outputPath('next-version-mobile.png') });
      await page.locator('#admin-navigation-drawer a[href="/settings/ffmpeg"]').click();
      await expect(page).toHaveURL(/\/settings\/ffmpeg$/);
      await waitForHydration(page);
      await expect(version).toHaveText(`Next engine version${expectedVersion}`);
    }
    expect(errors).toEqual([]);
    await context.close();
  }
});
