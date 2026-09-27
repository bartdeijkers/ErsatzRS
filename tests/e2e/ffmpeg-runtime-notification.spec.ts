import { expect, Page, test } from '@playwright/test';

const INSTALLED_TAG = process.env.ERSATZRS_E2E_FFMPEG_RUNTIME_NOTIFICATION ?? '';

async function waitForHydration(page: Page): Promise<void> {
  await expect
    .poll(
      () =>
        page.evaluate(
          () =>
            (window as Window & { __ersatzrs_hydrated?: boolean })
              .__ersatzrs_hydrated === true,
        ),
      { timeout: 15_000 },
    )
    .toBe(true);
}

function runtimeToast(page: Page) {
  return page.getByText(`FFmpeg runtime downloaded: ${INSTALLED_TAG}`);
}

test.describe('FFmpeg runtime startup notification', () => {
  test('does not replay the same downloaded-runtime toast after page switches', async ({
    page,
  }) => {
    test.skip(
      INSTALLED_TAG.length === 0,
      'ERSATZRS_E2E_FFMPEG_RUNTIME_NOTIFICATION must be set before the E2E server starts',
    );

    await page.request.post('/admin-api/settings/ui/save', {
      form: { is_dark_mode: 'true', language: 'en-us' },
      maxRedirects: 0,
    });

    await page.goto('/system/health');
    await waitForHydration(page);
    await expect(runtimeToast(page)).toHaveCount(1);
    await expect
      .poll(() =>
        page.evaluate(() =>
          sessionStorage.getItem('ersatzrs.ffmpeg-runtime-notification'),
        ),
      )
      .toContain(`installed=${INSTALLED_TAG}`);

    await page.goto('/channels');
    await waitForHydration(page);
    await page.waitForLoadState('networkidle');
    await page.waitForTimeout(250);
    await expect(runtimeToast(page)).toHaveCount(0);

    await page.reload();
    await waitForHydration(page);
    await page.waitForLoadState('networkidle');
    await page.waitForTimeout(250);
    await expect(runtimeToast(page)).toHaveCount(0);
  });
});
