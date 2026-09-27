import { expect, test as base } from '@playwright/test';
import { spawn } from 'node:child_process';
import { closeSync, mkdirSync, mkdtempSync, openSync } from 'node:fs';
import { createServer } from 'node:net';
import { dirname, join } from 'node:path';

type IsolatedWebHost = { baseURL: string; databasePath: string };

// Destructive database faults must never affect the shared suite's workers.
export const test = base.extend<{ isolatedWebHost: IsolatedWebHost }>({
  isolatedWebHost: async ({ request }, use, info) => {
    const server = process.env.E2E_SERVER_BIN;
    const site = process.env.E2E_SITE_ROOT;
    const wrapperDb = process.env.E2E_DB_PATH;
    if (!server || !site || !wrapperDb) {
      throw new Error('Isolated host requires playwright-e2e.sh host exports');
    }
    const root = mkdtempSync(join(dirname(wrapperDb), 'storage-failure-'));
    const databasePath = join(root, 'isolated.db');
    for (const directory of ['state', 'data', 'tmp']) mkdirSync(join(root, directory));
    const listener = createServer();
    await new Promise<void>((resolve, reject) => {
      listener.once('error', reject);
      listener.listen(0, '127.0.0.1', resolve);
    });
    const address = listener.address();
    if (!address || typeof address === 'string') throw new Error('No loopback port');
    await new Promise<void>((resolve, reject) => listener.close(error => error ? reject(error) : resolve()));
    const baseURL = `http://127.0.0.1:${address.port}`;
    const log = openSync(info.outputPath('isolated-settings-host.log'), 'a');
    const child = spawn(server, [], {
      cwd: process.cwd(), windowsHide: true,
      env: { ...process.env,
        DATABASE_URL: `sqlite:${databasePath}`, ErsatzRS_DATABASE__URL: `sqlite:${databasePath}`,
        ERSATZRS_DATABASE__URL: `sqlite:${databasePath}`,
        ERSATZRS_APP_STATE_DIR: join(root, 'state'), ERSATZRS_APP_DATA_DIR: join(root, 'data'),
        TMPDIR: join(root, 'tmp'), LEPTOS_SITE_ROOT: site, LEPTOS_OUTPUT_NAME: 'ersatzrs-web',
        LEPTOS_SITE_ADDR: `127.0.0.1:${address.port}` },
      stdio: ['ignore', log, log],
    });
    closeSync(log);
    let spawnError: Error | undefined;
    child.on('error', error => { spawnError = error; });
    const exited = new Promise<void>(resolve => child.once('close', () => resolve()));
    try {
      await expect.poll(async () => {
        if (spawnError) throw spawnError;
        if (child.exitCode !== null || child.signalCode !== null) {
          throw new Error('Isolated host exited; inspect isolated-settings-host.log');
        }
        try { return (await request.get(`${baseURL}/healthz`, { timeout: 1000 })).ok(); }
        catch { return false; }
      }, { timeout: 15000 }).toBe(true);
      await use({ baseURL, databasePath });
    } finally {
      if (child.exitCode === null && child.signalCode === null) child.kill('SIGTERM');
      await exited;
    }
  },
});
