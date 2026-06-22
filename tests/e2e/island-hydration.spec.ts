import { expect, test } from '@playwright/test';

/**
 * Whole-app island hydration smoke.
 *
 * We keep regressing the same failure class: a change to a shell-level island,
 * a shared primitive, the router, or a Cargo dependency aborts Leptos island
 * hydration, and every interactive control on the affected pages silently stops
 * working (the stop-transcode/delete buttons on `/channels`, the refresh and
 * row-select controls on `/playouts`, and so on). Plain `<a>` links keep
 * working, so the breakage is easy to miss until a user hits it.
 *
 * This spec visits every route that renders at least one `#[island]` and proves,
 * per route, that hydration ran to completion. The signals are:
 *
 *  1. `window.__ersatzrs_hydrated === true` — set in `crates/ersatzrs-web/src/lib.rs`
 *     right after `hydrate_islands()`. A marker-mismatch panic in any island on
 *     the page aborts before that line, so the flag stays unset.
 *  2. No uncaught page errors (wasm panics surface here).
 *  3. No hydration-signature console errors (`failed_to_cast_marker_node`, etc).
 *  4. The route did not fall through to the 404 catch-all, so the island under
 *     test is really in the DOM (`[data-not-found]` is absent).
 *
 * Only routes that hydrate without seeded fixture data are listed here. Detail
 * and editor routes that need a real `:id` (e.g. `/blocks/:id`,
 * `/media/collections/:id`, `/schedules/:id/items`) are intentionally out of
 * scope for this no-fixture smoke; cover them in their own seeded specs.
 */

type IslandRoute = {
  /** Route to load. */
  route: string;
  /** The island(s) this route is here to exercise — for failure readability. */
  islands: string;
};

const ISLAND_ROUTES: IslandRoute[] = [
  // Channels
  { route: '/channels', islands: 'ChannelsListIsland' },

  // FFmpeg profiles
  { route: '/ffmpeg/profiles', islands: 'FfmpegProfilesListIsland' },
  { route: '/ffmpeg/profiles/add', islands: 'FfmpegProfileEditorIsland' },

  // Watermarks
  { route: '/watermarks', islands: 'WatermarksListIsland' },
  { route: '/watermarks/add', islands: 'WatermarkEditorIsland' },

  // Scheduling
  { route: '/playouts', islands: 'PlayoutsIsland' },
  { route: '/playouts/add', islands: 'PlayoutEditorIsland' },
  { route: '/blocks', islands: 'BlocksIsland' },
  { route: '/decos', islands: 'DecosIsland' },
  { route: '/deco-templates', islands: 'DecoTemplatesIsland' },
  { route: '/templates', islands: 'TemplatesIsland' },
  { route: '/schedules', islands: 'SchedulesIsland' },
  { route: '/schedules/add', islands: 'ScheduleEditorIsland' },

  // Search
  { route: '/search', islands: 'SearchIsland' },

  // Settings
  { route: '/settings/ffmpeg', islands: 'FfmpegSettingsIsland' },
  { route: '/settings/logging', islands: 'LoggingSettingsIsland' },
  { route: '/settings/hdhr', islands: 'HdhrSettingsIsland' },
  { route: '/settings/scanner', islands: 'ScannerSettingsIsland' },
  { route: '/settings/playout', islands: 'PlayoutSettingsIsland' },
  { route: '/settings/ui', islands: 'UiSettingsIsland' },
  { route: '/settings/xmltv', islands: 'XmltvSettingsIsland' },

  // System
  { route: '/system/health', islands: 'HealthIsland' },
  { route: '/system/logs', islands: 'LogsIsland' },
  { route: '/system/troubleshooting', islands: 'TroubleshootingIsland' },
  { route: '/system/troubleshooting/block-playout', islands: 'BlockPlayoutTroubleshootingIsland' },

  // Media browse / lists
  { route: '/media/libraries', islands: 'LibrariesIsland' },
  { route: '/media/trash', islands: 'TrashIsland' },
  { route: '/media/movies', islands: 'MoviesBrowserIsland' },
  { route: '/media/tv/shows', islands: 'ShowsBrowserIsland' },
  { route: '/media/tv/seasons', islands: 'SeasonsListIsland' },
  { route: '/media/tv/episodes', islands: 'EpisodesListIsland' },
  { route: '/media/music/artists', islands: 'ArtistsBrowserIsland' },
  { route: '/media/music/songs', islands: 'SongsBrowserIsland' },
  { route: '/media/music/videos', islands: 'MusicVideosListIsland' },
  { route: '/media/other/videos', islands: 'OtherVideosBrowserIsland' },
  { route: '/media/images', islands: 'ImagesBrowserIsland' },
  { route: '/media/remote/streams', islands: 'RemoteStreamsIsland' },

  // Collections family
  { route: '/media/collections', islands: 'CollectionsIsland' },
  { route: '/media/collections/add', islands: 'CollectionEditorIsland' },
  { route: '/media/smart-collections', islands: 'SmartCollectionsIsland' },
  { route: '/media/smart-collections/add', islands: 'SmartCollectionEditorIsland' },
  { route: '/media/multi-collections', islands: 'MultiCollectionsIsland' },
  { route: '/media/multi-collections/add', islands: 'MultiCollectionEditorIsland' },
  { route: '/media/rerun-collections', islands: 'RerunCollectionsIsland' },
  { route: '/media/rerun-collections/add', islands: 'RerunCollectionEditorIsland' },
  { route: '/media/playlists', islands: 'PlaylistsIsland' },
  { route: '/media/trakt/lists', islands: 'TraktListsIsland' },

  // Filler presets
  { route: '/media/filler/presets', islands: 'FillerPresetsIsland' },
  { route: '/media/filler/presets/add', islands: 'FillerPresetEditorIsland' },

  // Media sources
  { route: '/media/sources/local', islands: 'LocalLibrariesIsland' },
  { route: '/media/sources/local/add', islands: 'LocalLibraryEditorIsland' },
  { route: '/media/sources/jellyfin', islands: 'JellyfinSourcesIsland' },
  { route: '/media/sources/jellyfin/edit', islands: 'JellyfinConnectionIsland' },
  { route: '/media/sources/emby', islands: 'EmbySourcesIsland' },
  { route: '/media/sources/emby/edit', islands: 'EmbyConnectionIsland' },
  { route: '/media/sources/plex', islands: 'PlexSourcesIsland / PlexSignInIsland' },
];

const HYDRATION_ERROR_SIGNATURES = [
  'failed_to_cast_marker_node',
  'panicked',
  'rust_panic',
  'unreachable executed',
  'already borrowed',
  'borrowmuterror',
  'recursive use of an object',
  'hydration',
];

// Force a deterministic (English) UI once, against the shared throwaway DB, so
// headings/labels are stable if individual specs ever assert on them.
test.beforeAll(async ({ request }) => {
  await request.post('/api/settings/ui/save', {
    form: { is_dark_mode: 'true', language: 'en-us' },
    maxRedirects: 0,
  });
});

for (const { route, islands } of ISLAND_ROUTES) {
  test(`island hydration: ${route} (${islands})`, async ({ page }) => {
    const pageErrors: string[] = [];
    const consoleErrors: string[] = [];
    const consoleLog: string[] = [];
    page.on('pageerror', (error) => pageErrors.push(error.stack ?? error.message));
    page.on('console', (message) => {
      consoleLog.push(`[${message.type()}] ${message.text()}`);
      if (message.type() === 'error') {
        consoleErrors.push(message.text());
      }
    });
    const diag = () => `\n--- console ---\n${consoleLog.join('\n')}\n--- page errors ---\n${pageErrors.join('\n')}`;

    const response = await page.goto(route);
    expect(response?.status(), `GET ${route} should return HTTP 200`).toBe(200);

    // The route resolved to a real page (not the SPA 404 catch-all), so the
    // island under test is actually rendered and expected to hydrate.
    await expect(
      page.locator('[data-not-found]'),
      `${route} fell through to the 404 catch-all`,
    ).toHaveCount(0);

    // Primary signal: island hydration ran to completion without panicking.
    await expect
      .poll(() => page.evaluate(() => (window as Window & { __ersatzrs_hydrated?: boolean }).__ersatzrs_hydrated === true), {
        timeout: 15_000,
        message: `island hydration never completed on ${route} (window.__ersatzrs_hydrated stayed unset)`,
      })
      .toBe(true);

    // Uncaught wasm panics (hydration aborts) surface as page errors.
    expect(pageErrors, `uncaught page error(s) on ${route}${diag()}`).toEqual([]);

    // Hydration-signature console errors. Unrelated resource 404s are ignored
    // on purpose so the smoke stays focused on hydration health.
    const hydrationConsoleErrors = consoleErrors.filter((text) =>
      HYDRATION_ERROR_SIGNATURES.some((signature) => text.toLowerCase().includes(signature)),
    );
    expect(hydrationConsoleErrors, `hydration console error(s) on ${route}`).toEqual([]);
  });
}
