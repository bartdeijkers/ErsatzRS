import { execFileSync } from 'node:child_process';
import { createHash, randomUUID } from 'node:crypto';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
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

function editorPages(theme: 'day' | 'night', operand = false): unknown {
  const service = themedPages(theme) as {
    pages: { address: { page: number; subcode: number }; national_option: number }[];
    magazines: { presentation: { primary: number } }[];
    page_enhancements: { address: { page: number }; local_program: { commands: unknown[] } }[];
  };
  for (const page of service.pages) page.national_option = 0;
  service.magazines[0].presentation.primary = 0;
  if (operand) {
    const enhanced = service.page_enhancements.find(page => page.address.page === 200)!;
    enhanced.local_program.commands.push(
      { kind: 'set_position', position: { row: 2, column: 8 } },
      { kind: 'at_column', column: 8, operation: { kind: 'foreground', colour: 18 } },
      { kind: 'at_column', column: 8, operation: { kind: 'g0', code: 65 } },
      { kind: 'at_column', column: 9, operation: { kind: 'g0', code: 66 } },
      { kind: 'set_position', position: { row: 4, column: 8 } },
      { kind: 'at_column', column: 8, operation: { kind: 'foreground', colour: 18 } },
      { kind: 'at_column', column: 8, operation: { kind: 'g0', code: 67 } },
    );
  }
  return {
    schema_version: 3, service,
    authoring: {
      schema_version: 1, template: { id: 'synthetic-editor', revision: 'r1' },
      theme: { id: theme, overrides: [] },
      pages: service.pages.map(page => ({
        page: page.address,
        regions: [
          { id: 'free-title', kind: 'free', area: 'main', row: 2, column: 1, width: operand && page.address.page === 200 ? 39 : 20, height: 1 },
          { id: 'custom-zone', kind: 'zone', area: 'main', row: 3, column: 1, width: 20, height: 1 },
          { id: 'guide-content', kind: 'generated', area: 'main', row: 4, column: 1, width: 20, height: 1 },
        ],
        dynamic_fields: [{ id: 'guide-title', region: 'guide-content', binding: 'guide.title' }],
      })),
    },
  };
}

function standalonePages(theme: 'day' | 'night'): unknown {
  const path = fileURLToPath(new URL('../../../crates/ersatzrs-web/tests/fixtures/teletext-level15-restart-v2.json', pathToFileURL(__filename)));
  const service = JSON.parse(readFileSync(path, 'utf8'));
  for (const page of service.pages) for (const row of page.rows) for (const run of row.runs) {
    if (run.style.foreground === 'red' && theme === 'night') run.style.foreground = 'blue';
  }
  return service;
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
  readonly root: string;
  readonly addonId = `org.example.teletext.${randomUUID().replace(/-/g, '')}`;
  readonly channels: readonly [number, number];
  readonly originalUi: Map<string, string | undefined>;
  readonly packageHash: string;
  private readonly packagePath: string;
  private readonly packageFiles = new Map<string, Buffer>();
  private readonly authoredPages: boolean;
  private readonly playback: boolean;
  private readonly playbackDurationSeconds: number;
  private scripted = false;

  constructor(databasePath: string, options: { authoredPages?: boolean; editorPages?: boolean; editorOperand?: boolean; standaloneLevel15?: boolean; playback?: { ffmpeg: string; durationSeconds?: number } } = {}) {
    this.authoredPages = options.authoredPages === true;
    if (options.editorPages && !this.authoredPages) throw new Error('Synthetic editor requires the authored producer.');
    if (options.editorOperand && !options.editorPages) throw new Error('Synthetic operand editor requires editor pages.');
    if (options.standaloneLevel15 && (!this.authoredPages || options.editorPages)) throw new Error('Standalone base fixture requires the original V2 producer.');
    this.playback = options.playback !== undefined;
    this.playbackDurationSeconds = options.playback?.durationSeconds ?? 12;
    if (this.playback && !this.authoredPages) throw new Error('Synthetic playback requires the authored producer.');
    // Retained immutable histories/files belong to the wrapper DB's temporary
    // directory; it is disposed only after the host has stopped.
    this.root = mkdtempSync(join(this.playback ? dirname(databasePath) : tmpdir(), 'ersatzrs-e2e-teletext-'));
    if (options.playback) {
      try { this.generatePlaybackSource(options.playback.ffmpeg, this.playbackDurationSeconds); }
      catch (error) { rmSync(this.root, { recursive: true, force: true }); throw error; }
    }
    this.packagePath = this.authoredPages ? join(this.root, 'package') : this.root;
    if (this.authoredPages) mkdirSync(this.packagePath);
    this.database = new DatabaseSync(databasePath);
    this.database.exec('PRAGMA busy_timeout=10000; PRAGMA foreign_keys=ON;');
    this.originalUi = new Map(['pages.language', 'pages.is_dark_mode',
      'teletext.authoring.policy', 'teletext.output.policy', 'teletext.work.max_concurrent']
      .map(key => [key, this.setting(key)]));
    const profile = this.database.prepare('SELECT "Id" FROM "FFmpegProfile" ORDER BY "Id" LIMIT 1')
      .get() as { Id: number } | undefined;
    if (!profile) throw new Error('Disposable E2E database lacks an FFmpeg profile.');
    this.database.exec('BEGIN IMMEDIATE;');
    try {
      // Immutable designs/publication histories outlive deleted fixture channels.
      // Reserve their identities as well as existing IDs/numbers and shared tables.
      const tables = ['Channel', ...(this.playback ? ['Resolution', 'FFmpegProfile',
        'MediaSource', 'PlexMediaSource', 'Library', 'PlexLibrary', 'LibraryPath', 'Playout'] : [])];
      const maxima = tables.map(table => `SELECT MAX("Id") AS id FROM "${table}"`);
      const { base } = this.database.prepare(`SELECT MAX(id) + 1 AS base FROM (
        SELECT 599999 AS id UNION ALL ${maxima.join(' UNION ALL ')}
        UNION ALL SELECT MAX(CAST("Number" AS INTEGER)) FROM "Channel"
        UNION ALL SELECT MAX(CASE "DocumentVersion"
          WHEN 1 THEN json_extract(CAST("Document" AS TEXT),'$.selection.channel_id')
          WHEN 2 THEN json_extract(CAST("Document" AS TEXT),'$.effective_selection.channel_id')
        END) FROM "TeletextDesignRevision"
        UNION ALL SELECT MAX("ChannelId") FROM "TeletextPublicationRequest"
      )`).get() as { base: number };
      this.channels = [base, base + 1];
      for (const [index, id] of this.channels.entries()) {
        this.database.prepare(`INSERT INTO "Channel" (
          "Id","UniqueId","Number","SortNumber","Name","Group","FFmpegProfileId",
          "StreamingMode","StreamSelectorMode","PlayoutSource","PlayoutMode","TranscodeMode",
          "IdleBehavior","MusicVideoCreditsMode","SongVideoMode","SubtitleMode","IsEnabled","ShowInEpg"
        ) VALUES (?, ?, ?, ?, ?, 'E2E teletext', ?, 5, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1)`)
          .run(id, randomUUID(), String(id), id, `Synthetic teletext ${index + 1}`, profile.Id);
      }
      this.database.exec('COMMIT;');
    } catch (error) {
      this.database.exec('ROLLBACK;');
      throw error;
    }
    const producerScript = options.editorPages ? authoredScript.replace('teletext.pages.v2', 'teletext.pages.v3') : authoredScript;
    this.packageFiles.set('fixture.sh', Buffer.from(this.authoredPages ? producerScript : scriptBody));
    if (this.authoredPages) {
      for (const theme of ['day', 'night'] as const) {
        const response = options.standaloneLevel15 ? standalonePages(theme) : options.editorPages ? editorPages(theme, options.editorOperand) : themedPages(theme);
        this.packageFiles.set(`response-${theme}.json`, Buffer.from(JSON.stringify(response)));
      }
    }
    for (const [name, body] of this.packageFiles) writeFileSync(join(this.packagePath, name), body, { mode: 0o644 });
    this.packageHash = contentHash(this.packageFiles);
    const manifest = {
      manifest_version: 6, id: this.addonId, version: '1.0.0', host_version: '*',
      name: { 'en-US': 'Synthetic teletext themes' },
      summary: { 'en-US': this.authoredPages ? 'Synthetic original-page producer' : 'Configuration-only synthetic fixture' },
      license: 'MIT', source_url: 'https://example.test/teletext',
      capabilities: [{ id: 'addon.check.v1' }, { id: options.editorPages ? 'teletext.pages.v3' : 'teletext.pages.v2' }],
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
    if (options.playback) this.configurePlayback(profile.Id);
  }

  private generatePlaybackSource(ffmpeg: string, durationSeconds = 12): void {
    if (!Number.isSafeInteger(durationSeconds) || durationSeconds < 1 || durationSeconds > 600) {
      throw new Error('Synthetic playback duration must be between 1 and 600 seconds.');
    }
    const version = execFileSync(ffmpeg, ['-version'], { encoding: 'utf8', timeout: 10_000, maxBuffer: 65_536 });
    if (!/^ffmpeg version (?:n)?9(?:[.\s-])/.test(version.split('\n')[0])) {
      throw new Error('Synthetic playback requires the selected FFmpeg 9 executable.');
    }
    const source = join(this.root, 'synthetic-client-source.ts');
    const bytes = execFileSync(ffmpeg, ['-hide_banner', '-loglevel', 'error', '-nostdin',
      '-f', 'lavfi', '-i', 'color=c=red:size=160x90:rate=25',
      '-f', 'lavfi', '-i', 'sine=frequency=440:sample_rate=48000', '-t', String(durationSeconds),
      '-map', '0:v:0', '-map', '1:a:0', '-c:v', 'mpeg2video', '-b:v', '1000k',
      '-c:a', 'aac', '-b:a', '128k', '-metadata:s:a:0', 'language=eng',
      '-muxrate', '4953600', '-pcr_period', '20', '-mpegts_start_pid', '256',
      '-mpegts_pmt_start_pid', '4096', '-f', 'mpegts', 'pipe:1'],
    { timeout: 30_000, maxBuffer: (durationSeconds <= 12 ? 8 : durationSeconds <= 60 ? 40 : 384) * 1024 * 1024, killSignal: 'SIGKILL' });
    if (!bytes.length || bytes.length % 188 !== 0) throw new Error('Synthetic source is not packet-aligned MPEG-TS.');
    for (let offset = 0; offset < bytes.length; offset += 188) {
      if (bytes[offset] !== 0x47 || (((bytes[offset + 1] & 0x1f) << 8) | bytes[offset + 2]) === 512) {
        throw new Error('Synthetic source has invalid transport framing or an authored-PID collision.');
      }
    }
    writeFileSync(source, bytes);
  }

  private configurePlayback(templateProfile: number): void {
    const id = this.channels[0];
    this.database.exec('BEGIN IMMEDIATE;');
    try {
      this.database.prepare('INSERT INTO "Resolution" ("Id","Name","Width","Height","IsCustom") VALUES (?,\'Synthetic teletext\',160,90,1)').run(id);
      // Preserve schema additions without changing the shared template profile.
      const template = this.database.prepare('SELECT * FROM "FFmpegProfile" WHERE "Id"=?').get(templateProfile);
      if (!template) throw new Error('Disposable E2E profile is unavailable.');
      const profile = { ...template, Id: id, Name: `Synthetic teletext ${id}`, ResolutionId: id,
        TranscodeVideo: 0, TranscodeAudio: 0, VideoFormat: 99, AudioFormat: 99,
        VideoPreset: 'veryfast', VideoProfile: null,
        VideoBitrate: 4000, VideoBufferSize: 2000, AudioBitrate: 128, AudioBufferSize: 256,
        AudioChannels: 1, AudioSampleRate: 48, HardwareAcceleration: 0, ThreadCount: 1,
        OutputDynamicRange: 0, HdrMasteringDisplay: null, HdrMaxCll: null, HdrMaxFall: null,
        NormalizeColors: 0, NormalizeFramerate: 0,
        DeinterlaceVideo: 0, NormalizeLoudnessMode: 0 };
      const keys = Object.keys(profile);
      if (keys.some(key => !/^[A-Za-z][A-Za-z0-9]*$/.test(key))) throw new Error('Unexpected E2E profile schema.');
      this.database.prepare(`INSERT INTO "FFmpegProfile" (${keys.map(key => `"${key}"`).join(',')}) VALUES (${keys.map(() => '?').join(',')})`)
        .run(...keys.map(key => profile[key as keyof typeof profile]));
      this.database.prepare('INSERT INTO "MediaSource" ("Id") VALUES (?)').run(id);
      this.database.prepare('INSERT INTO "PlexMediaSource" ("Id","ServerName","ClientIdentifier") VALUES (?,?,?)')
        .run(id, `Synthetic-${id}`, `synthetic-${id}`);
      this.database.prepare('INSERT INTO "Library" ("Id","MediaSourceId","MediaKind","Name") VALUES (?,?,1,\'Synthetic teletext\')').run(id, id);
      this.database.prepare('INSERT INTO "PlexLibrary" ("Id","ShouldSyncItems") VALUES (?,1)').run(id);
      this.database.prepare('INSERT INTO "LibraryPath" ("Id","LibraryId","Path") VALUES (?,?,?)').run(id, id, this.root);
      for (const channel of this.channels) {
        this.database.prepare('UPDATE "Channel" SET "FFmpegProfileId"=?,"StreamingMode"=1,"StreamingEngine"=0,"PreferredAudioLanguageCode"=\'eng\',"SubtitleMode"=0 WHERE "Id"=?').run(id, channel);
        const schedule = join(this.root, `synthetic-client-schedule-${channel}.json`);
        this.database.prepare('INSERT INTO "Playout" ("Id","ChannelId","ScheduleKind","ScheduleFile","Seed") VALUES (?,?,20,?,0)').run(channel, channel, schedule);
      }
      this.database.exec('COMMIT;');
    } catch (error) {
      this.database.exec('ROLLBACK;');
      throw error;
    }
    this.resetSourceStart();
  }

  // Initial test-owned media/collection setup only. The real host Reset action
  // must evaluate Lua and publish every interval/checkpoint; this helper never
  // inserts or edits either authority-bearing table.
  configureScripted(): void {
    if (!this.playback || this.scripted) throw new Error('Scripted setup requires an unused playback fixture.');
    const id = this.channels[0] + 10;
    this.database.exec('BEGIN IMMEDIATE;');
    try {
      this.database.prepare('INSERT INTO "MediaSource" ("Id") VALUES (?)').run(id);
      this.database.prepare('INSERT INTO "LocalMediaSource" ("Id") VALUES (?)').run(id);
      this.database.prepare('INSERT INTO "Library" ("Id","MediaSourceId","MediaKind","Name") VALUES (?,?,1,\'Synthetic Scripted clip\')').run(id, id);
      this.database.prepare('INSERT INTO "LocalLibrary" ("Id") VALUES (?)').run(id);
      this.database.prepare('INSERT INTO "LibraryPath" ("Id","LibraryId","Path") VALUES (?,?,?)').run(id, id, this.root);
      this.database.prepare('INSERT INTO "MediaItem" ("Id","LibraryPathId","State") VALUES (?,?,0)').run(id, id);
      this.database.prepare('INSERT INTO "Movie" ("Id") VALUES (?)').run(id);
      this.database.prepare('INSERT INTO "MovieMetadata" ("DateAdded","DateUpdated","MetadataKind","MovieId","Title") VALUES (\'2026-01-01\',\'2026-01-01\',0,?,\'Synthetic red video and tone\')').run(id);
      this.database.prepare('INSERT INTO "MediaVersion" ("Id","DateAdded","DateUpdated","Duration","Height","MovieId","Name","RFrameRate","VideoScanKind","Width") VALUES (?,\'2026-01-01\',\'2026-01-01\',\'00:00:12\',90,?,\'Synthetic\',\'25/1\',1,160)').run(id, id);
      this.database.prepare('INSERT INTO "MediaFile" ("MediaVersionId","Path","PathHash") VALUES (?,?,?)').run(id, join(this.root, 'synthetic-client-source.ts'), `synthetic-${id}`);
      this.database.prepare('INSERT INTO "MediaStream" ("Index","Codec","MediaStreamKind","Channels","Default","Forced","MediaVersionId","PixelFormat") VALUES (0,\'mpeg2video\',1,0,1,0,?,\'yuv420p\')').run(id);
      this.database.prepare('INSERT INTO "MediaStream" ("Index","Codec","MediaStreamKind","Language","Channels","Default","Forced","MediaVersionId") VALUES (1,\'aac\',2,\'eng\',1,1,0,?)').run(id);
      this.database.prepare('INSERT INTO "Collection" ("Id","Name","UseCustomPlaybackOrder") VALUES (?,?,0)').run(id, `Synthetic Scripted ${this.addonId}`);
      this.database.prepare('INSERT INTO "CollectionItem" ("CollectionId","MediaItemId") VALUES (?,?)').run(id, id);
      for (const channel of this.channels) {
        const path = join(this.root, `synthetic-controls-${channel}.lua`);
        writeFileSync(path, 'error("Synthetic controls are not configured")');
        this.database.prepare('UPDATE "Playout" SET "ScheduleKind"=4,"ScheduleFile"=? WHERE "Id"=? AND "ChannelId"=?').run(path, channel, channel);
      }
      this.database.exec('COMMIT;');
      this.scripted = true;
    } catch (error) { this.database.exec('ROLLBACK;'); throw error; }
  }

  // Undefined emits no teletext commands: a fresh control-free Lua build.
  writeScriptedControls(channel: number, profile: number | null | undefined, count: number, leading = 0): void {
    if (!this.scripted || !this.channels.includes(channel) || !Number.isInteger(count) || count < 1
      || !Number.isInteger(leading) || leading < 0 || count + leading > 80) {
      throw new Error('Scripted controls must stay within the owned bounded fixture.');
    }
    if (profile !== null && profile !== undefined && !this.profiles().some(row => row.Id === profile && !row.IsDeleted)) {
      throw new Error('Scripted controls require an existing fixture profile.');
    }
    const controls = profile === undefined ? '' : profile === null
      ? `assert(teletext_enabled('${this.addonId}',false).ok)`
      : `assert(teletext_profile('${this.addonId}',${profile}).ok)\nassert(teletext_enabled('${this.addonId}',true).ok)`;
    writeFileSync(join(this.root, `synthetic-controls-${channel}.lua`),
      `add_collection('clip','Synthetic Scripted ${this.addonId}')\n${leading ? `add_count('clip',${leading})\n` : ''}${controls}\nadd_count('clip',${count})\n`);
  }

  scriptedIntervals(channel: number): { Start: string; Finish: string; State: number; ProfileId: number | null }[] {
    if (!this.scripted || !this.channels.includes(channel)) throw new Error('Scripted interval lookup requires an owned channel.');
    return this.database.prepare(`SELECT "Start","Finish","State",
      json_extract("CandidateSnapshot",'$.profile') AS "ProfileId"
      FROM "PlayoutTeletextInterval" WHERE "PlayoutId"=? AND "AddonId"=? ORDER BY "Start"`)
      .all(channel, this.addonId) as ReturnType<TeletextChannelFixture['scriptedIntervals']>;
  }

  resetSourceStart(): void {
    if (!this.playback) throw new Error('Source reset requires the explicit playback fixture.');
    for (const channel of this.channels) {
      writeFileSync(join(this.root, `synthetic-client-schedule-${channel}.json`), JSON.stringify({
        startTime: new Date().toISOString(), programs: [{ title: 'Synthetic local clip',
          serverKey: `Synthetic-${this.channels[0]}`, duration: Math.max(60_000, this.playbackDurationSeconds * 1000),
          file: join(this.root, 'synthetic-client-source.ts'), ratingKey: '1' }],
      }));
    }
  }

  setOutputPolicy(raw?: string | null): void {
    if (!this.playback) throw new Error('Output policy requires the explicit playback fixture.');
    if (raw === null) {
      this.database.prepare('DELETE FROM "ConfigElement" WHERE "Key"=?').run('teletext.output.policy');
      return;
    }
    const path = fileURLToPath(new URL('../../../crates/ersatzrs-web/tests/fixtures/teletext-output-policy-v1.json', pathToFileURL(__filename)));
    this.database.prepare('REPLACE INTO "ConfigElement" ("Key","Value") VALUES (?,?)')
      .run('teletext.output.policy', raw ?? readFileSync(path, 'utf8'));
    this.database.prepare('REPLACE INTO "ConfigElement" ("Key","Value") VALUES (?,?)')
      .run('teletext.work.max_concurrent', '2');
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
    // The stored declared role selects its exact private codec association;
    // never treat an effective original as a base selection via COALESCE.
    const owner = `CASE r."DocumentVersion"
      WHEN 1 THEN json_extract(CAST(r."Document" AS TEXT),'$.selection.channel_id')
      WHEN 2 THEN json_extract(CAST(r."Document" AS TEXT),'$.effective_selection.channel_id')
      END`;
    return this.database.prepare(`SELECT h."Id",h."Name",h."Revision",h."IsDeleted",r."Digest",
      ${owner} AS "ChannelId"
      FROM "TeletextDesign" h JOIN "TeletextDesignRevision" r
      ON r."DesignId"=h."Id" AND r."Revision"=h."Revision"
      WHERE ${owner} IN (?,?) ORDER BY h."Id"`)
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
    const retained = this.scripted || this.playback && (this.database.prepare(
      'SELECT COUNT(*) AS count FROM "TeletextPublicationRequest" WHERE "ChannelId" IN (?,?)')
      .get(...this.channels) as { count: number }).count > 0;
    if (retained) {
      this.database.prepare('UPDATE "Channel" SET "IsEnabled"=0,"ShowInEpg"=0 WHERE "Id" IN (?,?)').run(...this.channels);
      this.database.prepare('UPDATE "ChannelTeletextBinding" SET "Enabled"=0 WHERE "AddonId"=?').run(this.addonId);
      this.database.prepare('UPDATE "AddonInstallation" SET "Enabled"=0 WHERE "AddonId"=?').run(this.addonId);
      // Keep immutable teletext histories/files, but retire only our runtime
      // playouts and their cascading live intervals/checkpoints. Disabling an
      // add-on alone leaves legitimate global Health failures for later specs.
      this.database.prepare('DELETE FROM "Playout" WHERE "ChannelId" IN (?,?)').run(...this.channels);
      this.restoreSettings();
      this.database.close();
      // The wrapper owns later removal of this directory after host shutdown.
      // HTTP body cancellation alone does not certify worker/process cleanup.
      return;
    }
    this.database.prepare('DELETE FROM "ChannelTeletextBinding" WHERE "AddonId"=?').run(this.addonId);
    this.database.prepare('DELETE FROM "TeletextProfile" WHERE "AddonId"=?').run(this.addonId);
    this.database.prepare('DELETE FROM "AddonInstallation" WHERE "AddonId"=?').run(this.addonId);
    this.database.prepare('DELETE FROM "Channel" WHERE "Id" IN (?,?)').run(...this.channels);
    if (this.playback) {
      const id = this.channels[0];
      this.database.prepare('DELETE FROM "FFmpegProfile" WHERE "Id"=?').run(id);
      this.database.prepare('DELETE FROM "Resolution" WHERE "Id"=?').run(id);
      this.database.prepare('DELETE FROM "MediaSource" WHERE "Id"=?').run(id);
    }
    this.restoreSettings();
    this.database.close();
    rmSync(this.root, { recursive: true, force: true });
  }

  private restoreSettings(): void {
    for (const [key, value] of this.originalUi) {
      if (value === undefined) this.database.prepare('DELETE FROM "ConfigElement" WHERE "Key"=?').run(key);
      else this.database.prepare('REPLACE INTO "ConfigElement" ("Key","Value") VALUES (?,?)').run(key, value);
    }
  }
}
