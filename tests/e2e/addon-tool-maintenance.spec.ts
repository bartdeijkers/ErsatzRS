import { expect, test } from '@playwright/test';
import { DatabaseSync } from 'node:sqlite';
import { randomUUID } from 'node:crypto';

const databasePath = process.env.E2E_DB_PATH ?? process.env.ERSATZRS_DB_PATH;
test.skip(!databasePath, 'An isolated E2E database is required.');
test.describe.configure({ mode: 'serial' });

for (const javaScriptEnabled of [true, false]) {
  test(`latest tool maintenance and permission review (${javaScriptEnabled ? 'hydrated' : 'no JS'})`, async ({ browser, baseURL }) => {
    const addonId = `org.example.maintenance.${randomUUID().replace(/-/g, '')}`;
    const db = new DatabaseSync(databasePath!);
    db.exec('PRAGMA busy_timeout = 10000');
    // Disabled synthetic package: rendering and permission review cannot cause
    // automatic maintenance or invoke a real installed executable.
    const manifest = {
      manifest_version: 6, id: addonId, version: '1.0.0', host_version: '*',
      name: { 'en-US': 'Synthetic tool maintenance' }, summary: { 'en-US': 'Read-only maintenance fixture' },
      license: 'MIT', source_url: 'https://example.invalid/maintenance',
      capabilities: [{ id: 'addon.check.v1' }, { id: 'tool.update.v1' }],
      entrypoints: { any: { path: 'unused.sh', kind: 'posix_shell' } }, dependencies: [], panels: [],
      permissions: { network_hosts: [], media_read: false, managed_ffmpeg: false, external_commands: [], tool_update: ['YT_DLP_BIN'] },
      settings: [{ key: 'YT_DLP_BIN', kind: 'executable', label: { 'en-US': 'yt-dlp executable' }, required: true, default: 'synthetic-unused-tool', options: [] }],
    };
    db.prepare(`INSERT INTO "AddonInstallation" ("AddonId", "Version", "Enabled", "PackageSha256", "ContentSha256", "ManifestJson", "InstallPath", "InstalledAt", "UpdatedAt", "TrustTier")
      VALUES (?, '1.0.0', 0, 'synthetic', 'synthetic', ?, 'synthetic-unused-installation', '2026-09-01T00:00:00Z', '2026-09-01T00:00:00Z', 'verified')`).run(addonId, JSON.stringify(manifest));
    const insert = db.prepare(`INSERT INTO "AddonToolMaintenanceJob" ("AddonId", "ToolKey", "Trigger", "State", "Outcome", "DiagnosticCode", "Diagnostic", "BeforeVersion", "AfterVersion", "CreatedAt", "UpdatedAt") VALUES (?, 'YT_DLP_BIN', 0, 3, ?, ?, 'SYNTHETIC_PRIVATE_DIAGNOSTIC', '2026.09.01', '2026.09.08', ?, ?)`);
    insert.run(addonId, 2, 'unsupported_installation', '2026-09-01T00:00:00Z', '2026-09-01T00:00:00Z');
    const latest = insert.run(addonId, 0, null, '2026-09-08T00:00:00Z', '2026-09-08T00:00:00Z').lastInsertRowid;
    const context = await browser.newContext({ baseURL, javaScriptEnabled });
    const page = await context.newPage();
    const errors: string[] = [];
    page.on('pageerror', error => errors.push(error.message));
    page.on('console', message => {
      if (message.type() === 'error' && /hydration|failed_to_cast_marker_node|rust_panic/i.test(message.text())) errors.push(message.text());
    });
    try {
      await page.request.post('/api/settings/ui/save', { form: { language: 'en-us', is_dark_mode: 'true' }, maxRedirects: 0 });
      const visit = async () => {
        await page.goto(`/settings/addons/${addonId}`);
        await expect(page.locator('[data-not-found]')).toHaveCount(0);
        await expect(page.getByTestId('addon-tool-maintenance')).toBeVisible();
        if (javaScriptEnabled) await expect.poll(() => page.evaluate(() => window.__ersatzrs_hydrated === true)).toBe(true);
      };
      await visit();
      await expect(page.getByTestId('addon-tool-maintenance-status')).toHaveText('Updated');
      await expect(page.getByTestId('addon-tool-maintenance').locator('article')).toHaveCount(1);
      await expect(page.getByTestId('addon-tool-maintenance')).toContainText('2026.09.01 → 2026.09.08');
      await expect(page.locator('#overview').getByTestId('addon-tool-update-permission')).toContainText('YT_DLP_BIN');
      expect(await page.content()).not.toContain('SYNTHETIC_PRIVATE_DIAGNOSTIC');
      if (javaScriptEnabled) {
        await page.getByRole('button', { name: 'Enable', exact: true }).click();
        await expect(page.locator('#addon-toggle-dialog').getByTestId('addon-tool-update-permission')).toBeVisible();
        await expect(page.locator('#addon-toggle-dialog').getByTestId('addon-tool-update-permission')).toContainText('YT_DLP_BIN');
      }
      for (const [state, outcome, code, status, help] of [
        [3, 1, null, 'Already current', null],
        [3, 2, 'unsupported_installation', 'Manual update required', 'original installation method'],
        [4, 4, 'update_failed', 'Failed', 'automatic maintenance will retry later'],
        [2, null, 'tool-busy', 'Waiting to retry', 'active playback is not interrupted'],
        [2, null, 'tool-cleanup-required', 'Waiting to retry', 'offline recovery command'],
      ] as const) {
        db.prepare('UPDATE "AddonToolMaintenanceJob" SET "State" = ?, "Outcome" = ?, "DiagnosticCode" = ? WHERE "Id" = ?').run(state, outcome, code, latest);
        await visit();
        await expect(page.getByTestId('addon-tool-maintenance-status')).toHaveText(status);
        if (help) await expect(page.getByTestId('addon-tool-maintenance-remediation')).toContainText(help);
        if (code === 'tool-cleanup-required') {
          await expect(page.getByTestId('addon-tool-maintenance-remediation')).toContainText('Stop the server, scanner, and all related provider and tool processes');
          await expect(page.getByTestId('addon-tool-maintenance-remediation')).toContainText('does not repair the executable');
          await expect(page.getByTestId('addon-tool-offline-recovery-command')).toContainText('--recover-addon-tool --tool-path "<absolute-executable-path>" --confirm-all-processes-stopped');
        }
      }
      db.prepare('UPDATE "AddonInstallation" SET "LastCheckStatus" = ?, "LastCheckCode" = ?, "LastCheckMessage" = ? WHERE "AddonId" = ?')
        .run('warning', 'tool-update-failed-stale', 'SYNTHETIC_PRIVATE_PROVIDER_PROSE', addonId);
      await visit();
      await expect(page.locator('#status')).toContainText('The program could not be updated and a newer release is available.');
      await expect(page.locator('#status')).toContainText('original installation method');
      expect(await page.content()).not.toContain('SYNTHETIC_PRIVATE_PROVIDER_PROSE');
      db.prepare('UPDATE "AddonInstallation" SET "LastCheckToolUpdateJson" = ? WHERE "AddonId" = ?')
        .run(JSON.stringify({ program: 'Example Player', installed_version: '1.0', latest_version: '1.2' }), addonId);
      await visit();
      await expect(page.locator('#status')).toContainText('Example Player: installed 1.0, latest 1.2.');
      await expect(page.locator('#status')).toContainText('Automatic maintenance will retry.');
      expect(await page.content()).not.toContain('SYNTHETIC_PRIVATE_PROVIDER_PROSE');
      db.prepare('UPDATE "AddonInstallation" SET "LastCheckStatus" = ?, "LastCheckCode" = NULL, "LastCheckMessage" = ?, "LastCheckToolUpdateJson" = NULL WHERE "AddonId" = ?')
        .run('ready', 'Synthetic add-on ready', addonId);
      await visit();
      await expect(page.locator('#status')).toContainText('Synthetic add-on ready');
      await expect(page.locator('#status')).not.toContainText('a newer release is available');
      expect(db.prepare('SELECT "Enabled" FROM "AddonInstallation" WHERE "AddonId" = ?').get(addonId)?.Enabled).toBe(0);
      expect(db.prepare('SELECT COUNT(*) AS n FROM "AddonToolMaintenanceJob" WHERE "AddonId" = ?').get(addonId)?.n).toBe(2);
      expect(errors).toEqual([]);
    } finally {
      await context.close();
      db.prepare('DELETE FROM "AddonToolMaintenanceJob" WHERE "AddonId" = ?').run(addonId);
      db.prepare('DELETE FROM "AddonInstallation" WHERE "AddonId" = ?').run(addonId);
      db.close();
    }
  });
}
