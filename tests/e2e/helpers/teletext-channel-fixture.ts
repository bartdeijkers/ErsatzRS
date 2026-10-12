import { createHash, randomUUID } from 'node:crypto';
import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { DatabaseSync } from 'node:sqlite';

const scriptBody = 'printf invoked >> "$(dirname "$0")/unexpected-invocation"\nexit 91\n';

// Only the browser runner's disposable database is supported. This package
// exercises configuration without executing or publishing authored pages.
export class TeletextChannelFixture {
  readonly database: DatabaseSync;
  readonly root = mkdtempSync(join(tmpdir(), 'ersatzrs-e2e-teletext-'));
  readonly addonId = `org.example.teletext.${randomUUID().replace(/-/g, '')}`;
  readonly channels: readonly [number, number];
  readonly originalUi: Map<string, string | undefined>;
  readonly packageHash: string;

  constructor(databasePath: string) {
    this.database = new DatabaseSync(databasePath);
    this.database.exec('PRAGMA busy_timeout=10000; PRAGMA foreign_keys=ON;');
    this.originalUi = new Map(['pages.language', 'pages.is_dark_mode']
      .map(key => [key, this.setting(key)]));
    const base = 600_000 + Number.parseInt(randomUUID().slice(0, 5), 16);
    this.channels = [base, base + 1];
    const profile = this.database.prepare('SELECT "Id" FROM "FFmpegProfile" ORDER BY "Id" LIMIT 1')
      .get() as { Id: number } | undefined;
    if (!profile) throw new Error('Disposable E2E database lacks an FFmpeg profile.');
    for (const [index, id] of this.channels.entries()) {
      this.database.prepare(`INSERT INTO "Channel" (
        "Id","UniqueId","Number","SortNumber","Name","Group","FFmpegProfileId",
        "StreamingMode","StreamSelectorMode","PlayoutSource","PlayoutMode","TranscodeMode",
        "IdleBehavior","MusicVideoCreditsMode","SongVideoMode","SubtitleMode","IsEnabled","ShowInEpg"
      ) VALUES (?, ?, ?, ?, ?, 'E2E teletext', ?, 5, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1)`)
        .run(id, randomUUID(), String(id), id, `Synthetic teletext ${index + 1}`, profile.Id);
    }
    const script = Buffer.from(scriptBody);
    writeFileSync(join(this.root, 'fixture.sh'), script);
    const length = (value: number) => {
      const bytes = Buffer.alloc(8);
      bytes.writeBigUInt64LE(BigInt(value));
      return bytes;
    };
    const path = Buffer.from('fixture.sh');
    this.packageHash = createHash('sha256').update('ersatzrs-addon-content-v1\0')
      .update(length(path.length)).update(path).update(length(script.length))
      .update(script).update(Buffer.from([0])).digest('hex');
    const manifest = {
      manifest_version: 6, id: this.addonId, version: '1.0.0', host_version: '*',
      name: { 'en-US': 'Synthetic teletext themes' },
      summary: { 'en-US': 'Configuration-only synthetic fixture' },
      license: 'MIT', source_url: 'https://example.test/teletext',
      capabilities: [{ id: 'addon.check.v1' }, { id: 'teletext.pages.v2' }],
      entrypoints: { any: { path: 'fixture.sh', kind: 'posix_shell' } },
      dependencies: [], panels: [],
      permissions: { network_hosts: [], media_read: false, managed_ffmpeg: false, external_commands: [], tool_update: [] },
      settings: [
        { key: 'THEME', label: { 'en-US': 'Theme' }, kind: 'enum', required: true, default: 'day', options: ['day', 'night'] },
        { key: 'LABEL', label: { 'en-US': 'Custom label' }, kind: 'string', required: false, default: 'Synthetic', options: [] },
      ],
    };
    this.database.prepare(`INSERT INTO "AddonInstallation" (
      "AddonId","Version","Enabled","PackageSha256","ContentSha256","ManifestJson",
      "InstallPath","InstalledAt","UpdatedAt","StateRevision","TrustTier"
    ) VALUES (?, '1.0.0', 1, ?, ?, ?, ?, '2026-10-12T00:00:00Z',
      '2026-10-12T00:00:00Z', 'synthetic-config-revision', 'local')`)
      .run(this.addonId, 'a'.repeat(64), this.packageHash, JSON.stringify(manifest), this.root);
  }

  setting(key: string): string | undefined {
    return (this.database.prepare('SELECT "Value" FROM "ConfigElement" WHERE "Key"=?')
      .get(key) as { Value: string } | undefined)?.Value;
  }

  profiles(): { Id: number; Name: string; Revision: number; IsDeleted: number }[] {
    return this.database.prepare('SELECT "Id","Name","Revision","IsDeleted" FROM "TeletextProfile" WHERE "AddonId"=? ORDER BY "Id"')
      .all(this.addonId) as ReturnType<TeletextChannelFixture['profiles']>;
  }

  binding(channel: number): { ProfileId: number; Enabled: number; Revision: number } | undefined {
    return this.database.prepare('SELECT "ProfileId","Enabled","Revision" FROM "ChannelTeletextBinding" WHERE "ChannelId"=? AND "AddonId"=?')
      .get(channel, this.addonId) as ReturnType<TeletextChannelFixture['binding']>;
  }

  theme(profile: number): string | undefined {
    return (this.database.prepare('SELECT "Value" FROM "TeletextProfileSetting" WHERE "ProfileId"=? AND "Key"=\'THEME\'')
      .get(profile) as { Value: string } | undefined)?.Value;
  }

  safetySnapshot(): unknown {
    // Include existing histories/pointers instead of inventing publication
    // receipts. A fresh runner proves configuration does not create them.
    const tables = ['TeletextPublicationRequest', 'TeletextPublicationRequestCurrent',
      'TeletextAcceptedGeneration', 'TeletextAcceptedGenerationCurrent'];
    return {
      publication: tables.map(table => this.database.prepare(`SELECT * FROM "${table}" ORDER BY 1`).all()),
      channels: this.database.prepare('SELECT * FROM "Channel" WHERE "Id" IN (?,?) ORDER BY "Id"').all(...this.channels),
      playouts: this.database.prepare('SELECT * FROM "Playout" WHERE "ChannelId" IN (?,?) ORDER BY "Id"').all(...this.channels),
      package: this.database.prepare('SELECT * FROM "AddonInstallation" WHERE "AddonId"=?').get(this.addonId),
    };
  }

  unchangedPackage(): boolean {
    return !existsSync(join(this.root, 'unexpected-invocation'))
      && readFileSync(join(this.root, 'fixture.sh'), 'utf8') === scriptBody;
  }

  close(): void {
    this.database.prepare('DELETE FROM "ChannelTeletextBinding" WHERE "AddonId"=?').run(this.addonId);
    this.database.prepare('DELETE FROM "TeletextProfile" WHERE "AddonId"=?').run(this.addonId);
    this.database.prepare('DELETE FROM "AddonInstallation" WHERE "AddonId"=?').run(this.addonId);
    this.database.prepare('DELETE FROM "Channel" WHERE "Id" IN (?,?)').run(...this.channels);
    for (const [key, value] of this.originalUi) {
      if (value === undefined) this.database.prepare('DELETE FROM "ConfigElement" WHERE "Key"=?').run(key);
      else this.database.prepare('REPLACE INTO "ConfigElement" ("Key","Value") VALUES (?,?)').run(key, value);
    }
    this.database.close();
    rmSync(this.root, { recursive: true, force: true });
  }
}
