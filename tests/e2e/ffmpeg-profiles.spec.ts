import { expect, Page, test } from '@playwright/test';
import { execFileSync } from 'child_process';

const dbPath = process.env.E2E_DB_PATH ?? process.env.ERSATZRS_DB_PATH;

function sqlite(sql: string): string {
  if (!dbPath) throw new Error('E2E_DB_PATH is required for profile preservation proof');
  return execFileSync('sqlite3', [dbPath], {
    input: `.timeout 10000\n${sql}`, encoding: 'utf8',
  }).trim();
}

function nativePolicy(id: string): string {
  return sqlite(`SELECT "ThreadCount", "PadMode", "VideoProfile", "VideoPreset",
    "AllowBFrames", "VaapiDisplay", "QsvExtraHardwareFrames", "NormalizeColors"
    FROM "FFmpegProfile" WHERE "Id" = ${id};`);
}

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
      const fields = [...section.querySelectorAll(':scope div, :scope label')].filter((el) => {
        // Applicability help belongs to its field, so measure that complete block.
        if (el.matches('[data-native-engine-only]')) return true;
        if (el.closest('[data-native-engine-only]')) return false;
        return el.querySelector('input,select') && !el.querySelector('h2,h3') &&
          ![...el.querySelectorAll('div')].some((d) => d.querySelector('input,select'));
      });
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
  await waitForHydration(page);
  await page.waitForLoadState('networkidle');
}

async function waitForHydration(page: Page): Promise<void> {
  await expect
    .poll(
      () =>
        page.evaluate(
          () =>
            (window as Window & { __ersatzrs_hydrated?: boolean }).__ersatzrs_hydrated ===
            true,
        ),
      { message: 'FFmpeg profile editor hydration never completed' },
    )
    .toBe(true);
}

async function createNvencH264Profile(page: Page, name: string): Promise<string> {
  await gotoAddProfile(page);
  const editor = page.locator('form[data-ffmpeg-profile-editor-hydrated="true"]');
  await editor.locator('input[name="name"]').fill(name);
  const hardwareSelect = editor.locator('select[name="hardware_acceleration"]');
  const videoFormatSelect = editor.locator('select[name="video_format"]');
  await hardwareSelect.selectOption('2');
  await videoFormatSelect.selectOption('1');
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
  test('Transcode Video controls profile video settings', async ({ page }) => {
    await gotoAddProfile(page);
    const editor = page.locator('form[data-ffmpeg-profile-editor-hydrated="true"]');
    const transcodeVideo = editor.getByRole('checkbox', { name: 'Transcode Video' });
    const name = uniqueName('Video Copy Profile');

    await expect(transcodeVideo).toBeChecked();
    await expect(editor.getByText('Video Format', { exact: true })).toBeVisible();
    await transcodeVideo.uncheck();
    await expect(editor.getByText('Video Format', { exact: true })).toBeHidden();
    await editor.locator('input[name="name"]').fill(name);
    await editor.getByRole('button', { name: /add profile/i }).first().click();
    await expect(page).toHaveURL(/\/ffmpeg$/);
    await expect(page.getByRole('row').filter({ hasText: name })).toContainText('Copy');
  });

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
    await waitForHydration(page);
    await page.waitForLoadState('networkidle');
    const editor = page.locator('form[data-ffmpeg-profile-editor-hydrated="true"]');
    await expect(editor.locator('select[name="hardware_acceleration"]')).toHaveValue('2');
    await expect(editor.locator('select[name="video_format"]')).toHaveValue('1');
    await expect(editor.locator('select[name="video_profile"]')).toHaveValue('high444p');

    const videoFormatSelect = editor.locator('select[name="video_format"]');
    await videoFormatSelect.selectOption('3');
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
      await waitForHydration(page);
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
    const profileSelect = noJsPage.locator('noscript select[name="video_profile"]');
    await expect(profileSelect).toBeVisible();
    await expect
      .poll(async () => profileSelect.locator('option').allTextContents())
      .toEqual(expect.arrayContaining(['main', 'high', 'high444p']));

    await profileSelect.selectOption('high');
    await noJsPage.getByRole('button', { name: /save profile/i }).first().click();
    await expect(noJsPage).toHaveURL(new RegExp(`${editHref}$`));
    await context.close();

    await page.goto(editHref);
    await waitForHydration(page);
    await page.waitForLoadState('networkidle');
    await expect(
      page
        .locator('form[data-ffmpeg-profile-editor-hydrated="true"]')
        .locator('select[name="video_profile"]'),
    ).toHaveValue('high');
  });
});


async function expectApplicabilityContained(page: Page, width: number): Promise<void> {
  await page.setViewportSize({ width, height: 900 });
  for (const hint of await page.locator('[data-native-engine-only] p').all()) {
    if (!await hint.isVisible()) continue;
    const box = await hint.boundingBox();
    expect(box).not.toBeNull();
    expect(box!.x).toBeGreaterThanOrEqual(0);
    expect(box!.x + box!.width).toBeLessThanOrEqual(width);
    expect(await hint.evaluate(node => node.scrollWidth <= node.clientWidth + 1)).toBe(true);
  }
  expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(width);
}

test.describe('native-only profile guidance', () => {
  test.skip(!dbPath, 'Requires the disposable E2E database for policy preservation');

  for (const language of ['en-us', 'nl']) {
    test(`localizes applicability and retains profile policy with and without JavaScript (${language})`, async ({ page, browser, baseURL }, testInfo) => {
      test.setTimeout(60_000);
      await page.request.post('/admin-api/settings/ui/save', {
        form: { is_dark_mode: 'true', language: 'en-us' }, maxRedirects: 0,
      });
      const href = await createNvencH264Profile(page, uniqueName('Native Policy'));
      const id = href.match(/\/(\d+)$/)?.[1];
      if (!id) throw new Error('Created profile has no numeric ID');
      const noJs = await browser.newContext({ baseURL, javaScriptEnabled: false });
      const expected = language === 'nl'
        ? 'Alleen gebruikt door de native engine van ErsatzRS. ErsatzTV Next negeert deze instelling.'
        : 'Used only by the ErsatzRS native engine. ErsatzTV Next ignores this setting.';
      const save = language === 'nl' ? 'Profiel opslaan' : 'Save Profile';
      const diagnostics: string[] = [];
      page.on('pageerror', error => diagnostics.push(error.message));
      page.on('console', message => {
        if (message.type() === 'error' && /hydrat|marker|checksum/i.test(message.text())) {
          diagnostics.push(message.text());
        }
      });
      try {
        sqlite(`UPDATE "FFmpegProfile" SET "ThreadCount"=3, "NormalizeColors"=1, "AllowBFrames"=1 WHERE "Id"=${id};`);
        const baseline = nativePolicy(id);
        await page.request.post('/admin-api/settings/ui/save', {
          form: { is_dark_mode: 'true', language }, maxRedirects: 0,
        });
        await page.goto(href);
        await waitForHydration(page);
        const editor = page.locator('form[data-ffmpeg-profile-editor-hydrated="true"]');
        const fields = ['thread_count', 'pad_mode', 'video_profile', 'video_preset',
          'allow_b_frames', 'vaapi_display', 'qsv_extra_hardware_frames', 'normalize_colors'];
        await expect(editor.locator('[data-native-engine-only]')).toHaveCount(8);
        for (const field of fields) {
          await expect(editor.locator(`[data-native-engine-only="${field}"] p`)).toHaveText(expected);
        }
        await expect(editor.locator('[data-native-engine-only="thread_count"] input')).toHaveValue('3');
        await expect(editor.locator('[data-native-engine-only="normalize_colors"] input')).toBeChecked();
        await expect(editor.locator('[data-native-engine-only="allow_b_frames"] input')).toBeChecked();
        // Hidden accelerator-specific help remains mounted and becomes visible with its field.
        const hardware = editor.locator('select[name="hardware_acceleration"]');
        await hardware.selectOption('3');
        await expect(editor.locator('[data-native-engine-only="pad_mode"] p')).toBeVisible();
        await expect(editor.locator('[data-native-engine-only="vaapi_display"] p')).toBeVisible();
        for (const width of [1440, 320]) await expectApplicabilityContained(page, width);
        await hardware.selectOption('1');
        await expect(editor.locator('[data-native-engine-only="qsv_extra_hardware_frames"] p')).toBeVisible();
        for (const width of [1440, 320]) await expectApplicabilityContained(page, width);
        // Discard accelerator changes; read the original saved profile before testing a save.
        await page.goto(href);
        await waitForHydration(page);
        for (const width of [1440, 320]) await expectApplicabilityContained(page, width);
        await page.screenshot({ path: testInfo.outputPath(`native-policy-${language}-320px.png`), fullPage: true });
        await editor.getByRole('button', { name: save, exact: true }).first().click();
        await expect(page).toHaveURL(/\/ffmpeg$/);
        expect(nativePolicy(id)).toBe(baseline);
        const fallbackPage = await noJs.newPage();
        await fallbackPage.goto(href);
        const fallback = fallbackPage.locator('.ffmpeg-profile-noscript form');
        await expect(fallback.locator('[data-native-engine-only="video_profile"] p')).toHaveText(expected);
        await expect(fallback.locator('select[name="video_profile"]')).toHaveValue('high444p');
        await expectApplicabilityContained(fallbackPage, 320);
        await fallback.getByRole('button', { name: save, exact: true }).click();
        await expect(fallbackPage).toHaveURL(new RegExp(`${href}$`));
        expect(nativePolicy(id)).toBe(baseline);
        await page.goto(href);
        await waitForHydration(page);
        await expect(editor.locator('select[name="video_profile"]')).toHaveValue('high444p');
        expect(diagnostics).toEqual([]);
      } finally {
        await noJs.close();
        sqlite(`DELETE FROM "FFmpegProfile" WHERE "Id"=${id};`);
        await page.request.post('/admin-api/settings/ui/save', {
          form: { is_dark_mode: 'true', language: 'en-us' }, maxRedirects: 0,
        });
      }
    });
  }
});

test.describe('row282 profile validation and save feedback', () => {
  test.skip(!dbPath, 'Requires the disposable E2E database for saved profile proof');

  for (const language of ['en-us', 'nl']) {
    const save = language === 'nl' ? 'Profiel opslaan' : 'Save Profile';
    const add = language === 'nl' ? 'Profiel toevoegen' : 'Add Profile';
    const setLanguage = async (page: Page, value: string) => {
      await page.request.post('/admin-api/settings/ui/save', {
        form: { is_dark_mode: 'true', language: value }, maxRedirects: 0,
      });
    };

    test(`depth changes and hydrated save errors retain Native policy (${language})`, async ({ page }) => {
      test.setTimeout(60_000);
      let id: string | undefined;
      await setLanguage(page, 'en-us');
      try {
        const name = uniqueName('Depth Policy');
        await gotoAddProfile(page);
        const editor = page.locator('form[data-ffmpeg-profile-editor-hydrated="true"]');
        await editor.locator('input[name="name"]').fill(name);
        await editor.locator('select[name="video_profile"]').selectOption('main');
        await editor.locator('select[name="bit_depth"]').selectOption('1');
        await expect(editor.locator('select[name="video_profile"]')).toHaveValue('high10');
        await editor.locator('select[name="bit_depth"]').selectOption('0');
        await expect(editor.locator('select[name="video_profile"]')).toHaveValue('high');
        await expect(editor.locator('[data-native-engine-only]')).toHaveCount(8);
        await editor.getByRole('button', { name: 'Add Profile', exact: true }).first().click();
        await expect(page).toHaveURL(/\/ffmpeg$/);
        const href = await page.getByRole('row').filter({ hasText: name }).locator('a[href^="/ffmpeg/"]').first().getAttribute('href');
        id = href?.match(/\/(\d+)$/)?.[1];
        if (!id || !href) throw new Error('Created profile has no numeric ID');
        sqlite(`UPDATE "FFmpegProfile" SET "ThreadCount"=3,"NormalizeColors"=1,"AllowBFrames"=1 WHERE "Id"=${id};`);
        const baseline = nativePolicy(id);
        await setLanguage(page, language);
        await page.goto(href);
        await waitForHydration(page);
        await editor.locator('select[name="video_profile"]').selectOption('high10');
        await editor.getByRole('button', { name: save, exact: true }).first().click();
        await expect(page.getByText(language === 'nl'
          ? 'Videoprofiel mag niet high10 zijn bij 8-bit h264.'
          : 'Video Profile cannot be high10 with 8-bit h264.', { exact: false })).toBeVisible();
        expect(nativePolicy(id)).toBe(baseline);
        await expect(page).toHaveURL(new RegExp(`/ffmpeg/${id}$`));
        await editor.locator('select[name="video_profile"]').selectOption('high');
        const duplicate = sqlite('SELECT "Name" FROM "FFmpegProfile" WHERE "Id"=1;');
        await editor.locator('input[name="name"]').fill(duplicate);
        await editor.getByRole('button', { name: save, exact: true }).first().click();
        await expect(page.getByText(language === 'nl'
          ? /Er bestaat al een FFmpeg-profiel met de naam/
          : /An FFmpeg profile named/)).toBeVisible();
        await expect(editor.locator('input[name="name"]')).toHaveValue(duplicate);
        expect(nativePolicy(id)).toBe(baseline);
        expect(sqlite(`SELECT "Name" FROM "FFmpegProfile" WHERE "Id"=${id};`)).toBe(name);
      } finally {
        if (id) sqlite(`DELETE FROM "FFmpegProfile" WHERE "Id"=${id};`);
        await setLanguage(page, 'en-us');
      }
    });

    test(`no-JS Add, depth changes and actual errors preserve saved policy (${language})`, async ({ page, browser, baseURL }) => {
      test.setTimeout(60_000);
      let id: string | undefined;
      const context = await browser.newContext({ baseURL, javaScriptEnabled: false });
      await setLanguage(page, language);
      try {
        const fallbackPage = await context.newPage();
        const name = uniqueName('界'.repeat(26));
        expect(name.length).toBeLessThanOrEqual(50);
        expect(Buffer.byteLength(name, 'utf8')).toBeGreaterThan(50);
        await fallbackPage.goto('/ffmpeg/add');
        const form = fallbackPage.locator('.ffmpeg-profile-noscript form');
        await form.locator('input[name="name"]').fill(name);
        await form.locator('select[name="hardware_acceleration"]').selectOption('0');
        await form.locator('select[name="video_format"]').selectOption('1');
        await form.locator('select[name="bit_depth"]').selectOption('1');
        await form.getByRole('button', { name: add, exact: true }).click();
        await expect(fallbackPage).toHaveURL(/\/ffmpeg\/\d+$/);
        const href = new URL(fallbackPage.url()).pathname;
        id = href.match(/\/(\d+)$/)?.[1];
        if (!id) throw new Error('Fallback Add did not create a profile');
        await expect(form.locator('input[name="name"]')).toHaveValue(name);
        expect(sqlite(`SELECT "BitDepth","VideoProfile" FROM "FFmpegProfile" WHERE "Id"=${id};`)).toBe('1|high10');
        sqlite(`UPDATE "FFmpegProfile" SET "ThreadCount"=3,"NormalizeColors"=1,"AllowBFrames"=1 WHERE "Id"=${id};`);
        const expected = nativePolicy(id).split('|');
        expected[2] = 'high';
        await fallbackPage.goto(href);
        await form.locator('select[name="bit_depth"]').selectOption('0');
        await form.getByRole('button', { name: save, exact: true }).click();
        await expect(fallbackPage).toHaveURL(new RegExp(`${href}$`));
        await expect(form.locator('select[name="video_profile"]')).toHaveValue('high');
        expect(nativePolicy(id)).toBe(expected.join('|'));
        const baseline = nativePolicy(id);
        await fallbackPage.goto(href);
        await form.locator('input[name="name"]').fill(sqlite('SELECT "Name" FROM "FFmpegProfile" WHERE "Id"=1;'));
        await form.getByRole('button', { name: save, exact: true }).click();
        await expect(fallbackPage).toHaveURL(/\?error=duplicate$/);
        await expect(fallbackPage.getByRole('alert')).toContainText(language === 'nl'
          ? 'Er bestaat al een FFmpeg-profiel met deze naam.'
          : 'An FFmpeg profile with this name already exists.');
        expect(nativePolicy(id)).toBe(baseline);
        await form.locator('select[name="video_format"]').selectOption('3');
        await form.locator('select[name="bit_depth"]').selectOption('1');
        await form.getByRole('button', { name: save, exact: true }).click();
        await expect(fallbackPage).toHaveURL(/\?error=mpeg2-ten-bit$/);
        await expect(fallbackPage.getByRole('alert')).toContainText('10-bit');
        expect(nativePolicy(id)).toBe(baseline);
        await form.locator('input[name="name"]').fill('');
        await form.getByRole('button', { name: save, exact: true }).click();
        await expect(fallbackPage).toHaveURL(/\?error=name-required$/);
        await expect(fallbackPage.getByRole('alert')).toContainText(language === 'nl'
          ? 'Profielnaam is verplicht.' : 'Profile name is required.');
        expect(nativePolicy(id)).toBe(baseline);
        expect(sqlite(`SELECT "Name" FROM "FFmpegProfile" WHERE "Id"=${id};`)).toBe(name);
        // A valid saved HDR profile must retain the actual reason when an HTML
        // edit selects an incompatible codec or depth, without committing it.
        sqlite(`UPDATE "FFmpegProfile" SET "VideoFormat"=2,"BitDepth"=1,"OutputDynamicRange"=1,"VideoProfile"=NULL WHERE "Id"=${id};`);
        const hdrBaseline = sqlite(`SELECT "VideoFormat","BitDepth","OutputDynamicRange" FROM "FFmpegProfile" WHERE "Id"=${id};`);
        const hdrNative = nativePolicy(id);
        await fallbackPage.goto(href);
        await form.locator('select[name="video_format"]').selectOption('1');
        await form.getByRole('button', { name: save, exact: true }).click();
        await expect(fallbackPage).toHaveURL(/\?error=hdr-codec$/);
        await expect(fallbackPage.getByRole('alert')).toContainText('HDR10');
        await form.locator('select[name="bit_depth"]').selectOption('0');
        await form.getByRole('button', { name: save, exact: true }).click();
        await expect(fallbackPage).toHaveURL(/\?error=hdr-depth$/);
        await expect(fallbackPage.getByRole('alert')).toContainText('10-bit');
        expect(nativePolicy(id)).toBe(hdrNative);
        expect(sqlite(`SELECT "VideoFormat","BitDepth","OutputDynamicRange" FROM "FFmpegProfile" WHERE "Id"=${id};`)).toBe(hdrBaseline);
      } finally {
        await context.close();
        if (id) sqlite(`DELETE FROM "FFmpegProfile" WHERE "Id"=${id};`);
        await setLanguage(page, 'en-us');
      }
    });
  }
});
