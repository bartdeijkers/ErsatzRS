// Explicit native Windows acceptance; packaged launcher identity is a separate gate.
import { expect, test, type Page } from '@playwright/test';
import { spawn, execFileSync, type ChildProcess } from 'child_process';
import { createHash } from 'crypto';
import { closeSync, existsSync, mkdirSync, mkdtempSync, openSync, readFileSync, readdirSync, realpathSync, writeFileSync } from 'fs';
import { createServer, type ServerResponse } from 'http';
import { dirname, join, relative, isAbsolute } from 'path';
import { tmpdir } from 'os';
import { addonId, sourceUrl, hydrated, installPreparedFixture, preparedSqlite } from './addon-prepared-fixture';

const quote = (value: string) => `'${value.replace(/'/g, "''")}'`;
const digest = (path: string) => createHash('sha256').update(readFileSync(path)).digest('hex');
const alive = (pid: number) => { try { process.kill(pid, 0); return true; } catch { return false; } };
function firstFile(directory: string): string {
  for (const entry of readdirSync(directory, { withFileTypes: true })) {
    if (entry.isFile()) return join(directory, entry.name);
    if (entry.isDirectory()) { const file = firstFile(join(directory, entry.name)); if (file) return file; }
  }
  return '';
}

async function baseline(page: Page, sql: (query: string) => string): Promise<number> {
  await page.request.post('/api/settings/ui/save', { form: { is_dark_mode: 'true', language: 'en-us' }, maxRedirects: 0 });
  await hydrated(page, '/media/addon-lists/add');
  await page.getByLabel('Add-on list URL').fill(sourceUrl);
  await page.getByLabel('Add-on', { exact: true }).selectOption(addonId);
  await page.getByLabel('Delivery', { exact: true }).selectOption('download_ahead');
  await page.getByLabel('Collection Type', { exact: true }).selectOption('remote_streams');
  await page.getByLabel('Storage folder').fill('publication-restart');
  await page.getByRole('combobox', { name: 'Automatic Refresh' }).selectOption('never');
  await page.getByRole('checkbox', { name: 'Create playlist in original order' }).check();
  await page.getByRole('checkbox', { name: 'Load item details automatically' }).uncheck();
  await page.getByRole('button', { name: 'Import overview' }).click();
  await expect(page).toHaveURL(/\/media\/addon-lists\/\d+\?import=1$/);
  const id = Number(page.url().match(/addon-lists\/(\d+)/)?.[1]);
  expect(id).toBeGreaterThan(0);
  await expect.poll(() => sql(`SELECT "State" FROM "AddonMediaListImportJob" WHERE "AddonMediaListId"=${id} ORDER BY "Generation" DESC LIMIT 1;`), { timeout: 60000 }).toBe('3');
  await hydrated(page, `/media/addon-lists/${id}/playback`);
  await page.locator('[data-option-key="processed"] select').selectOption('true');
  await Promise.all([page.waitForEvent('framenavigated', frame => frame === page.mainFrame()),
    page.getByRole('button', { name: 'Save settings', exact: true }).click()]);
  // Finish hydrating the save response before starting the next navigation.
  await hydrated(page);
  await hydrated(page, `/media/addon-lists/${id}`);
  await expect.poll(() => sql(`SELECT COUNT(*) FROM "AddonMediaListItemOperationJob" WHERE "AddonMediaListId"=${id} AND "Kind"=1 AND "State"=2;`), { timeout: 120000 }).toBe('1');
  await expect.poll(() => sql(`SELECT "PublishedRevision"||','||("PublicationJournalJson" IS NULL) FROM "AddonMediaList" WHERE "Id"=${id};`), { timeout: 90000 }).toBe('1,1');
  return id;
}

test('packaged Windows recovers publication after scheduled scan overlap and browser disconnect', async ({ browser, request }, info) => {
  test.skip(process.env.E2E_WINDOWS_PUBLICATION_RESTART !== '1', 'Opt-in disposable Windows candidate acceptance');
  test.skip(process.platform !== 'win32' || info.project.name !== 'chromium', 'Native Windows and serial Chromium required');
  test.setTimeout(600000);
  const server = process.env.E2E_SERVER_BIN;
  const site = process.env.E2E_SITE_ROOT;
  const candidateSha = process.env.E2E_CANDIDATE_SHA;
  const ffmpeg = process.env.ErsatzRS_FFMPEG__PATH ?? process.env.ERSATZRS_FFMPEG__PATH;
  const ffprobe = process.env.ErsatzRS_FFPROBE__PATH ?? process.env.ERSATZRS_FFPROBE__PATH;
  if (!server || !site || !candidateSha?.match(/^[a-f0-9]{40}$/) || !ffmpeg || !ffprobe) {
    throw new Error('Require E2E_SERVER_BIN, E2E_SITE_ROOT, E2E_CANDIDATE_SHA and explicit FFmpeg/ffprobe paths');
  }
  for (const path of [server, site, ffmpeg, ffprobe]) expect(existsSync(path)).toBe(true);
  const root = mkdtempSync(join(tmpdir(), 'ersatzrs-publication-restart-'));
  const database = join(root, 'acceptance.db');
  const state = join(root, 'state');
  const data = join(root, 'data');
  const auxiliary = join(root, 'auxiliary');
  for (const directory of [state, data, auxiliary]) mkdirSync(directory);
  const filesystem = execFileSync('powershell.exe', ['-NoProfile', '-NonInteractive', '-Command',
    '([System.IO.DriveInfo]::new([System.IO.Path]::GetPathRoot($env:N03_ROOT))).DriveFormat'],
  { env: { ...process.env, N03_ROOT: root }, encoding: 'utf8', timeout: 10000 }).trim();
  expect(filesystem).toBe('NTFS');
  const sql = (query: string) => preparedSqlite(database, `.bail on\n${query}`);
  const timeline: { event: string; at: string }[] = [];
  const mark = (event: string) => timeline.push({ event, at: new Date().toISOString() });
  const errors: string[] = [];
  const watchErrors = (page: Page) => {
    page.on('pageerror', error => errors.push(error.name));
    page.on('console', message => { if (message.type() === 'error') errors.push('console_error'); });
  };
  let host: ChildProcess | undefined;
  let holder: ChildProcess | undefined;
  let context: Awaited<ReturnType<typeof browser.newContext>> | undefined;
  let starts = 0;
  let list = 0;
  let library = 0;
  let operation = 0;
  let job = 0;
  let generation = 0;
  const workers = new Set<number>();
  type Barrier = { released: boolean; entered: boolean; responses: ServerResponse[] };
  const scheduledGate: Barrier = { released: false, entered: false, responses: [] };
  const childGate: Barrier = { released: false, entered: false, responses: [] };
  const media = join(root, 'probe.mp4');
  execFileSync(ffmpeg, ['-hide_banner', '-loglevel', 'error', '-nostdin', '-y', '-f', 'lavfi',
    '-i', 'color=c=red:size=160x90:rate=25', '-t', '1', '-c:v', 'libx264', '-threads', '1',
    '-movflags', '+faststart', media], { timeout: 30000 });
  const bytes = readFileSync(media);
  const send = (response: ServerResponse) => {
    if (!response.destroyed) response.writeHead(200, { 'Content-Type': 'video/mp4', 'Content-Length': bytes.length }).end(bytes);
  };
  const release = (barrier: Barrier) => { barrier.released = true; for (const response of barrier.responses) send(response); barrier.responses = []; };
  const mediaServer = createServer((_incoming, response) => {
    // Identify the actual durable scanner owner, never infer a new scan from
    // request count (ffprobe may issue multiple HTTP requests).
    try {
      const kind = library ? sql(`SELECT "Kind" FROM "ManagedLibraryOperation" WHERE "LibraryId"=${library} AND "Kind" IN (1,4) AND "State"=1 ORDER BY "Id" DESC LIMIT 1;`) : '';
      const barrier = kind === '1' ? scheduledGate : kind === '4' ? childGate : undefined;
      if (barrier && !barrier.released) { barrier.entered = true; barrier.responses.push(response); }
      else send(response);
    } catch {
      errors.push('scanner_barrier_state_unavailable');
      response.writeHead(500).end();
    }
  });
  await new Promise<void>(done => mediaServer.listen(0, '127.0.0.1', done));
  const address = mediaServer.address();
  if (!address || typeof address === 'string') throw new Error('No loopback media port');
  const portServer = createServer();
  await new Promise<void>(done => portServer.listen(0, '127.0.0.1', done));
  const hostAddress = portServer.address();
  if (!hostAddress || typeof hostAddress === 'string') throw new Error('No host port');
  const base = `http://127.0.0.1:${hostAddress.port}`;
  await new Promise<void>((done, reject) => portServer.close(error => error ? reject(error) : done()));
  const start = async () => {
    const log = join(root, `host-${++starts}.log`);
    const fd = openSync(log, 'a');
    host = spawn(server, [], { cwd: dirname(server), windowsHide: true,
      env: { ...process.env, DATABASE_URL: `sqlite:${database}`, ErsatzRS_DATABASE__URL: `sqlite:${database}`,
        ERSATZRS_DATABASE__URL: `sqlite:${database}`, ERSATZRS_APP_STATE_DIR: state, ERSATZRS_APP_DATA_DIR: data,
        ErsatzRS_FFMPEG__PATH: ffmpeg, ErsatzRS_FFPROBE__PATH: ffprobe, LEPTOS_SITE_ROOT: site,
        LEPTOS_SITE_ADDR: `127.0.0.1:${hostAddress.port}`, LEPTOS_OUTPUT_NAME: 'ersatzrs-web' },
      stdio: ['ignore', fd, fd] });
    closeSync(fd);
    let spawnFailed = false;
    host.on('error', () => { spawnFailed = true; });
    await expect.poll(async () => {
      if (spawnFailed || host!.exitCode !== null || host!.signalCode !== null) throw new Error('Owned candidate startup failed');
      try { return (await request.get(`${base}/healthz`, { timeout: 1000 })).ok(); } catch { return false; }
    }, { timeout: 90000 }).toBe(true);
    expect(existsSync(database)).toBe(true);
    expect(existsSync(join(data, 'logs'))).toBe(true);
    const bootstrap = readFileSync(log, 'utf8').replace(/\\/g, '/').toLowerCase();
    expect(bootstrap.includes(database.replace(/\\/g, '/').toLowerCase()), 'Candidate bootstrap must name the disposable database').toBe(true);
    await expect.poll(() => {
      const children = execFileSync('powershell.exe', ['-NoProfile', '-NonInteractive', '-Command',
        `Get-CimInstance Win32_Process -Filter "ParentProcessId = ${host!.pid}" | Where-Object { $_.CommandLine -like '*--internal-addon-preparation-worker*' } | Select-Object -ExpandProperty ProcessId`],
      { encoding: 'utf8', timeout: 10000 }).trim();
      const pids = children.split(/\s+/).filter(Boolean).map(Number);
      for (const pid of pids) workers.add(pid);
      return pids.length;
    }, { timeout: 15000 }).toBeGreaterThan(0);
    mark(`host_started_${starts}`);
  };
  const stop = async () => {
    if (host && host.exitCode === null && host.signalCode === null) host.kill('SIGKILL');
    await expect.poll(() => !host || host.exitCode !== null || host.signalCode !== null, { timeout: 10000 }).toBe(true);
    await expect.poll(() => [...workers].every(pid => !alive(pid)), { timeout: 30000 }).toBe(true);
  };
  try {
    await start();
    installPreparedFixture(database);
    context = await browser.newContext({ baseURL: base });
    let page = await context.newPage();
    watchErrors(page);
    list = await baseline(page, sql);
    library = Number(sql(`SELECT "LibraryId" FROM "AddonMediaListLibrary" WHERE "AddonMediaListId"=${list};`));
    expect(library).toBeGreaterThan(0);
    // Setup only: ordinary finite YAML is probed, unlike generated managed
    // definitions or unknown liveness. Its auxiliary path shares the library
    // lease but is outside the publication-managed tree.
    writeFileSync(join(auxiliary, 'barrier.yml'), `url: http://127.0.0.1:${address.port}/probe.mp4\ntitle: Synthetic scan barrier\nliveness: finite\nduration: "00:00:01"\n`);
    sql(`INSERT INTO "LibraryPath" ("Path","LibraryId") VALUES (${quote(auxiliary)},${library});`);
    await hydrated(page, `/media/addon-lists/${list}`);
    sql(`UPDATE "Library" SET "LastScan"=NULL WHERE "Id"=${library};
      INSERT INTO "ConfigElement" ("Key","Value") VALUES ('scanner.library_refresh_interval','1'),
      ('maintenance.loop.library_refresh.last_run','2000-01-01 00:00:00')
      ON CONFLICT("Key") DO UPDATE SET "Value"=excluded."Value";`);
    await expect.poll(() => scheduledGate.entered, { timeout: 60000, intervals: [100] }).toBe(true);
    const scheduled = Number(sql(`SELECT "Id" FROM "ManagedLibraryOperation" WHERE "LibraryId"=${library} AND "Kind"=1 AND "State"=1 ORDER BY "Id" DESC LIMIT 1;`));
    expect(scheduled).toBeGreaterThan(0);
    mark('scheduled_scan_probe_held');
    sql(`UPDATE "AddonMediaList" SET "Name"='Recovered publication',"DraftRevision"="DraftRevision"+1 WHERE "Id"=${list};`);
    await expect.poll(() => Number(sql(`SELECT COUNT(*) FROM "AddonMediaListPublicationJob" WHERE "AddonMediaListId"=${list} AND "TargetPublishedRevision"=2;`)), { timeout: 5000, intervals: [100] }).toBe(1);
    const identity = JSON.parse(sql(`SELECT json_object('operation',"OperationId",'job',"Id",'generation',"Generation") FROM "AddonMediaListPublicationJob" WHERE "AddonMediaListId"=${list} ORDER BY "Generation" DESC LIMIT 1;`));
    operation = identity.operation; job = identity.job; generation = identity.generation;
    await expect.poll(() => Number(sql(`SELECT COUNT(*) FROM "AddonMediaListPublicationEvent" WHERE "PublicationJobId"=${job} AND "Kind"=2 AND "RelatedOperationId"=${scheduled};`)), { timeout: 5000, intervals: [100] }).toBeGreaterThan(0);
    mark('publish_waits_for_scheduled_scan');
    release(scheduledGate);
    await expect.poll(() => childGate.entered, { timeout: 15000, intervals: [100] }).toBe(true);
    expect(sql(`SELECT "State" FROM "ManagedLibraryOperation" WHERE "Id"=${scheduled};`)).toBe('3');
    const journal = JSON.parse(sql(`SELECT "PublicationJournalJson" FROM "AddonMediaList" WHERE "Id"=${list};`));
    expect(journal.operation_id).toBe(operation);
    expect(journal.generation).toBe(generation);
    const backup = journal.backup_folder as string;
    // Rust may persist a verbatim Windows path; compare existing canonical paths
    // so the same directory spelling does not look like a different volume.
    const child = relative(realpathSync.native(root), realpathSync.native(backup));
    expect(child !== '' && child !== '..' && !child.startsWith('..\\') && !child.startsWith('../') && !isAbsolute(child)).toBe(true);
    const locked = firstFile(backup);
    expect(locked).not.toBe('');
    holder = spawn('powershell.exe', ['-NoProfile', '-NonInteractive', '-Command',
      '$stream=[System.IO.File]::Open($env:N03_LOCK_PATH,[System.IO.FileMode]::Open,[System.IO.FileAccess]::Read,[System.IO.FileShare]::Read);try{[Console]::Out.WriteLine("held");[Console]::Out.Flush();[Console]::In.ReadLine()|Out-Null}finally{$stream.Dispose()}'],
    { windowsHide: true, env: { ...process.env, N03_LOCK_PATH: locked }, stdio: ['pipe', 'pipe', 'pipe'] });
    let held = false;
    let holderFailed = false;
    holder.on('error', () => { holderFailed = true; });
    holder.stdout!.on('data', chunk => { if (chunk.toString().includes('held')) held = true; });
    await expect.poll(() => {
      if (holderFailed || holder!.exitCode !== null) throw new Error('Owned NTFS fault helper failed');
      return held;
    }, { timeout: 10000, intervals: [100] }).toBe(true);
    mark('ntfs_handle_held_during_publication_scan');
    release(childGate);
    await context.close();
    mark('browser_disconnected');
    // Eight attempts use at most 136 seconds of scheduled backoff plus work
    // and polling; 240 seconds bounds this real NTFS fault. Keep the handle
    // until Failed is stable for explicit UI recovery. No journal is seeded.
    await expect.poll(() => sql(`SELECT "State" FROM "ManagedLibraryOperation" WHERE "Id"=${operation};`), { timeout: 240000, intervals: [250, 500, 1000] }).toBe('4');
    expect(Number(sql(`SELECT COUNT(*) FROM "AddonMediaListPublicationEvent" WHERE "PublicationJobId"=${job} AND "ErrorCategory"=2 AND "OsErrorCode" IN (5,32,33);`))).toBeGreaterThan(0);
    expect(sql(`SELECT "PublicationJournalJson" IS NOT NULL FROM "AddonMediaList" WHERE "Id"=${list};`)).toBe('1');
    mark('sharing_failure_retains_journal');
    await stop();
    mark('host_terminated');
    holder.stdin!.end('\n');
    await expect.poll(() => holder!.exitCode !== null, { timeout: 10000 }).toBe(true);
    await start();
    context = await browser.newContext({ baseURL: base });
    page = await context.newPage();
    watchErrors(page);
    await hydrated(page, `/media/addon-lists/${list}`);
    await expect(page.getByTestId('addon-publication-recovery')).toContainText('publication_recovery_exhausted');
    await page.screenshot({ path: info.outputPath('restart-recovery.png'), fullPage: true });
    await expect(page.locator('button[value="recover-retry"]')).toBeEnabled();
    await page.locator('button[value="recover-retry"]').click();
    await expect.poll(() => sql(`SELECT "PublishedRevision"||','||("PublicationJournalJson" IS NULL) FROM "AddonMediaList" WHERE "Id"=${list};`), { timeout: 90000 }).toBe('2,1');
    await expect.poll(() => sql(`SELECT "State" FROM "ManagedLibraryOperation" WHERE "Id"=${operation};`)).toBe('3');
    expect(sql(`SELECT COUNT(*)||','||MAX("Generation") FROM "AddonMediaListPublicationJob" WHERE "AddonMediaListId"=${list} AND "TargetPublishedRevision"=2;`)).toBe(`1,${generation}`);
    expect(sql(`SELECT COUNT(*) FROM "AddonMediaListRevision" WHERE "AddonMediaListId"=${list} AND "Revision"=2 AND "CompletedAt" IS NOT NULL;`)).toBe('1');
    expect(Number(sql(`SELECT COUNT(*) FROM "AddonMediaListPublicationEvent" WHERE "PublicationJobId"=${job} AND "Kind"=12;`))).toBeGreaterThan(0);
    expect(sql(`SELECT COUNT(*) FROM "ManagedLibraryLease" WHERE "LibraryId"=${library};`)).toBe('0');
    expect(sql(`SELECT COUNT(*) FROM "ManagedLibraryOperation" WHERE "LibraryId"=${library} AND "State" IN (0,1,2);`)).toBe('0');
    expect(sql(`SELECT COUNT(*) FROM "AddonMediaListItem" WHERE "AddonMediaListId"=${list} AND "Published"=1;`)).toBe('1');
    expect(sql(`SELECT COUNT(*) FROM "PlaylistItem" WHERE "PlaylistId"=(SELECT "PlaylistId" FROM "AddonMediaList" WHERE "Id"=${list});`)).toBe('1');
    expect(existsSync(backup)).toBe(false);
    expect(errors).toEqual([]);
    mark('one_exact_revision_recovered_without_repair');
  } finally {
    release(scheduledGate); release(childGate);
    await context?.close().catch(() => {});
    if (holder && holder.exitCode === null) holder.stdin?.end('\n');
    try { await stop(); } finally {
      if (holder && holder.exitCode === null) holder.kill('SIGKILL');
      mediaServer.closeAllConnections();
      await new Promise<void>(done => mediaServer.close(() => done()));
      const events = job ? JSON.parse(sql(`SELECT json_group_array(json_object('kind',"Kind",'errorCategory',"ErrorCategory",'osErrorCode',"OsErrorCode")) FROM "AddonMediaListPublicationEvent" WHERE "PublicationJobId"=${job};`)) : [];
      writeFileSync(info.outputPath('publication-restart-proof.json'), JSON.stringify({ candidateSha, filesystem, events,
        serverSha256: digest(server), launcherIdentity: 'Separate gate; packaged server started directly',
        timeline, starts, list, library, operation, job, generation, browserErrorNames: errors,
        retainedDisposableDirectory: root, cleanup: 'Owned processes only; disposable evidence retained' }, null, 2));
    }
  }
});
