import { expect, test } from '@playwright/test';
import { spawn, execFileSync, type ChildProcess } from 'child_process';
import { closeSync, cpSync, existsSync, mkdirSync, mkdtempSync, openSync, readFileSync } from 'fs';
import { createServer } from 'net';
import { dirname, join } from 'path';
import { addonId, sourceUrl, hydrated, installPreparedFixture, preparedSqlite } from './addon-prepared-fixture';

function alive(pid: number): boolean {
  try { process.kill(pid, 0); return true; }
  catch (error) {
    if ((error as NodeJS.ErrnoException).code === 'ESRCH') return false;
    throw error;
  }
}

function workerPid(hostPid: number): number {
  if (process.platform === 'win32') {
    const result = execFileSync('powershell.exe', ['-NoLogo', '-NoProfile', '-Command',
      `Get-CimInstance Win32_Process -Filter "ParentProcessId = ${hostPid}" | Where-Object { $_.CommandLine -like '*--internal-addon-preparation-worker*' } | Select-Object -ExpandProperty ProcessId`],
    { encoding: 'utf8', timeout: 10000 }).trim();
    return Number(result);
  }
  const rows = execFileSync('ps', ['-axo', 'pid=,ppid=,args='], { encoding: 'utf8', timeout: 10000 });
  const match = rows.split('\n').map(line => line.trim().match(/^(\d+)\s+(\d+)\s+(.*)$/))
    .find(row => row && Number(row[2]) === hostPid && row[3].includes('--internal-addon-preparation-worker'));
  return match ? Number(match[1]) : 0;
}

async function unusedPort(): Promise<number> {
  const listener = createServer();
  await new Promise<void>((resolve, reject) => {
    listener.once('error', reject);
    listener.listen(0, '127.0.0.1', resolve);
  });
  const address = listener.address();
  if (!address || typeof address === 'string') throw new Error('No loopback port');
  await new Promise<void>((resolve, reject) => listener.close(error => error ? reject(error) : resolve()));
  return address.port;
}

test('actual host death drains preparation before one safe restart', async ({ browser, request }, info) => {
  test.setTimeout(300000);
  const server = process.env.E2E_SERVER_BIN;
  const site = process.env.E2E_SITE_ROOT;
  const wrapperDb = process.env.E2E_DB_PATH;
  test.skip(info.project.name !== 'chromium', 'One isolated transport acceptance is sufficient');
  test.skip(!server || !site || !wrapperDb, 'Requires the isolated playwright-e2e.sh host exports');
  const ffmpeg = process.env.ErsatzRS_FFMPEG__PATH ?? process.env.ERSATZRS_FFMPEG__PATH ?? process.env.FFMPEG_PATH;
  if (!ffmpeg || !existsSync(ffmpeg)) throw new Error('Provide the wrapper managed FFmpeg path; this fixture must not download a runtime');
  const ffprobe = process.env.ErsatzRS_FFPROBE__PATH ?? process.env.ERSATZRS_FFPROBE__PATH ?? process.env.FFPROBE_PATH
    ?? join(dirname(ffmpeg), process.platform === 'win32' ? 'ffprobe.exe' : 'ffprobe');
  if (!existsSync(ffprobe)) throw new Error('The matching managed ffprobe is required');
  // Never share the wrapper server/database. Failure evidence stays inside its disposable root.
  const root = mkdtempSync(join(dirname(wrapperDb!), 'preparation-restart-'));
  const database = join(root, 'restart.db');
  const state = join(root, 'state');
  const data = join(root, 'data');
  const home = join(root, 'home');
  for (const path of [state, data, home]) mkdirSync(path);
  const sideSite = join(root, 'site');
  cpSync(site!, sideSite, { recursive: true });
  const port = await unusedPort();
  const baseURL = `http://127.0.0.1:${port}`;
  const sql = (query: string): string => preparedSqlite(database, query);
  const context = await browser.newContext({ baseURL });
  const page = await context.newPage();
  let host: ChildProcess | undefined;
  const workers: number[] = [];
  let startCount = 0;
  const start = async (): Promise<ChildProcess> => {
    const log = openSync(join(root, `host-${++startCount}.log`), 'a');
    const child = spawn(server!, [], {
      cwd: process.cwd(), windowsHide: true,
      env: { ...process.env, HOME: home, USERPROFILE: home,
        ErsatzRS_FFMPEG__PATH: ffmpeg, ErsatzRS_FFPROBE__PATH: ffprobe,
        DATABASE_URL: `sqlite:${database}`, ErsatzRS_DATABASE__URL: `sqlite:${database}`,
        ERSATZRS_DATABASE__URL: `sqlite:${database}`, ERSATZRS_APP_STATE_DIR: state,
        ERSATZRS_APP_DATA_DIR: data, LEPTOS_SITE_ROOT: sideSite, LEPTOS_OUTPUT_NAME: 'ersatzrs-web',
        LEPTOS_SITE_ADDR: `127.0.0.1:${port}` },
      stdio: ['ignore', log, log],
    });
    closeSync(log);
    host = child;
    let spawnError: Error | undefined;
    child.on('error', error => { spawnError = error; });
    await expect.poll(async () => {
      if (spawnError) throw spawnError;
      if (child.exitCode !== null || child.signalCode !== null) throw new Error('Owned side host exited; inspect its log');
      try { return (await request.get(`${baseURL}/healthz`, { timeout: 1000 })).ok(); }
      catch { return false; }
    }, { timeout: 90000 }).toBe(true);
    let worker = 0;
    await expect.poll(() => { worker = workerPid(child.pid!); return worker; }, { timeout: 15000 }).toBeGreaterThan(0);
    workers.push(worker);
    return child;
  };
  try {
    await start();
    // Normal bootstrap supplies required defaults; the wrapper does not run seed_admin.sql either.
    const fixture = installPreparedFixture(database, true);
    const calls = (): Array<{ command: string; provider?: number; descendant?: number }> =>
      existsSync(fixture.invocationLog) ? readFileSync(fixture.invocationLog, 'utf8').trim()
        .split(/\r?\n/).filter(Boolean).map(line => JSON.parse(line)) : [];
    await page.request.post('/api/settings/ui/save', { form: { is_dark_mode: 'true', language: 'en-us' }, maxRedirects: 0 });
    await hydrated(page, '/media/addon-lists/add');
    await page.getByLabel('Add-on list URL').fill(sourceUrl);
    await page.getByLabel('Add-on', { exact: true }).selectOption(addonId);
    await page.getByLabel('Delivery', { exact: true }).selectOption('download_ahead');
    await page.getByLabel('Collection Type', { exact: true }).selectOption('remote_streams');
    await page.getByLabel('Storage folder').fill('prepared-restart');
    await page.getByRole('combobox', { name: 'Automatic Refresh' }).selectOption('never');
    await page.getByRole('checkbox', { name: 'Create playlist in original order' }).check();
    await page.getByRole('checkbox', { name: 'Load item details automatically' }).uncheck();
    await page.getByRole('button', { name: 'Import overview' }).click();
    await expect(page).toHaveURL(/\/media\/addon-lists\/\d+\?import=1$/);
    const listId = Number(page.url().match(/addon-lists\/(\d+)/)?.[1]);
    expect(listId).toBeGreaterThan(0);
    await expect.poll(() => sql(`SELECT "State" FROM "AddonMediaListImportJob" WHERE "AddonMediaListId"=${listId} ORDER BY "Generation" DESC LIMIT 1;`), { timeout: 60000 }).toBe('3');
    await hydrated(page, `/media/addon-lists/${listId}`);
    const option = page.locator('form[data-option-key="processed"]');
    await option.getByRole('checkbox').check();
    const saved = page.waitForResponse(response => response.url().includes('/api/addon-lists/options/change') && response.request().method() === 'POST');
    await option.getByRole('button', { name: 'Save Changes', exact: true }).click();
    expect((await saved).ok()).toBe(true);
    await hydrated(page, `/media/addon-lists/${listId}`);
    await page.getByRole('button', { name: 'Prepare publication', exact: true }).click();
    await expect.poll(() => calls().filter(call => call.command === 'paused').length, { timeout: 60000 }).toBe(1);
    const paused = calls().find(call => call.command === 'paused')!;
    expect(alive(paused.provider!)).toBe(true);
    expect(alive(paused.descendant!)).toBe(true);
    expect(Number(sql('SELECT COALESCE(SUM("ReservedBytes"),0) FROM "AddonMediaListArtifact";'))).toBeGreaterThan(0);
    const jobId = Number(sql(`SELECT "Id" FROM "AddonMediaListItemOperationJob" WHERE "AddonMediaListId"=${listId} AND "Kind"=1;`));
    const checkpoint = (): string => sql(`SELECT json_extract("Value",'$.phase') FROM "ConfigElement" WHERE "Key"='addon.preparation.worker';`);
    expect(checkpoint()).toBe('Running');
    // Node maps SIGKILL to TerminateProcess on Windows. It targets only this spawned host.
    expect(host!.kill('SIGKILL')).toBe(true);
    await expect.poll(() => host!.exitCode !== null || host!.signalCode !== null, { timeout: 10000 }).toBe(true);
    await expect.poll(() => checkpoint(), { timeout: 30000 }).toBe('Idle');
    await expect.poll(() => [workers[0], paused.provider!, paused.descendant!].every(pid => !alive(pid)), { timeout: 15000 }).toBe(true);
    expect(sql('SELECT COALESCE(SUM("ReservedBytes"),0) FROM "AddonMediaListArtifact";')).toBe('0');
    expect(sql(`SELECT "State" FROM "AddonMediaListItemOperationJob" WHERE "Id"=${jobId};`)).toBe('0');
    await start();
    // No manual retry or second Prepare click: recovered work resumes through its durable queue.
    await expect.poll(() => sql(`SELECT "State" FROM "AddonMediaListItemOperationJob" WHERE "Id"=${jobId};`), { timeout: 90000 }).toBe('2');
    expect(calls().filter(call => call.command === 'prepare')).toHaveLength(2);
    expect(sql(`SELECT COUNT(*) FROM "AddonMediaListItemOperationJob" WHERE "AddonMediaListId"=${listId} AND "Kind"=1;`)).toBe('1');
    expect(sql('SELECT COUNT(*) FROM "AddonMediaListArtifact" WHERE "State"=2 AND "DurationMilliseconds"=60000;')).toBe('1');
    expect(sql('SELECT COALESCE(SUM("ReservedBytes"),0) FROM "AddonMediaListArtifact";')).toBe('0');
    await hydrated(page, `/media/addon-lists/${listId}`);
    await page.getByRole('button', { name: 'Publish list', exact: true }).click();
    await expect.poll(() => sql(`SELECT COUNT(*) FROM "AddonMediaListRevision" WHERE "AddonMediaListId"=${listId} AND "Revision"=1 AND "CompletedAt" IS NOT NULL;`), { timeout: 90000 }).toBe('1');
    expect(sql('SELECT COUNT(*) FROM "AddonMediaListRevisionArtifact";')).toBe('1');
    expect(calls().filter(call => call.command === 'prepare')).toHaveLength(2);
    expect(calls().some(call => call.command === 'play')).toBe(false);
  } finally {
    await context.close();
    if (host && host.exitCode === null && host.signalCode === null) host.kill('SIGKILL');
    await expect.poll(() => !host || host.exitCode !== null || host.signalCode !== null, { timeout: 10000 }).toBe(true);
    await expect.poll(() => workers.every(pid => !alive(pid)), { timeout: 30000 }).toBe(true);
  }
});
