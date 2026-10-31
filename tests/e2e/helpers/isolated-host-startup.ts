import { execFileSync } from 'node:child_process';
import { closeSync, openSync, readFileSync, readSync } from 'node:fs';
import { performance } from 'node:perf_hooks';

export type StartupOwner = { pid: number; startTick: string };
type ProcessState = StartupOwner & { parent: number; comm: string; state: string; wchan: string };
type Probe = { flags: string[]; codecs: string[]; formats: string[];
  preparationWorker: boolean; nextController: boolean };
export type StartupSnapshot = {
  status: 'observed' | 'unsupported' | 'owner-unavailable' | 'snapshot-unavailable';
  platform: NodeJS.Platform; owner: StartupOwner | null; truncated: boolean;
  processes: (ProcessState & { probe: Probe })[];
};

const knownCommands = new Set(['ersatzrs-web', 'ersatzrs-scanne', 'ffmpeg', 'ffprobe',
  'ersatztv-channe', 'ersatztv-channel', 'node', 'vainfo', 'nvidia-smi']);
const knownFlags = new Set(['-version', '-hwaccels', '-encoders', '-decoders', '-filters',
  '-hide_banner', '-nostdin', '-init_hw_device', '-filter_hw_device', '-hwaccel',
  '-hwaccel_output_format', '-show_streams', '-show_format', '-show_frames',
  '-bsfs', '-formats', '-pix_fmts', '-devices']);
const knownCodecs = new Set(['h264', 'hevc', 'mpeg2video', 'libx264', 'libx265', 'aac',
  'h264_nvenc', 'hevc_nvenc', 'h264_qsv', 'hevc_qsv', 'h264_vaapi', 'hevc_vaapi', 'copy',
  'h264_cuvid', 'hevc_cuvid', 'mpeg2_cuvid', 'mpeg4_cuvid', 'mjpeg_cuvid',
  'vc1_cuvid', 'vp8_cuvid', 'vp9_cuvid', 'av1_cuvid']);
const knownFormats = new Set(['null', 'lavfi', 'mpegts', 'rawvideo', 'json']);

function state(pid: number): ProcessState {
  const raw = readFileSync(`/proc/${pid}/stat`, 'utf8');
  const end = raw.lastIndexOf(')');
  const fields = raw.slice(end + 2).trim().split(/\s+/);
  if (end < 0 || !/^\d+$/.test(fields[19] ?? '') || !/^\d+$/.test(fields[1] ?? '')) {
    throw new Error('Invalid owned process identity.');
  }
  const command = raw.slice(raw.indexOf('(') + 1, end);
  const wait = readFileSync(`/proc/${pid}/wchan`, 'utf8').trim();
  return { pid, startTick: fields[19], parent: Number(fields[1]),
    comm: knownCommands.has(command) ? command : '[other]',
    state: /^[RSDZTWXIPt]$/.test(fields[0]) ? fields[0] : '?',
    wchan: /^[A-Za-z0-9_]{1,80}$/.test(wait) ? wait : '[unavailable]' };
}

export function startupOwner(pid: number | undefined): StartupOwner | undefined {
  if (process.platform !== 'linux' || !pid || !Number.isSafeInteger(pid) || pid <= 0) return undefined;
  try { const current = state(pid); return { pid, startTick: current.startTick }; }
  catch { return undefined; }
}

function sameOwner(owner: StartupOwner | undefined): boolean {
  if (!owner || !Number.isSafeInteger(owner.pid) || owner.pid <= 0 || !/^\d+$/.test(owner.startTick)) return false;
  return startupOwner(owner.pid)?.startTick === owner.startTick;
}

function probe(pid: number, comm: string): Probe {
  // Bound reads and retain only enumerated properties. Never persist argv,
  // environment, source names/URLs, hardware paths or complete filter graphs.
  const buffer = Buffer.alloc(65_536);
  const descriptor = openSync(`/proc/${pid}/cmdline`, 'r');
  let count: number;
  try { count = readSync(descriptor, buffer, 0, buffer.length, 0); }
  finally { closeSync(descriptor); }
  const args = buffer.subarray(0, count).toString('utf8').split('\0');
  const codecs: string[] = []; const formats: string[] = [];
  for (let index = 1; index + 1 < args.length; index++) {
    if (['-c:v', '-c:a', '-codec:v', '-codec:a', '-vcodec', '-acodec'].includes(args[index])
      && knownCodecs.has(args[index + 1])) codecs.push(args[index + 1]);
    if (args[index] === '-f' && knownFormats.has(args[index + 1])) formats.push(args[index + 1]);
  }
  return { flags: [...new Set(args.filter(arg => knownFlags.has(arg)))],
    codecs: [...new Set(codecs)], formats: [...new Set(formats)],
    preparationWorker: args.includes('--internal-addon-preparation-worker'),
    nextController: comm === 'ersatztv-channe' || comm === 'ersatztv-channel' };
}

export function snapshotOwnedStartup(owner: StartupOwner | undefined): StartupSnapshot {
  const result: StartupSnapshot = { status: 'owner-unavailable', platform: process.platform,
    owner: owner ?? null, truncated: false, processes: [] };
  if (process.platform !== 'linux') return { ...result, status: 'unsupported' };
  const deadline = performance.now() + 3000;
  if (!sameOwner(owner)) return result;
  try {
    const table = execFileSync('ps', ['-eo', 'pid=,ppid='], {
      encoding: 'utf8', timeout: Math.max(1, Math.ceil(deadline - performance.now())), maxBuffer: 1024 * 1024,
    }).trim().split('\n').map(line => {
      const match = line.match(/^\s*(\d+)\s+(\d+)\s*$/);
      if (!match) throw new Error('Invalid process table.');
      return { pid: Number(match[1]), parent: Number(match[2]) };
    });
    // Recheck identity before touching any descendant's /proc entry.
    if (!sameOwner(owner)) return result;
    const root = state(owner!.pid);
    if (root.startTick !== owner!.startTick) return result;
    const rootProbe = probe(root.pid, root.comm);
    if (!sameOwner(owner)) return result;
    result.processes.push({ ...root, probe: rootProbe });
    // Insertion order is breadth-first. Only a validated live parent can
    // confer ownership; an old PID table alone never authorizes a /proc read.
    const confirmed = new Map<number, StartupOwner>([[root.pid, owner!]]);
    for (const parent of confirmed.values()) {
      for (const row of table) {
        if (row.parent !== parent.pid || confirmed.has(row.pid)) continue;
        if (performance.now() >= deadline || confirmed.size >= 256) { result.truncated = true; break; }
        if (!sameOwner(owner)) return { ...result, processes: [] };
        if (!sameOwner(parent)) break;
        try {
          const current = state(row.pid);
          if (current.parent !== parent.pid) continue;
          const inspected = probe(row.pid, current.comm);
          const after = state(row.pid);
          if (!sameOwner(parent) || !sameOwner(owner)) continue;
          if (after.startTick !== current.startTick || after.parent !== parent.pid) continue;
          confirmed.set(row.pid, { pid: row.pid, startTick: current.startTick });
          result.processes.push({ ...current, probe: inspected });
        } catch { /* A short-lived owned child may disappear during observation. */ }
      }
      if (performance.now() >= deadline) { result.truncated = true; break; }
    }
    if (!sameOwner(owner)) return { ...result, processes: [] };
    return { ...result, status: 'observed' };
  } catch {
    // Error strings may contain paths or arguments. Preserve the original
    // readiness exception in the caller, with a non-sensitive diagnostic state.
    return { ...result, status: 'snapshot-unavailable', processes: [] };
  }
}
