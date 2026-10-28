import { expect, test as base } from '@playwright/test';
import { spawn, type ChildProcess } from 'node:child_process';
import { closeSync, mkdirSync, mkdtempSync, openSync, writeFileSync } from 'node:fs';
import { createServer } from 'node:net';
import { dirname, join } from 'node:path';

type HostIdentity = { pid: number; generation: number };
type HostStop = {
  pid: number | undefined;
  generation: number;
  wasRunning: boolean;
  termRequested: boolean;
  forced: boolean;
  closed: boolean;
  exitCode: number | null;
  signalCode: NodeJS.Signals | null;
};
type HostRestart = { before: HostIdentity; after: HostIdentity; stop: HostStop };
type IsolatedWebHost = {
  baseURL: string;
  listenAddress: string;
  databasePath: string;
  processIdentity(): HostIdentity;
  restart(): Promise<HostRestart>;
};
type OwnedHost = {
  child: ChildProcess;
  generation: number;
  closed: boolean;
  exited: Promise<void>;
  spawnError?: Error;
  stopping?: Promise<HostStop>;
};

async function waitClosed(host: OwnedHost, milliseconds: number): Promise<boolean> {
  if (host.closed) return true;
  let timer: ReturnType<typeof setTimeout> | undefined;
  try {
    return await Promise.race([
      host.exited.then(() => true),
      new Promise<false>(resolve => { timer = setTimeout(() => resolve(false), milliseconds); }),
    ]);
  } finally { clearTimeout(timer); }
}

const running = (host: OwnedHost): boolean =>
  host.child.exitCode === null && host.child.signalCode === null && !host.closed;

async function stopOwned(host: OwnedHost, history: HostStop[], evidence: string): Promise<HostStop> {
  // A stopped or failed owner is never signalled again, including during final
  // teardown. Only this ChildProcess handle can be targeted; no shared PID input.
  if (host.stopping) return host.stopping;
  host.stopping = (async () => {
    const record: HostStop = { pid: host.child.pid, generation: host.generation,
      wasRunning: running(host), termRequested: false, forced: false, closed: false,
      exitCode: null, signalCode: null };
    if (record.wasRunning) record.termRequested = host.child.kill('SIGTERM');
    // SIGTERM invokes the native graceful path on Unix. Node's Windows signal
    // mapping does not establish graceful shutdown; record requested signals only.
    if (!await waitClosed(host, 30_000)) {
      if (running(host)) { record.forced = true; host.child.kill('SIGKILL'); }
      record.closed = await waitClosed(host, 10_000);
    } else { record.closed = true; }
    record.exitCode = host.child.exitCode;
    record.signalCode = host.child.signalCode;
    history.push(record);
    writeFileSync(evidence, JSON.stringify(history, null, 2));
    if (!record.closed) throw new Error('Owned isolated host termination was not confirmed; restart was refused.');
    // Parent close is not independent proof that every runtime child has exited.
    return record;
  })();
  return host.stopping;
}

// Destructive database faults and restart trials must never affect the shared
// suite's host/workers. All persistent paths survive owned process replacement.
export const test = base.extend<{ isolatedWebHost: IsolatedWebHost }>({
  isolatedWebHost: [async ({ request }, use, info) => {
    const server = process.env.E2E_SERVER_BIN;
    const site = process.env.E2E_SITE_ROOT;
    const wrapperDb = process.env.E2E_DB_PATH;
    if (!server || !site || !wrapperDb) {
      throw new Error('Isolated host requires playwright-e2e.sh host exports');
    }
    // Manual receiver trials reuse an explicitly configured LAN forwarding
    // port. Ordinary suites retain loopback/ephemeral isolation. Fail closed
    // before filesystem/process creation if the dedicated opt-in is invalid.
    const externalPort = process.env.ERSATZRS_TELETEXT_EXTERNAL_CLIENT_PORT;
    if (externalPort !== undefined && (!process.env.ERSATZRS_TELETEXT_EXTERNAL_CLIENT_DIR
      || !/^[0-9]{1,5}$/.test(externalPort) || Number(externalPort) < 1 || Number(externalPort) > 65535)) {
      throw new Error('External-client port requires the owned client directory and an integer from 1 through 65535.');
    }
    const bindHost = externalPort === undefined ? '127.0.0.1' : '0.0.0.0';
    const bindPort = externalPort === undefined ? 0 : Number(externalPort);
    const root = mkdtempSync(join(dirname(wrapperDb), 'storage-failure-'));
    const databasePath = join(root, 'isolated.db');
    for (const directory of ['state', 'data', 'tmp']) mkdirSync(join(root, directory));
    const listener = createServer();
    await new Promise<void>((resolve, reject) => {
      listener.once('error', reject);
      listener.listen(bindPort, bindHost, resolve);
    });
    const address = listener.address();
    if (!address || typeof address === 'string') throw new Error('No owned host port');
    await new Promise<void>((resolve, reject) => listener.close(error => error ? reject(error) : resolve()));
    const baseURL = `http://127.0.0.1:${address.port}`;
    // Hold this exact launch contract for initial start and every restart. The
    // wrapper copied the matching executable/site; no build/download is started.
    const environment = { ...process.env,
      DATABASE_URL: `sqlite:${databasePath}`, ErsatzRS_DATABASE__URL: `sqlite:${databasePath}`,
      ERSATZRS_DATABASE__URL: `sqlite:${databasePath}`,
      ERSATZRS_APP_STATE_DIR: join(root, 'state'), ERSATZRS_APP_DATA_DIR: join(root, 'data'),
      TMPDIR: join(root, 'tmp'), LEPTOS_SITE_ROOT: site, LEPTOS_OUTPUT_NAME: 'ersatzrs-web',
      LEPTOS_SITE_ADDR: `${bindHost}:${address.port}` };
    const stops: HostStop[] = [];
    const evidence = info.outputPath('isolated-host-stops.json');
    let host: OwnedHost | undefined;
    let generation = 0;
    let disposing = false;
    let restarting: Promise<HostRestart> | undefined;
    const start = async (): Promise<void> => {
      const log = openSync(info.outputPath('isolated-settings-host.log'), 'a');
      let child: ChildProcess;
      try { child = spawn(server, [], { cwd: process.cwd(), windowsHide: true,
        env: environment, stdio: ['ignore', log, log] }); }
      finally { closeSync(log); }
      const owner: OwnedHost = { child, generation: ++generation, closed: false,
        exited: Promise.resolve() };
      owner.exited = new Promise<void>(resolve => child.once('close', () => { owner.closed = true; resolve(); }));
      child.on('error', error => { owner.spawnError = error; });
      host = owner;
      await expect.poll(async () => {
        if (owner.spawnError) throw owner.spawnError;
        if (!running(owner)) throw new Error('Isolated host exited; inspect isolated-settings-host.log');
        try { return (await request.get(`${baseURL}/healthz`, { timeout: 1000 })).ok(); }
        catch { return false; }
      }, { timeout: 15000 }).toBe(true);
    };
    const processIdentity = (): HostIdentity => {
      if (!host || !running(host) || !host.child.pid) throw new Error('Owned isolated host is not running.');
      return { pid: host.child.pid, generation: host.generation };
    };
    const restart = (): Promise<HostRestart> => {
      if (disposing || restarting) return Promise.reject(new Error('Owned isolated host restart is unavailable.'));
      const before = processIdentity();
      const owner = host!;
      const operation = (async () => {
        const stop = await stopOwned(owner, stops, evidence);
        if (disposing) throw new Error('Owned isolated host restart was cancelled during teardown.');
        await start();
        return { before, after: processIdentity(), stop };
      })();
      restarting = operation;
      void operation.finally(() => { if (restarting === operation) restarting = undefined; }).catch(() => undefined);
      return operation;
    };
    try {
      await start();
      await use({ baseURL, listenAddress: environment.LEPTOS_SITE_ADDR, databasePath, processIdentity, restart });
    } finally {
      disposing = true;
      // Even an abandoned callback is awaited within its existing stop/start
      // bounds before cleanup. It cannot spawn a new host after final stop.
      if (restarting) await restarting.catch(() => undefined);
      if (host) await stopOwned(host, stops, evidence);
    }
  // Teardown can await 15s remaining startup plus 40s bounded owned shutdown.
  // Keep the existing startup bound; allow cleanup to finish before fixture expiry.
  }, { timeout: 60_000 }],
});
