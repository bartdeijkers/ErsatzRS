import { test, expect } from './fixtures';

test.describe('admin hydration smoke', () => {
  test.beforeEach(({ seedAdmin }) => {
    seedAdmin();
  });

  test('queues a local library scan from /media/libraries', async ({ page }) => {
    await page.goto('/media/libraries');

    await expect(page.getByRole('heading', { name: 'Libraries' })).toBeVisible();
    const row = page.locator('tbody tr').filter({ hasText: 'E2E Movies' });
    await expect(row).toBeVisible();
    await expect(row.getByRole('button', { name: 'Deep Scan' })).toBeHidden();

    await row.getByRole('button', { name: 'Scan' }).click();
    await expect(page.getByText('Scan queued')).toBeVisible();
  });

  test('search submits through the hydrated island', async ({ page }) => {
    await page.goto('/search');

    await page.locator('input[type="text"]').fill('test');
    await page.keyboard.press('Enter');

    await expect(page).toHaveURL(/\/search\?query=test$/);
    await expect(page.getByRole('cell', { name: 'E2E Test Movie' })).toBeVisible();
  });

  test('deletes a channel from /channels and keeps the disabled toggle live', async ({ page }) => {
    await page.goto('/channels');

    const row = page.locator('tbody tr').filter({ hasText: 'E2E Channel' });
    await expect(row).toBeVisible();

    await row.getByRole('button', { name: 'Delete channel' }).click();
    await expect(page.getByRole('heading', { name: 'Confirm Delete' })).toBeVisible();
    await expect(page.getByText('E2E Channel')).toBeVisible();
    await page.getByRole('button', { name: 'Delete' }).click();

    await expect(row).toBeHidden();

    const showDisabled = page.getByLabel('Show Disabled');
    await showDisabled.check();
    await expect(showDisabled).toBeChecked();
  });

  test('edits an existing channel from /channels/:id', async ({ page }) => {
    await page.goto('/channels/1');

    await expect(page.getByRole('heading', { name: 'Edit Channel' })).toBeVisible();
    await expect(page.locator('select[name="ffmpeg_profile_id"]')).toContainText('E2E Profile');

    await page.locator('input[name="name"]').fill('E2E Channel Renamed');
    await page.locator('select[name="streaming_mode"]').selectOption('4');
    await page.getByRole('button', { name: 'Save' }).click();

    await expect(page).toHaveURL(/\/channels$/);
    const row = page.locator('tbody tr').filter({ hasText: 'E2E Channel Renamed' });
    await expect(row).toBeVisible();
    await expect(row).toContainText('HLS Segmenter');
  });

  test('validates and creates a channel from /channels/add', async ({ page }) => {
    await page.goto('/channels/add');

    await page.getByRole('button', { name: 'Save' }).click();
    await expect(page.getByText('Number is required.')).toBeVisible();
    await expect(page.getByText('Name is required.')).toBeVisible();
    await expect(page.getByText('FFmpeg profile is required.')).toBeVisible();

    await page.locator('input[name="number"]').fill('200');
    await page.locator('input[name="name"]').fill('E2E Added Channel');
    await page.locator('select[name="ffmpeg_profile_id"]').selectOption('1');
    await page.locator('select[name="streaming_mode"]').selectOption('4');
    await page.getByRole('button', { name: 'Save' }).click();

    await expect(page).toHaveURL(/\/channels$/);
    await expect(page.locator('tbody tr').filter({ hasText: 'E2E Added Channel' })).toBeVisible();
  });

  test('deletes an FFmpeg profile from /ffmpeg/profiles', async ({ page }) => {
    await page.goto('/ffmpeg/profiles');

    const row = page.locator('tbody tr').filter({ hasText: 'E2E Profile' });
    await expect(row).toBeVisible();

    await row.getByRole('button', { name: 'Delete' }).click();
    await expect(page.getByRole('heading', { name: 'Confirm Delete' })).toBeVisible();
    await expect(page.getByText('E2E Profile')).toBeVisible();
    await page.getByRole('button', { name: 'Delete' }).click();

    await expect(row).toBeHidden();
    await expect(page.getByText('0 items')).toBeVisible();
  });

  test('toggles and saves the FFmpeg profile editor', async ({ page }) => {
    await page.goto('/ffmpeg/profiles/1');

    await expect(page.getByRole('heading', { name: 'Edit FFmpeg Profile' })).toBeVisible();
    await expect(page.locator('input[name="name"]')).toHaveValue('E2E Profile');
    await expect(page.getByText('Hardware Acceleration')).toBeHidden();

    await page.getByLabel('Normalize Video').check();
    await expect(page.getByText('Hardware Acceleration')).toBeVisible();

    await page.getByRole('button', { name: 'Save Profile' }).click();
    await expect(page).toHaveURL(/\/ffmpeg$/);
    await expect(page.locator('tbody tr').filter({ hasText: 'E2E Profile' })).toBeVisible();
  });

  test('copies an FFmpeg profile from the editor modal', async ({ page }) => {
    await page.goto('/ffmpeg/profiles/1');

    await page.getByRole('button', { name: 'Copy Profile' }).click();
    await expect(page.getByRole('heading', { name: 'Copy Profile' })).toBeVisible();

    await page.locator('input').last().fill('E2E Profile Copy');
    await page.getByRole('button', { name: 'Copy' }).click();

    await expect(page).toHaveURL(/\/ffmpeg\/\d+$/);
    await expect(page.locator('input[name="name"]')).toHaveValue('E2E Profile Copy');
  });

  test('adds and deletes a local media source from /media/sources/local', async ({
    page,
    mediaFixturePath,
  }) => {
    await page.goto('/media/sources/local');

    await page.getByRole('link', { name: '+ Add Library' }).click();
    await expect(page).toHaveURL(/\/media\/sources\/local\/add$/);

    await page.locator('#local-library-name').fill('E2E Added Library');
    await page.locator('#local-library-paths').fill(mediaFixturePath);
    await page.getByRole('button', { name: 'Add' }).click();

    await expect(page).toHaveURL(/\/media\/sources\/local$/);
    const row = page.locator('tbody tr').filter({ hasText: 'E2E Added Library' });
    await expect(row).toBeVisible();

    await row.getByRole('button', { name: 'Delete' }).click();
    await expect(page.getByRole('heading', { name: 'Confirm Delete' })).toBeVisible();
    await page.getByRole('button', { name: 'Delete' }).click();

    await expect(row).toBeHidden();
  });
});
