import { createHash, randomUUID } from 'node:crypto';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { DatabaseSync } from 'node:sqlite';
import { fileURLToPath, pathToFileURL } from 'node:url';

const scriptBody = 'printf invoked >> "$(dirname "$0")/unexpected-invocation"\nexit 91\n';
// Intentional Unix-only synthetic producer, matching the native authoring
// fixture. The UUID generation is checked before using it in sed replacement.
const authoredScript = String.raw`test "$1" = pages || exit 9
test "$ERSATZRS_ADDON_CAPABILITY" = teletext.pages.v2 || exit 10
theme="$ERSATZRS_ADDON_SETTING_THEME"
case "$theme" in day|night) ;; *) exit 11;; esac
IFS= read -r input || test -n "$input" || exit 12
printf '%s' "$input" > "../authoring-input-$theme.json"
printf '%s\n' "$theme" >> ../authoring-calls
generation="${'$'}{input#*\"generation\":\"}"
generation="${'$'}{generation%%\"*}"
case "$generation" in ""|*[!0-9a-f-]*) exit 13;; esac
sed "s/native-profile-generation/$generation/" "response-$theme.json"
`;

function themedPages(theme: 'day' | 'night'): unknown {
  // Same original two-page model as ordinary_stream_flow::themed_pages: one
  // magazine palette, Local X26 foreground/G0, reciprocal original Index links.
  const rgb = theme === 'day' ? [15, 0, 0] : [0, 0, 15];
  const owner = (page: number) => ({ page, subcode: 0 });
  return {
    schema_version: 2, generation: 'native-profile-generation', language: 'nld', initial_page: 200,
    pages: [200, 201].map(page => ({
      address: owner(page), kind: 'full_page', language: 'nld', national_option: 1,
      rows: [{ row: 1, runs: [{ column: 8,
        style: { foreground: 'white', background: 'black', height: 'normal', boxed: false, flash: false, conceal: false },
        content: { kind: 'text', text: 'ZZ' },
      }] }],
      navigation: [{ role: 'index', target: { page: page === 200 ? 201 : 200, subcode: null } }],
    })),
    page_enhancements: [200, 201].map(page => ({
      address: owner(page), presentation: null, compositional_links: null,
      local_program: { commands: [
        { kind: 'set_position', position: { row: 1, column: 8 } },
        { kind: 'at_column', column: 8, operation: { kind: 'foreground', colour: 18 } },
        { kind: 'at_column', column: 8, operation: { kind: 'g0', code: 65 } },
      ] },
      resources: { objects: { public: null, global: null }, drcs: { normal: null, global: null }, defaults: [null, null, null, null] },
    })),
    magazines: [{ magazine: 2, presentation: {
      primary: 1, secondary: null,
      palette: { clut2: Array.from({ length: 8 }, () => rgb), clut3: Array.from({ length: 8 }, () => [3, 3, 3]) },
      screen_colour: 0, row_colour: 0, black_background_substitution: false,
      remapping: 'fg_clut0_bg_clut0', panels: { left: false, right: false, status: 'level25_and35', width: 0 },
    } }],
    objects: [], drcs: [], mots: [], empty_object_pages: [],
    mot_selections: [200, 201].map(page => ({ display: owner(page), mot: null })),
  };
}

function contentHash(files: Map<string, Buffer>): string {
  const length = (value: number) => {
    const bytes = Buffer.alloc(8);
    bytes.writeBigUInt64LE(BigInt(value));
    return bytes;
  };
  const digest = createHash('sha256').update('ersatzrs-addon-content-v1\0');
  for (const [name, body] of [...files].sort(([left], [right]) => left < right ? -1 : left > right ? 1 : 0)) {
    const path = Buffer.from(name);
    digest.update(length(path.length)).update(path).update(length(body.length)).update(body).update(Buffer.from([0]));
  }
  return digest.digest('hex');
}

// Only the browser runner's disposable database is supported. This package
// defaults to configuration-only behavior. The optional authored producer is
// synthetic engine evidence, never an installed public package or publication.
export class TeletextChannelFixture {
  readonly database: DatabaseSync;
  readonly root = mkdtempSync(join(tmpdir(), 'ersatzrs-e2e-teletext-'));
  readonly addonId = `org.example.teletext.${randomUUID().replace(/-/g, '')}`;
  readonly channels: readonly [number, number];
  readonly originalUi: Map<string, string | undefined>;
  readonly packageHash: string;
  private readonly packagePath: string;
  private readonly packageFiles = new Map<string, Buffer>();
  private readonly authoredPages: boolean;

  constructor(databasePath: string, options: { authoredPages?: boolean } = {}) {
    this.authoredPages = options.authoredPages === true;
    this.packagePath = this.authoredPages ? join(this.root, 'package') : this.root;
    if (this.authoredPages) mkdirSync(this.packagePath);
    this.database = new DatabaseSync(databasePath);
    this.database.exec('PRAGMA busy_timeout=10000; PRAGMA foreign_keys=ON;');
    this.originalUi = new Map(['pages.language', 'pages.is_dark_mode',
      'teletext.authoring.policy', 'teletext.output.policy', 'teletext.work.max_concurrent']
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
    this.packageFiles.set('fixture.sh', Buffer.from(this.authoredPages ? authoredScript : scriptBody));
    if (this.authoredPages) {
      for (const theme of ['day', 'night'] as const) {
        this.packageFiles.set(`response-${theme}.json`, Buffer.from(JSON.stringify(themedPages(theme))));
      }
    }
    for (const [name, body] of this.packageFiles) writeFileSync(join(this.packagePath, name), body, { mode: 0o644 });
    this.packageHash = contentHash(this.packageFiles);
    const manifest = {
      manifest_version: 6, id: this.addonId, version: '1.0.0', host_version: '*',
      name: { 'en-US': 'Synthetic teletext themes' },
      summary: { 'en-US': this.authoredPages ? 'Synthetic original-page producer' : 'Configuration-only synthetic fixture' },
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
      .run(this.addonId, 'a'.repeat(64), this.packageHash, JSON.stringify(manifest), this.packagePath);
  }

  setting(key: string): string | undefined {
    return (this.database.prepare('SELECT "Value" FROM "ConfigElement" WHERE "Key"=?')
      .get(key) as { Value: string } | undefined)?.Value;
  }

  setAuthoringPolicy(raw?: string | null): void {
    if (raw === null) {
      this.database.prepare('DELETE FROM "ConfigElement" WHERE "Key"=?').run('teletext.authoring.policy');
      return;
    }
    if (raw === undefined) {
      const fixture = fileURLToPath(new URL('../../../crates/ersatzrs-web/tests/fixtures/teletext-output-policy-v1.json', pathToFileURL(__filename)));
      const policy = JSON.parse(readFileSync(fixture, 'utf8'));
      delete policy.source;
      delete policy.recovery_timeout_ms;
      policy.operation_timeout_ms = 30_000;
      // Explicit test-only bounds, never production configuration defaults.
      policy.preview = { max_pages: 16, max_model_records: 32_768, max_pixels: 168_000, max_diagnostics: 1400, max_work: 50_000_000 };
      raw = JSON.stringify(policy);
    }
    this.database.prepare('REPLACE INTO "ConfigElement" ("Key","Value") VALUES (?,?)').run('teletext.authoring.policy', raw);
  }

  calls(): string[] {
    const path = join(this.root, this.authoredPages ? 'authoring-calls' : 'unexpected-invocation');
    if (!existsSync(path)) return [];
    const value = readFileSync(path, 'utf8');
    return this.authoredPages ? value.trim().split('\n').filter(Boolean) : value.match(/invoked/g) ?? [];
  }

  request(theme: 'day' | 'night'): Record<string, unknown> | undefined {
    const path = join(this.root, `authoring-input-${theme}.json`);
    return existsSync(path) ? JSON.parse(readFileSync(path, 'utf8')) : undefined;
  }

  designs(channel?: number): { Id: number; Name: string; Revision: number; IsDeleted: number; ChannelId: number; Digest: string }[] {
    const channels = channel === undefined ? this.channels : [channel, channel];
    if (!channels.every(value => this.channels.includes(value))) throw new Error('Design lookup must stay within the fixture channels.');
    return this.database.prepare(`SELECT h."Id",h."Name",h."Revision",h."IsDeleted",r."Digest",
      json_extract(CAST(r."Document" AS TEXT),'$.selection.channel_id') AS "ChannelId"
      FROM "TeletextDesign" h JOIN "TeletextDesignRevision" r
      ON r."DesignId"=h."Id" AND r."Revision"=h."Revision"
      WHERE json_extract(CAST(r."Document" AS TEXT),'$.selection.channel_id') IN (?,?) ORDER BY h."Id"`)
      .all(...channels) as ReturnType<TeletextChannelFixture['designs']>;
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
      // Preserve every configuration key and value, except the exact seven
      // scheduler heartbeat values written independently by maintenance.rs.
      configuration: this.database.prepare(`SELECT "Key", CASE WHEN "Key" IN (
          'maintenance.loop.playout.last_run','maintenance.loop.library_refresh.last_run',
          'maintenance.loop.provider_poll.last_run','maintenance.loop.cache_cleanup.last_run',
          'maintenance.loop.search_index.last_run','maintenance.loop.artwork_cleanup.last_run',
          'maintenance.loop.subtitle_cleanup.last_run'
        ) THEN '<background-run>' ELSE "Value" END AS "Value" FROM "ConfigElement"
        WHERE "Key" NOT IN ('pages.language','pages.is_dark_mode') AND "Key" NOT LIKE 'health.%'
        ORDER BY "Key"`).all(),
      channels: this.database.prepare('SELECT * FROM "Channel" WHERE "Id" IN (?,?) ORDER BY "Id"').all(...this.channels),
      playouts: this.database.prepare('SELECT * FROM "Playout" WHERE "ChannelId" IN (?,?) ORDER BY "Id"').all(...this.channels),
      package: this.database.prepare('SELECT * FROM "AddonInstallation" WHERE "AddonId"=?').get(this.addonId),
    };
  }

  unchangedPackage(): boolean {
    const names = readdirSync(this.packagePath).sort();
    if (names.join('\0') !== [...this.packageFiles.keys()].sort().join('\0')) return false;
    const actual = new Map<string, Buffer>();
    for (const [name, original] of this.packageFiles) {
      const path = join(this.packagePath, name);
      if (!statSync(path).isFile() || (statSync(path).mode & 0o111) !== 0) return false;
      const body = readFileSync(path);
      if (!body.equals(original)) return false;
      actual.set(name, body);
    }
    return contentHash(actual) === this.packageHash;
  }

  close(): void {
    // Immutable owned history remains until the wrapper disposes its E2E DB;
    // retain the production deletion guards and tombstone only our design heads.
    for (const design of this.designs()) {
      this.database.prepare('UPDATE "TeletextDesign" SET "IsDeleted"=1 WHERE "Id"=?').run(design.Id);
    }
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
