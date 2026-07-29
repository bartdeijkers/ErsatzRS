import { expect, Page, test } from '@playwright/test';

type RouteExpectation = {
  path: string;
  heading: string;
  marker: string;
  englishMarker?: string;
  unit?: string;
};

type LanguageExpectation = {
  code: string;
  save: string;
  defaultSubtitleLanguage: string;
  fallbackSubtitleLanguage: string;
  routes: RouteExpectation[];
};

const languages: LanguageExpectation[] = [
  {
    code: 'en-us',
    save: 'Save Settings',
    defaultSubtitleLanguage: 'Default Subtitle Language',
    fallbackSubtitleLanguage: 'Fallback Subtitle Language',
    routes: [
      { path: '/settings/ffmpeg', heading: 'FFmpeg', marker: 'FFmpeg Path' },
      { path: '/settings/logging', heading: 'Logging', marker: 'Default Minimum Log Level' },
      { path: '/settings/hdhr', heading: 'HDHomeRun', marker: 'Tuner Count' },
      {
        path: '/settings/scanner',
        heading: 'Scanner',
        marker: 'Library Refresh Interval',
        unit: 'hours',
      },
      { path: '/settings/playout', heading: 'Playout', marker: 'Skip Missing Items' },
      { path: '/settings/ui', heading: 'UI', marker: 'Theme' },
      { path: '/settings/xmltv', heading: 'XMLTV', marker: 'XMLTV Time Zone' },
    ],
  },
  {
    code: 'pl',
    save: 'Zapisz ustawienia',
    defaultSubtitleLanguage: 'Domyślny język napisów',
    fallbackSubtitleLanguage: 'Zapasowy język napisów',
    routes: [
      {
        path: '/settings/ffmpeg',
        heading: 'FFmpeg',
        marker: 'Ścieżka FFmpeg',
        englishMarker: 'FFmpeg Path',
      },
      {
        path: '/settings/logging',
        heading: 'Dzienniki zdarzeń',
        marker: 'Domyślny minimalny poziom logów',
        englishMarker: 'Default Minimum Log Level',
      },
      {
        path: '/settings/hdhr',
        heading: 'HDHomeRun',
        marker: 'Liczba tunerów',
        englishMarker: 'Tuner Count',
      },
      {
        path: '/settings/scanner',
        heading: 'Skaner',
        marker: 'Interwał odświeżania biblioteki',
        englishMarker: 'Library Refresh Interval',
        unit: 'godz.',
      },
      {
        path: '/settings/playout',
        heading: 'Emisja',
        marker: 'Pomijaj brakujące elementy',
        englishMarker: 'Skip Missing Items',
      },
      { path: '/settings/ui', heading: 'Interfejs', marker: 'Motyw', englishMarker: 'Theme' },
      {
        path: '/settings/xmltv',
        heading: 'XMLTV',
        marker: 'Strefa czasowa XMLTV',
        englishMarker: 'XMLTV Time Zone',
      },
    ],
  },
  {
    code: 'pt-br',
    save: 'Salvar configurações',
    defaultSubtitleLanguage: 'Idioma de legenda padrão',
    fallbackSubtitleLanguage: 'Idioma de legenda alternativo',
    routes: [
      {
        path: '/settings/ffmpeg',
        heading: 'FFmpeg',
        marker: 'Caminho do FFmpeg',
        englishMarker: 'FFmpeg Path',
      },
      {
        path: '/settings/logging',
        heading: 'Logs',
        marker: 'Nível mínimo de log padrão',
        englishMarker: 'Default Minimum Log Level',
      },
      {
        path: '/settings/hdhr',
        heading: 'HDHomeRun',
        marker: 'Quantidade de sintonizadores',
        englishMarker: 'Tuner Count',
      },
      {
        path: '/settings/scanner',
        heading: 'Scanner',
        marker: 'Intervalo de atualização da biblioteca',
        englishMarker: 'Library Refresh Interval',
        unit: 'horas',
      },
      {
        path: '/settings/playout',
        heading: 'Programação',
        marker: 'Ignorar itens ausentes',
        englishMarker: 'Skip Missing Items',
      },
      { path: '/settings/ui', heading: 'Interface', marker: 'Tema', englishMarker: 'Theme' },
      {
        path: '/settings/xmltv',
        heading: 'XMLTV',
        marker: 'Fuso horário XMLTV',
        englishMarker: 'XMLTV Time Zone',
      },
    ],
  },
  {
    code: 'nl',
    save: 'Instellingen opslaan',
    defaultSubtitleLanguage: 'Standaardtaal voor ondertitels',
    fallbackSubtitleLanguage: 'Terugvaltaal voor ondertitels',
    routes: [
      {
        path: '/settings/ffmpeg',
        heading: 'FFmpeg',
        marker: 'FFmpeg-pad',
        englishMarker: 'FFmpeg Path',
      },
      {
        path: '/settings/logging',
        heading: 'Logboek',
        marker: 'Standaard minimaal logniveau',
        englishMarker: 'Default Minimum Log Level',
      },
      {
        path: '/settings/hdhr',
        heading: 'HDHomeRun',
        marker: 'Aantal tuners',
        englishMarker: 'Tuner Count',
      },
      {
        path: '/settings/scanner',
        heading: 'Scanner',
        marker: 'Vernieuwingsinterval bibliotheek',
        englishMarker: 'Library Refresh Interval',
        unit: 'uur',
      },
      {
        path: '/settings/playout',
        heading: 'Uitzending',
        marker: 'Ontbrekende items overslaan',
        englishMarker: 'Skip Missing Items',
      },
      { path: '/settings/ui', heading: 'Interface', marker: 'Thema', englishMarker: 'Theme' },
      {
        path: '/settings/xmltv',
        heading: 'XMLTV',
        marker: 'XMLTV-tijdzone',
        englishMarker: 'XMLTV Time Zone',
      },
    ],
  },
  {
    code: 'fr-fr',
    save: 'Enregistrer les paramètres',
    defaultSubtitleLanguage: 'Langue de sous-titres par défaut',
    fallbackSubtitleLanguage: 'Langue de sous-titres de secours',
    routes: [
      {
        path: '/settings/ffmpeg',
        heading: 'FFmpeg',
        marker: 'Chemin de FFmpeg',
        englishMarker: 'FFmpeg Path',
      },
      {
        path: '/settings/logging',
        heading: 'Journalisation',
        marker: 'Niveau de journalisation minimal par défaut',
        englishMarker: 'Default Minimum Log Level',
      },
      {
        path: '/settings/hdhr',
        heading: 'HDHomeRun',
        marker: 'Nombre de tuners',
        englishMarker: 'Tuner Count',
      },
      {
        path: '/settings/scanner',
        heading: 'Scanner',
        marker: "Intervalle d'actualisation des bibliothèques",
        englishMarker: 'Library Refresh Interval',
        unit: 'heures',
      },
      {
        path: '/settings/playout',
        heading: 'Diffusion',
        marker: 'Ignorer les éléments manquants',
        englishMarker: 'Skip Missing Items',
      },
      { path: '/settings/ui', heading: 'Interface', marker: 'Thème', englishMarker: 'Theme' },
      {
        path: '/settings/xmltv',
        heading: 'XMLTV',
        marker: 'Fuseau horaire XMLTV',
        englishMarker: 'XMLTV Time Zone',
      },
    ],
  },
];

async function switchLanguage(page: Page, code: string): Promise<void> {
  await page.goto('/settings/ui');
  const form = page.locator('form[action="/api/settings/ui/save"]');
  await expect(form).toBeVisible();
  await form.locator('select[name="language"]').selectOption(code);
  await form.locator('select[name="is_dark_mode"]').selectOption('true');
  await form.locator('button[type="submit"]').click();
  await expect(page).toHaveURL(/\/settings\/ui$/);
  await expect.poll(() => page.locator('html').getAttribute('lang')).toBe(code);
}

test.describe('settings i18n parity', () => {
  test('settings route family renders translated labels in every supported culture', async ({
    browserName,
    page,
  }) => {
    test.skip(
      browserName !== 'chromium',
      'settings i18n proof mutates global pages.language; Chromium coverage avoids project races',
    );

    const runtimeErrors: string[] = [];
    page.on('console', (message) => {
      if (message.type() === 'error') {
        runtimeErrors.push(message.text());
      }
    });
    page.on('pageerror', (error) => runtimeErrors.push(error.message));

    for (const language of languages) {
      await switchLanguage(page, language.code);

      for (const route of language.routes) {
        await page.goto(route.path);
        await expect(page.locator('html')).toHaveAttribute('lang', language.code);
        await expect(page.getByRole('heading', { name: route.heading }).first()).toBeVisible();
        await expect(page.getByText(route.marker, { exact: true }).first()).toBeVisible();
        if (route.unit) {
          await expect(page.getByText(route.unit, { exact: true }).first()).toBeVisible();
        }
        await expect(page.getByRole('button', { name: language.save }).first()).toBeVisible();
        await expect(page.locator('[role="alert"]')).toHaveCount(0);

        if (route.path === '/settings/ffmpeg') {
          await expect(
            page.getByText(language.defaultSubtitleLanguage, { exact: true }),
          ).toBeVisible();
          await expect(
            page.getByText(language.fallbackSubtitleLanguage, { exact: true }),
          ).toBeVisible();
          if (language.code !== 'en-us') {
            await expect(page.getByText('Default Subtitle Language', { exact: true })).toHaveCount(
              0,
            );
            await expect(page.getByText('Fallback Subtitle Language', { exact: true })).toHaveCount(
              0,
            );
          }
        }

        if (route.englishMarker) {
          await expect(page.getByText(route.englishMarker, { exact: true })).toHaveCount(0);
        }
        if (language.code !== 'en-us') {
          await expect(page.getByRole('button', { name: 'Save Settings' })).toHaveCount(0);
        }
      }
    }

    expect(runtimeErrors).toEqual([]);
  });

  test('ffmpeg subtitle language preferences persist through the HTML form', async ({
    browserName,
    page,
  }) => {
    test.skip(
      browserName !== 'chromium',
      'FFmpeg settings are global; Chromium coverage avoids repeating the mutation across projects',
    );
    await page.goto('/settings/ffmpeg');
    const form = page.locator('form[action="/api/settings/ffmpeg/save"]');
    await expect(form).toBeVisible();

    const primary = form.locator('select[name="preferred_subtitle_language_code"]');
    const fallback = form.locator('select[name="fallback_subtitle_language_code"]');
    await primary.selectOption('dut');
    await fallback.selectOption('eng');
    await form.locator('button[type="submit"]').click();
    await expect(page).toHaveURL(/\/settings\/ffmpeg$/);
    await expect(primary).toHaveValue('dut');
    await expect(fallback).toHaveValue('eng');

    await primary.selectOption('');
    await fallback.selectOption('');
    await form.locator('button[type="submit"]').click();
    await expect(page).toHaveURL(/\/settings\/ffmpeg$/);
    await expect(primary).toHaveValue('');
    await expect(fallback).toHaveValue('');
  });
});
