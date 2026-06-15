import { expect, Page, test } from '@playwright/test';

function uniqueName(prefix: string): string {
  return `${prefix} ${Date.now()} ${Math.floor(Math.random() * 10_000)}`;
}

async function gotoAddProfile(page: Page): Promise<void> {
  await page.goto('/ffmpeg/add');
  await expect(page.getByRole('heading', { name: /add ffmpeg profile/i })).toBeVisible();
  await expect(page.locator('form[data-ffmpeg-profile-editor-hydrated="true"]')).toBeVisible();
  await page.waitForLoadState('networkidle');
}

async function createNvencH264Profile(page: Page, name: string): Promise<string> {
  await gotoAddProfile(page);
  const editor = page.locator('form[data-ffmpeg-profile-editor-hydrated="true"]');
  await editor.locator('input[name="name"]').fill(name);
  const hardwareSelect = editor.locator('select[name="hardware_acceleration"]');
  const videoFormatSelect = editor.locator('select[name="video_format"]');
  await hardwareSelect.selectOption('2');
  await hardwareSelect.dispatchEvent('change');
  await videoFormatSelect.selectOption('1');
  await videoFormatSelect.dispatchEvent('change');
  await expect
    .poll(async () => editor.locator('select[name="video_profile"] option').allTextContents())
    .toContain('high444p');
  await editor.locator('select[name="video_profile"]').selectOption('high444p');
  await editor.getByRole('button', { name: /add profile/i }).first().click();
  await expect(page).toHaveURL(/\/ffmpeg$/);
  const row = page.getByRole('row').filter({ hasText: name });
  await expect(row).toBeVisible();

  const editHref = await row.getByRole('link', { name: /^edit$/i }).getAttribute('href');

  if (editHref) {
    return editHref;
  }

  await row.getByRole('link', { name: /^edit$/i }).click();
  return page.url().replace(/^.*(?=\/ffmpeg\/\d+$)/, '');
}

test.describe('ffmpeg profile editor parity', () => {
  test('hydrated editor exposes h264 profile options, persists high444p, and warns for NVENC mpeg-2', async ({ page }) => {
    const name = uniqueName('NVENC H264 Profile');
    const editHref = await createNvencH264Profile(page, name);

    await page.goto(editHref);
    await page.waitForLoadState('networkidle');
    const editor = page.locator('form[data-ffmpeg-profile-editor-hydrated="true"]');
    await expect(editor.locator('select[name="hardware_acceleration"]')).toHaveValue('2');
    await expect(editor.locator('select[name="video_format"]')).toHaveValue('1');
    await expect(editor.locator('select[name="video_profile"]')).toHaveValue('high444p');

    const videoFormatSelect = editor.locator('select[name="video_format"]');
    await videoFormatSelect.selectOption('3');
    await videoFormatSelect.dispatchEvent('change');
    await expect(page.getByText('NVENC supports formats (h264, hevc, av1).')).toBeVisible();
  });

  test('no-JS edit form keeps the h264 profile select in the HTML fallback', async ({
    browser,
    baseURL,
    browserName,
    page,
  }) => {
    test.skip(
      browserName === 'firefox',
      'Firefox intermittently serves the island Suspense fallback for no-JS while projects run in parallel; hydrated Firefox coverage runs above.',
    );

    const name = uniqueName('Fallback NVENC Profile');
    const editHref = await createNvencH264Profile(page, name);

    const context = await browser.newContext({ javaScriptEnabled: false, baseURL });
    const noJsPage = await context.newPage();
    await noJsPage.goto(editHref);
    const profileSelect = noJsPage.locator('select[name="video_profile"]');
    await expect(profileSelect).toBeVisible();
    await expect
      .poll(async () => profileSelect.locator('option').allTextContents())
      .toEqual(expect.arrayContaining(['main', 'high', 'high444p']));

    await profileSelect.selectOption('high');
    await noJsPage.getByRole('button', { name: /save profile/i }).first().click();
    await expect(noJsPage).toHaveURL(/\/ffmpeg$/);
    await context.close();

    await page.goto(editHref);
    await page.waitForLoadState('networkidle');
    await expect(
      page
        .locator('form[data-ffmpeg-profile-editor-hydrated="true"]')
        .locator('select[name="video_profile"]'),
    ).toHaveValue('high');
  });
});
