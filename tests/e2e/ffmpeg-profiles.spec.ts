import { expect, Page, test } from '@playwright/test';

function uniqueName(prefix: string): string {
  return `${prefix} ${Date.now()} ${Math.floor(Math.random() * 10_000)}`;
}

type SectionGaps = Record<string, number[]>;

/**
 * Measures the vertical gap between consecutive field blocks (label+control
 * wrappers) inside each titled editor section. Gaps that cross the bordered
 * Hardware Acceleration sub-panel include its padding/border and are larger
 * by design; every plain field-to-field gap must match General's rhythm.
 */
async function measureSectionGaps(page: Page): Promise<SectionGaps> {
  return page.evaluate(() => {
    const form = document.querySelector('form[data-ffmpeg-profile-editor-hydrated="true"]');
    if (!form) {
      return {};
    }
    const sections = [...form.querySelectorAll('div[class*="section"]')].filter(
      (d) => /(^|\s)section/.test(d.className) && d.querySelector('h2'),
    );
    const report: Record<string, number[]> = {};
    for (const section of sections) {
      const title = section.querySelector('h2')?.textContent?.trim() ?? '?';
      const fields = [...section.querySelectorAll(':scope div, :scope label')].filter(
        (el) =>
          el.querySelector('input,select') &&
          !el.querySelector('h2,h3') &&
          ![...el.querySelectorAll('div')].some((d) => d.querySelector('input,select')),
      );
      const boxes = fields
        .map((f) => f.getBoundingClientRect())
        .filter((b) => b.height > 0);
      const gaps: number[] = [];
      for (let i = 1; i < boxes.length; i++) {
        const gap = Math.round(boxes[i].top - boxes[i - 1].bottom);
        if (gap >= 0) {
          gaps.push(gap);
        }
      }
      report[title] = gaps;
    }
    return report;
  });
}

function dominantGap(gaps: number[]): number {
  const counts = new Map<number, number>();
  for (const gap of gaps) {
    counts.set(gap, (counts.get(gap) ?? 0) + 1);
  }
  let best = -1;
  let bestCount = 0;
  for (const [gap, count] of counts) {
    if (count > bestCount) {
      best = gap;
      bestCount = count;
    }
  }
  return best;
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
  test('Transcode Audio controls profile audio settings', async ({ page }) => {
    await gotoAddProfile(page);
    const editor = page.locator('form[data-ffmpeg-profile-editor-hydrated="true"]');
    const transcodeAudio = editor.getByRole('checkbox', { name: 'Transcode Audio' });
    const name = uniqueName('Audio Copy Profile');

    await expect(transcodeAudio).toBeChecked();
    await expect(editor.getByText('Audio Format', { exact: true })).toBeVisible();
    await transcodeAudio.uncheck();
    await expect(editor.getByText('Audio Format', { exact: true })).toBeHidden();
    await editor.locator('input[name="name"]').fill(name);
    await editor.getByRole('button', { name: /add profile/i }).first().click();
    await expect(page).toHaveURL(/\/ffmpeg$/);
    await expect(page.getByRole('row').filter({ hasText: name })).toContainText('Copy');
  });

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

  test('General, Video, and Audio sections keep a consistent vertical rhythm', async ({
    page,
  }) => {
    const name = uniqueName('Spacing Profile');
    const editHref = await createNvencH264Profile(page, name);

    for (const viewport of [
      { width: 1440, height: 900 },
      { width: 390, height: 844 },
    ]) {
      await page.setViewportSize(viewport);
      await page.goto(editHref);
      await page.waitForLoadState('networkidle');
      await expect(
        page.locator('form[data-ffmpeg-profile-editor-hydrated="true"]'),
      ).toBeVisible();

      const gaps = await measureSectionGaps(page);
      expect(Object.keys(gaps).length, 'expected titled editor sections').toBeGreaterThanOrEqual(3);

      const generalRhythm = dominantGap(gaps.General ?? []);
      expect(generalRhythm, 'General section has a measurable rhythm').toBeGreaterThan(0);

      for (const [title, sectionGaps] of Object.entries(gaps)) {
        expect(sectionGaps.length, `${title} has measurable field gaps`).toBeGreaterThan(0);
        for (const gap of sectionGaps) {
          // No collapsed rows anywhere; sub-panel crossings may exceed the rhythm.
          expect(gap, `${title} field gap at ${viewport.width}px`).toBeGreaterThanOrEqual(
            generalRhythm,
          );
        }
        expect(
          dominantGap(sectionGaps),
          `${title} dominant rhythm at ${viewport.width}px`,
        ).toBe(generalRhythm);
      }
    }
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
