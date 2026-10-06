import { expect, Page, TestInfo, test } from '@playwright/test';
import { execFileSync } from 'child_process';

const dbPath = process.env.E2E_DB_PATH ?? process.env.ERSATZRS_DB_PATH;

function sqlite(sql: string): string {
  if (!dbPath) {
    throw new Error('E2E_DB_PATH or ERSATZRS_DB_PATH is required for toast seeding');
  }
  return execFileSync('sqlite3', [dbPath], {
    input: `.timeout 10000\n${sql}`,
    encoding: 'utf8',
  }).trim();
}

function seedPlayout(testInfo: TestInfo, variant: number): string {
  const projectOffset = Array.from(testInfo.project.name).reduce(
    (total, char) => total + char.charCodeAt(0),
    0,
  ) * 100;
  const channelId = 760_000 + projectOffset + variant * 10;
  const playoutId = channelId + 1;
  const channelName = `E2E Toast Parity ${variant} ${testInfo.project.name}`;
  const profileId = sqlite('SELECT "Id" FROM "FFmpegProfile" ORDER BY "Id" LIMIT 1;');
  if (!profileId) {
    throw new Error('E2E database has no FFmpegProfile row for toast seeding');
  }

  sqlite(`
DELETE FROM "Playout" WHERE "Id" = ${playoutId};
DELETE FROM "Channel" WHERE "Id" = ${channelId};

INSERT INTO "Channel" (
  "Id", "UniqueId", "Number", "SortNumber", "Name", "Group",
  "FFmpegProfileId", "StreamingMode", "StreamSelectorMode",
  "PlayoutSource", "PlayoutMode", "TranscodeMode", "IdleBehavior",
  "MusicVideoCreditsMode", "SongVideoMode", "SubtitleMode", "IsEnabled", "ShowInEpg"
) VALUES (
  ${channelId}, '70000000-0000-0000-0000-${channelId.toString().padStart(12, '0')}',
  '${channelId}', ${-channelId}, '${channelName}', 'ErsatzRS',
  ${profileId}, 5, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1
);

INSERT INTO "Playout" ("Id", "ChannelId", "ScheduleKind", "Seed")
VALUES (${playoutId}, ${channelId}, 2, 1);
`);

  return channelName;
}

async function setUiTheme(page: Page, isDarkMode: boolean): Promise<void> {
  await page.request.post('/admin-api/settings/ui/save', {
    form: {
      is_dark_mode: isDarkMode ? 'true' : 'false',
      language: 'en-us',
    },
    maxRedirects: 0,
  });
}

async function waitForHydration(page: Page): Promise<void> {
  await expect
    .poll(
      () => page.evaluate(
        () => (window as Window & { __ersatzrs_hydrated?: boolean }).__ersatzrs_hydrated === true,
      ),
      { timeout: 15_000, message: 'playouts page did not finish hydration' },
    )
    .toBe(true);
}

async function openPlayoutsAndRefresh(page: Page, channelName: string) {
  await page.goto('/playouts');
  await waitForHydration(page);
  const row = page.getByRole('row').filter({ hasText: channelName });
  await expect(row).toBeVisible();
  await row.getByRole('button', { name: 'Update Playout' }).click();
  const dialog = page.getByRole('dialog', { name: 'Update Playout', exact: true });
  await expect(dialog.getByLabel('Action', { exact: true })).toHaveValue('refresh');
  await dialog.getByRole('button', { name: 'Queue update' }).click();
  const toast = page.locator('[data-toast-severity="success"]').filter({
    hasText: 'Playout rebuild queued',
  });
  await expect(toast).toBeVisible();
  return { row, toast };
}

test.describe('MudBlazor 8.15 toast parity', () => {
  test.skip(!dbPath, 'E2E_DB_PATH or ERSATZRS_DB_PATH is required to seed a playout');

  test('uses the dark filled top-right surface and pauses the remaining lifecycle on hover', async ({
    page,
  }, testInfo) => {
    test.setTimeout(45_000);
    const channelName = seedPlayout(testInfo, 1);
    await setUiTheme(page, true);
    const { toast } = await openPlayoutsAndRefresh(page, channelName);

    await expect(toast).toHaveAttribute('role', 'alert');
    await expect(toast).toHaveAttribute('aria-live', 'polite');
    await expect(toast.getByRole('button', { name: 'Close' })).toHaveAttribute('type', 'button');
    await expect(toast).toHaveAttribute('data-toast-phase', 'visible');
    const presentation = await toast.evaluate((element) => {
      const toastStyle = window.getComputedStyle(element);
      const stack = element.closest('[data-toast-stack]');
      const stackStyle = stack ? window.getComputedStyle(stack) : null;
      const icon = element.querySelector('[aria-hidden="true"] > span') as HTMLElement | null;
      const rect = element.getBoundingClientRect();
      return {
        background: toastStyle.backgroundColor,
        borderWidth: toastStyle.borderWidth,
        borderRadius: toastStyle.borderRadius,
        color: toastStyle.color,
        minWidth: toastStyle.minWidth,
        maxWidth: toastStyle.maxWidth,
        opacity: toastStyle.opacity,
        padding: toastStyle.padding,
        shadow: toastStyle.boxShadow,
        stackPosition: stackStyle?.position,
        stackTop: stackStyle?.top,
        stackRight: stackStyle?.right,
        iconWidth: icon ? window.getComputedStyle(icon).width : null,
        iconMask: icon ? window.getComputedStyle(icon).maskImage : null,
        contained:
          rect.left >= 0 &&
          rect.right <= window.innerWidth &&
          rect.top >= 0 &&
          rect.bottom <= window.innerHeight,
      };
    });
    expect(presentation).toMatchObject({
      background: 'rgb(11, 186, 131)',
      borderWidth: '0px',
      borderRadius: '4px',
      color: 'rgb(255, 255, 255)',
      minWidth: '288px',
      maxWidth: '500px',
      opacity: '0.95',
      padding: '6px 16px',
      stackPosition: 'fixed',
      stackTop: '24px',
      stackRight: '24px',
      iconWidth: '22px',
      contained: true,
    });
    expect(presentation.shadow).toContain('rgba(0, 0, 0, 0.2)');
    expect(presentation.iconMask).not.toBe('none');

    await toast.hover();
    await expect(toast).toHaveAttribute('data-toast-paused', 'true');
    await page.waitForTimeout(8_250);
    await expect(toast).toBeVisible();
    await page.screenshot({
      path: testInfo.outputPath('toast-parity-dark-desktop.png'),
    });

    await page.mouse.move(0, 0);
    await expect(toast).toHaveAttribute('data-toast-paused', 'false');
    await expect(toast).toHaveCount(0, { timeout: 8_500 });
  });

  test('contains the light mobile toast, pauses touch, suppresses duplicates, and fades close', async ({
    page,
  }, testInfo) => {
    test.setTimeout(35_000);
    const channelName = seedPlayout(testInfo, 2);
    await page.setViewportSize({ width: 390, height: 844 });
    await setUiTheme(page, false);
    const { row, toast } = await openPlayoutsAndRefresh(page, channelName);
    await expect(toast).toHaveAttribute('data-toast-phase', 'visible');

    const presentation = await toast.evaluate((element) => {
      const style = window.getComputedStyle(element);
      const rect = element.getBoundingClientRect();
      return {
        background: style.backgroundColor,
        opacity: style.opacity,
        left: rect.left,
        right: rect.right,
        viewportWidth: window.innerWidth,
        width: rect.width,
      };
    });
    expect(presentation.background).toBe('rgb(0, 200, 83)');
    expect(presentation.opacity).toBe('0.95');
    expect(presentation.left).toBeGreaterThanOrEqual(24);
    expect(presentation.right).toBeLessThanOrEqual(presentation.viewportWidth - 24);
    expect(presentation.width).toBeLessThanOrEqual(342);

    await toast.dispatchEvent('pointerdown', {
      pointerId: 1,
      pointerType: 'touch',
      isPrimary: true,
    });
    await expect(toast).toHaveAttribute('data-toast-paused', 'true');
    await page.waitForTimeout(8_250);
    await expect(toast).toBeVisible();
    await toast.dispatchEvent('pointerup', {
      pointerId: 1,
      pointerType: 'touch',
      isPrimary: true,
    });
    await expect(toast).toHaveAttribute('data-toast-paused', 'false');

    await row.getByRole('button', { name: 'Update Playout' }).click();
    await page.getByRole('dialog', { name: 'Update Playout', exact: true })
      .getByRole('button', { name: 'Queue update' }).click();
    await expect(page.getByText('Playout rebuild queued', { exact: true })).toHaveCount(1);
    await page.screenshot({
      path: testInfo.outputPath('toast-parity-light-mobile.png'),
    });

    await toast.getByRole('button', { name: 'Close' }).click();
    await expect(toast).toHaveAttribute('data-toast-phase', 'exiting');
    await expect(toast).toBeVisible();
    await expect(toast).toHaveCount(0, { timeout: 2_500 });
  });
});
