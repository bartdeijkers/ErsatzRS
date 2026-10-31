import { expect, test } from '@playwright/test';
import { spawn, type ChildProcess } from 'node:child_process';
import { snapshotOwnedStartup, startupOwner } from './helpers/isolated-host-startup';

async function stop(child: ChildProcess): Promise<void> {
  if (child.exitCode !== null || child.signalCode !== null) return;
  let timer: ReturnType<typeof setTimeout> | undefined;
  const closed = new Promise<void>((resolve, reject) => {
    child.once('close', () => resolve());
    timer = setTimeout(() => { child.kill('SIGKILL'); reject(new Error('Owned synthetic child did not close.')); }, 3000);
  });
  child.kill('SIGTERM');
  try { await closed; } finally { clearTimeout(timer); }
}

test('isolated startup diagnostics observe only the owned identity and redact arguments', async () => {
  test.skip(process.platform !== 'linux', 'Intentional Linux /proc observer; other platforms record unsupported.');
  const secret = 'synthetic-token-never-persist';
  const url = 'https://private.invalid/source?token=synthetic-token-never-persist';
  const child = spawn(process.execPath, ['-e', `
    const {spawn}=require('node:child_process');
    const nested=spawn(process.execPath,['-e','setInterval(()=>{},1000)',...process.argv.slice(1)],{stdio:'ignore'});
    nested.once('spawn',()=>process.send({pid:nested.pid}));
    process.on('SIGTERM',()=>{nested.once('close',()=>process.exit(0));nested.kill('SIGTERM');});
    setInterval(()=>{},1000);
  `, '--', secret, url, '-hwaccels', '-bsfs', '-formats', '-pix_fmts', '-devices', '-c:v', 'h264_cuvid', '-f', 'null'], { stdio: ['ignore', 'ignore', 'ignore', 'ipc'] });
  // Sibling activity is deliberately present in the same test process; a
  // parent-wide or broad /proc collector would incorrectly include it.
  const unrelated = spawn(process.execPath, ['-e', 'setInterval(()=>{},1000)'], { stdio: 'ignore' });
  const owner = startupOwner(child.pid);
  try {
    let timer: ReturnType<typeof setTimeout> | undefined;
    const nested = await new Promise<number>((resolve, reject) => {
      child.once('error', reject);
      child.once('message', message => { clearTimeout(timer); resolve((message as { pid: number }).pid); });
      timer = setTimeout(() => reject(new Error('Owned synthetic child did not become ready.')), 3000);
    }).finally(() => clearTimeout(timer));
    expect(owner).toBeDefined();
    const snapshot = snapshotOwnedStartup(owner);
    expect(snapshot.status).toBe('observed');
    expect(snapshot.truncated).toBe(false);
    expect(snapshot.processes.map(entry => entry.pid).sort((a, b) => a - b)).toEqual([child.pid!, nested].sort((a, b) => a - b));
    expect(snapshot.processes.some(entry => entry.pid === unrelated.pid || entry.pid === process.pid)).toBe(false);
    expect(snapshot.processes.every(entry => entry.startTick && entry.state && entry.wchan && entry.comm === 'node')).toBe(true);
    expect(snapshot.processes.every(entry => entry.probe.flags.includes('-hwaccels')
      && ['-bsfs', '-formats', '-pix_fmts', '-devices'].every(flag => entry.probe.flags.includes(flag))
      && entry.probe.codecs.includes('h264_cuvid') && entry.probe.formats.includes('null'))).toBe(true);
    const persisted = JSON.stringify(snapshot);
    for (const forbidden of [secret, url, 'private.invalid', 'setInterval', process.execPath, 'cmdline']) {
      expect(persisted).not.toContain(forbidden);
    }
    for (const invalid of [undefined, { pid: child.pid!, startTick: `${owner!.startTick}0` }, { pid: 2147483647, startTick: '1' }]) {
      const refused = snapshotOwnedStartup(invalid);
      expect(refused.status).toBe('owner-unavailable');
      expect(refused.processes).toEqual([]);
    }
  } finally {
    await Promise.all([stop(child), stop(unrelated)]);
  }
  expect(child.exitCode).toBe(0);
  expect(unrelated.signalCode).toBe('SIGTERM');
});
