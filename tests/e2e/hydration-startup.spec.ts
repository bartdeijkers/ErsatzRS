import { waitForHydration } from './hydration';
import { expect, test } from '@playwright/test';

// requestIdleCallback has no execution guarantee without a timeout. Model a
// browser that never grants idle time without consuming CPU or slowing I/O.
// Essential admin controls must still initialize within the existing bound.
test('admin controls initialize even when the browser grants no idle callbacks', async ({ page }) => {
  await page.addInitScript(() => {
    window.requestIdleCallback = () => 1;
    window.cancelIdleCallback = () => {};
  });
  await page.request.post('/api/settings/ui/save', {
    form: { is_dark_mode: 'true', language: 'en-us' }, maxRedirects: 0,
  });
  await page.goto('/settings/addons');
  await waitForHydration(page);

  // Prove that actual event handlers run; merely downloading WASM is insufficient.
  const settings = page.getByTestId('addon-detail-wait-settings');
  await settings.getByTestId('addon-detail-wait-minimum').fill('12');
  await settings.getByTestId('addon-detail-wait-maximum').fill('11');
  await settings.getByRole('button', { name: 'Save', exact: true }).click();
  await expect(page.locator('[data-toast-severity="error"]').filter({
    hasText: 'minimum does not exceed the maximum',
  })).toBeVisible();
});

test('document load does not report readiness while WASM initialization is pending', async ({ page }) => {
  let release!: () => void;
  const pending = new Promise<void>(resolve => { release = resolve; });
  await page.route('**/pkg/*.wasm', async route => {
    await pending;
    await route.continue();
  });
  try {
    await page.goto('/settings/addons', { waitUntil: 'load' });
    await expect.poll(() => page.evaluate(() =>
      (window as Window & { __ersatzrs_hydration_phase?: string }).__ersatzrs_hydration_phase,
    )).toBe('initializing-wasm');
    expect(await page.evaluate(() =>
      (window as Window & { __ersatzrs_hydrated?: boolean }).__ersatzrs_hydrated === true,
    )).toBe(false);
  } finally {
    release();
  }
  await waitForHydration(page);
  await expect.poll(() => page.evaluate(() =>
    (window as Window & { __ersatzrs_hydration_phase?: string }).__ersatzrs_hydration_phase,
  )).toBe('ready');
});
