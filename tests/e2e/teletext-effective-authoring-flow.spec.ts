import { expect, Locator, Page, test } from '@playwright/test';
import { closeSync, constants, copyFileSync, existsSync, fstatSync, lstatSync, mkdirSync, openSync, readFileSync, readSync, writeFileSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { dirname, join } from 'node:path';
import { DatabaseSync } from 'node:sqlite';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { waitForHydration } from './hydration';
import { TeletextChannelFixture } from './helpers/teletext-channel-fixture';
import { captureTeletextChannelTs } from './helpers/teletext-stream-capture';

// Actual served-host Reset -> Lua -> application publication creates every
// scoped interval. This spec never seeds interval/checkpoint/publication rows.
const databasePath = process.env.E2E_DB_PATH;
const operandEditor = process.env.ERSATZRS_TELETEXT_EFFECTIVE_EDITOR_OPERAND === '1';
const specUrl = pathToFileURL(__filename);
const cultures = [
  ['en-us', 'en_us'], ['nl', 'nl'], ['pl', 'pl'], ['es-es', 'es_es'],
  ['es-419', 'es_419'], ['fr-fr', 'fr_fr'], ['de-de', 'de_de'],
  ['it-it', 'it_it'], ['pt-br', 'pt_br'],
] as const;
function translations(module: string): (key: string) => string {
  const source = readFileSync(fileURLToPath(new URL(
    `../../crates/ersatzrs-web/src/admin/languages/${module}.rs`, specUrl,
  )), 'utf8');
  return key => {
    const found = source.match(new RegExp(String.raw`TextKey::${key}\s*=>\s*(?:\{\s*)?("(?:[^"\\]|\\.)*")`));
    if (!found) throw new Error(`Missing ${module}/${key} translation.`);
    return JSON.parse(found[1]) as string;
  };
}
async function ready(page: Page, js: boolean): Promise<void> {
  await expect(page.locator('[data-not-found]')).toHaveCount(0);
  if (js) await waitForHydration(page);
}
async function language(page: Page, culture: string, dark: boolean): Promise<void> {
  expect((await page.request.post('/admin-api/settings/ui/save', {
    form: { language: culture, is_dark_mode: String(dark) }, maxRedirects: 0,
  })).status()).toBe(303);
}
async function submit(page: Page, form: Locator, key: string, js: boolean): Promise<void> {
  const posted = page.waitForResponse(response => response.request().method() === 'POST'
    && new URL(response.url()).pathname.startsWith('/admin-api/teletext/'));
  const navigated = page.waitForEvent('framenavigated', { predicate: frame => frame === page.mainFrame() });
  await form.getByRole('button', { name: translations('en_us')(key), exact: true }).click();
  expect((await posted).status()).toBe(303);
  await navigated; await ready(page, js);
}
async function profile(page: Page, fixture: TeletextChannelFixture, theme: string, js: boolean): Promise<number> {
  const form = page.locator(`[data-addon-id="${fixture.addonId}"] [data-testid="teletext-profile-form"][data-profile-id=""]`);
  const advanced = form.locator('xpath=ancestor::details[@data-testid="teletext-source-advanced"]');
  if (await advanced.count() > 0 && await advanced.getAttribute('open') === null) await advanced.locator(':scope > summary').click();
  const details = form.locator('..');
  if (await details.getAttribute('open') === null) await details.locator(':scope > summary').click();
  await form.locator('[name="name"]').fill(`Synthetic effective ${theme}`);
  await form.locator('[data-setting-key="THEME"] select[name$=".mode"]').selectOption('literal');
  await form.locator('[data-setting-key="THEME"] [name$=".value"]').selectOption(theme);
  await submit(page, form, 'TeletextProfileCreate', js);
  const saved = fixture.profiles().find(row => row.Name === `Synthetic effective ${theme}`);
  expect(saved).toBeDefined(); return saved!.Id;
}
async function baseOff(page: Page, fixture: TeletextChannelFixture, channel: number, id: number, js: boolean): Promise<void> {
  await page.goto(`/channels/${channel}/teletext`); await ready(page, js);
  const addon = page.locator(`[data-addon-id="${fixture.addonId}"]`);
  const form = addon.getByTestId('teletext-binding-form');
  const revision = fixture.profiles().find(row => row.Id === id)!.Revision;
  await form.locator('[name="profile_choice"]').selectOption(`${id}:${revision}`);
  await submit(page, form, 'TeletextBindingSave', js);
  await submit(page, addon.getByTestId('teletext-disable-form'), 'TeletextBindingOff', js);
  expect(fixture.binding(channel)?.Enabled).toBe(0);
}
async function resetScript(page: Page, fixture: TeletextChannelFixture, channel: number, id: number | null, count: number): Promise<void> {
  fixture.writeScriptedControls(channel, id, count);
  const text = translations('en_us');
  await page.goto('/playouts'); await ready(page, true);
  const observed = fixture.database.prepare('SELECT "Name" FROM "Channel" WHERE "Id"=?').get(channel) as { Name: string } | undefined;
  expect(observed).toBeDefined();
  await page.getByPlaceholder(text('SearchForPlayouts'), { exact: true }).fill(observed!.Name);
  const row = page.getByRole('row').filter({
    has: page.locator(`a[href="/playouts/scripted/${channel}"]`),
  });
  await expect(row).toHaveCount(1);
  await row.getByRole('button', { name: text('RebuildPlayout'), exact: true }).click();
  const dialog = page.getByRole('dialog', { name: text('RebuildPlayout'), exact: true });
  await dialog.getByLabel(text('PlayoutRebuildMode'), { exact: true }).selectOption('reset');
  await dialog.getByRole('checkbox', { name: text('KeepCurrentProgramme'), exact: true }).uncheck();
  // Observe the URL emitted by the actual hydrated action. Server function
  // paths carry build hashes; the harness must not guess a registered URL.
  const posted = page.waitForResponse(response => response.request().method() === 'POST'
    && new URL(response.url()).pathname.startsWith('/admin-api/rebuild_playout'));
  await dialog.getByRole('button', { name: text('QueuePlayoutBuild'), exact: true }).click();
  const response = await posted;
  expect(response.status()).toBe(200);
  const fields = new URLSearchParams(response.request().postData() ?? '');
  expect(fields.get('id')).toBe(String(channel));
  expect(fields.get('mode')).toBe('reset');
  expect(fields.get('preserve_current')).toBe('false');
  expect(await response.json()).toEqual({ accepted: 1, coalesced: 0 });
  await expect(dialog).toBeHidden();
  await expect.poll(() => {
    const rows = fixture.scriptedIntervals(channel);
    if (rows.length !== 1 || rows[0].State !== Number(id !== null) || rows[0].ProfileId !== id) return false;
    const status = fixture.database.prepare('SELECT "Success" FROM "PlayoutBuildStatus" WHERE "PlayoutId"=?').get(channel) as { Success: number } | undefined;
    const items = fixture.database.prepare('SELECT COUNT(*) AS count FROM "PlayoutItem" WHERE "PlayoutId"=?').get(channel) as { count: number };
    return status?.Success === 1 && items.count === count;
  }, { timeout: 45_000 }).toBe(true);
}
async function prepare(page: Page, fixture: TeletextChannelFixture, theme: string, js: boolean, absent = false): Promise<string> {
  const form = page.getByTestId('teletext-effective-authoring-form');
  const publishAdvanced = page.getByTestId('teletext-publish-advanced');
  if (await publishAdvanced.getAttribute('open') === null) await publishAdvanced.locator(':scope > summary').click();
  await expect(form.locator('[name="selection_digest"]')).toHaveValue(/^[a-f0-9]{64}$/);
  await expect(form.locator('[name="scope"]')).toHaveValue('effective');
  await expect(form.getByRole('button')).toBeEnabled();
  await form.locator('[name="name"]').fill(`Synthetic scoped original ${theme}`);
  if (absent) {
    for (const field of ['entry_addon_id', 'entry_page', 'entry_subcode', 'v1_region', 'resource_transmissions']) {
      await expect(form.locator(`[name="${field}"]`)).toHaveCount(0);
    }
  } else {
    await form.locator('[name="entry_addon_id"]').selectOption(fixture.addonId);
    await form.locator('[name="entry_page"]').fill('200');
    await form.locator('[name="entry_subcode"]').fill('0000');
    await form.locator('[name="v1_region"]').selectOption('western_latin');
    await form.locator('[name="resource_transmissions"]').fill('[]');
  }
  await submit(page, form, 'TeletextValidateSaveDraft', js);
  await expect(page.getByTestId('teletext-feedback')).toHaveAttribute('data-code', 'draft');
  return page.url();
}
async function assertPreview(page: Page, theme: 'day' | 'night', address = '200'): Promise<void> {
  const model = page.getByTestId('teletext-preview-model');
  await expect(model).toHaveAttribute('data-scope', 'effective');
  await expect(model).toHaveAttribute('data-page', address);
  await expect(model).toHaveAttribute('data-subcode', '0000');
  const image = model.locator('img');
  await expect.poll(() => image.evaluate(element => (element as HTMLImageElement).complete
    && (element as HTMLImageElement).naturalWidth > 0)).toBe(true);
  const colours = await image.evaluate((element, operand) => {
    const canvas = document.createElement('canvas'); canvas.width = 480; canvas.height = 250;
    const context = canvas.getContext('2d')!; context.imageSmoothingEnabled = false;
    context.drawImage(element as HTMLImageElement, 0, 0, canvas.width, canvas.height);
    const data = context.getImageData(0, 0, canvas.width, canvas.height).data;
    let red = 0; let blue = 0;
    // The operand fixture deliberately adds blue AB to the day concept's row2.
    // The unchanged row1 remains the independent selected-theme assertion.
    const first = operand ? canvas.width * 10 * 4 : 0;
    const last = operand ? canvas.width * 20 * 4 : data.length;
    for (let index = first; index < last; index += 4) {
      if (data[index] === 255 && data[index + 1] === 0 && data[index + 2] === 0) red++;
      if (data[index] === 0 && data[index + 1] === 0 && data[index + 2] === 255) blue++;
    }
    return { red, blue };
  }, operandEditor);
  expect(colours[theme === 'day' ? 'red' : 'blue']).toBeGreaterThan(0);
  expect(colours[theme === 'day' ? 'blue' : 'red']).toBe(0);
}
function publication(fixture: TeletextChannelFixture, database = fixture.database): unknown[][] {
  return ['TeletextPublicationRequest', 'TeletextEffectivePublicationRequestCurrent',
    'TeletextAcceptedGeneration', 'TeletextEffectiveAcceptedGenerationCurrent'].map(table =>
    database.prepare(`SELECT * FROM "${table}" WHERE "ChannelId" IN (?,?) ORDER BY 1`).all(...fixture.channels));
}
function safety(fixture: TeletextChannelFixture): unknown {
  const snapshot = fixture.safetySnapshot() as { configuration: unknown; channels: unknown; playouts: unknown; package: unknown };
  return { configuration: snapshot.configuration, channels: snapshot.channels, playouts: snapshot.playouts, package: snapshot.package,
    bindings: fixture.channels.map(channel => fixture.binding(channel)),
    intervals: fixture.channels.map(channel => fixture.scriptedIntervals(channel)),
    basePointers: ['TeletextPublicationRequestCurrent', 'TeletextAcceptedGenerationCurrent'].map(table =>
      fixture.database.prepare(`SELECT * FROM "${table}" WHERE "ChannelId" IN (?,?) ORDER BY 1`).all(...fixture.channels)),
  };
}
function foreign(fixture: TeletextChannelFixture, database = fixture.database): unknown {
  return {
    bindings: database.prepare('SELECT * FROM "ChannelTeletextBinding" WHERE "ChannelId" NOT IN (?,?) ORDER BY "ChannelId","AddonId"').all(...fixture.channels),
    profiles: database.prepare('SELECT * FROM "TeletextProfile" WHERE "AddonId"!=? ORDER BY "Id"').all(fixture.addonId),
    packages: database.prepare('SELECT * FROM "AddonInstallation" WHERE "AddonId"!=? ORDER BY "AddonId"').all(fixture.addonId),
    publications: ['TeletextPublicationRequest', 'TeletextAcceptedGeneration', 'TeletextEffectivePublicationRequestCurrent', 'TeletextEffectiveAcceptedGenerationCurrent'].map(table =>
      database.prepare(`SELECT * FROM "${table}" WHERE "ChannelId" NOT IN (?,?) ORDER BY 1`).all(...fixture.channels)),
  };
}
function utcMilliseconds(value: string): number {
  const normalized = value.replace(' ', 'T');
  const time = Date.parse(/[zZ]|[+-]\d\d:\d\d$/.test(normalized) ? normalized : `${normalized}Z`);
  if (!Number.isFinite(time)) throw new Error('Published synthetic programme has an invalid UTC boundary.');
  return time;
}
async function captureOrdinary(fixture: TeletextChannelFixture, baseURL: string, channel: number,
  output: string, authored: boolean): Promise<void> {
  if (existsSync(output)) throw new Error('Effective admin capture requires a new output file.');
  // Wait for this channel's next genuine published item, independently of the
  // other channel's clock. Never rewrite source/interval/programme timestamps.
  const start = fixture.database.prepare(`SELECT "Start" FROM "PlayoutItem"
    WHERE "PlayoutId"=? AND julianday("Start")>julianday(?) ORDER BY "Start" LIMIT 1`)
    .get(channel, new Date().toISOString()) as { Start: string } | undefined;
  expect(start).toBeDefined();
  const boundary = utcMilliseconds(start!.Start);
  expect(boundary - Date.now()).toBeLessThanOrEqual(12_000);
  await expect.poll(() => Date.now() >= boundary + 25, { timeout: 13_000, intervals: [25] }).toBe(true);
  const captured = await captureTeletextChannelTs(`${baseURL}/iptv/hls-direct/${channel}.ts`, output,
    { maximumBytes: Math.floor(8 * 1024 * 1024 / 188) * 188, maximumTimeMs: 30_000 });
  expect(captured.ended).toBe('eof');
  const pids = new Map<number, number>();
  let malformed = 0;
  for (let offset = 0; offset < captured.bytes.length; offset += 188) {
    if (captured.bytes[offset] !== 0x47 || (captured.bytes[offset + 1] & 0x80) !== 0) malformed++;
    const pid = ((captured.bytes[offset + 1] & 0x1f) << 8) | captured.bytes[offset + 2];
    pids.set(pid, (pids.get(pid) ?? 0) + 1);
  }
  expect(malformed).toBe(0);
  for (const pid of [0, 256, 257]) expect(pids.get(pid) ?? 0).toBeGreaterThan(0);
  if (authored) expect(pids.get(512) ?? 0).toBeGreaterThan(0);
  else expect(pids.has(512)).toBe(false);
  execFileSync(process.env.FFMPEG_PATH!, ['-hide_banner', '-loglevel', 'error', '-nostdin',
    '-i', output, '-t', '1', '-map', '0:v:0', '-map', '0:a:0', '-f', 'null', '-'],
  { timeout: 30_000, maxBuffer: 65_536 });
  expect(fixture.calls()).toEqual(['day', 'night']);
  // Independent page decoding, TSDuck and headed client inspection use these
  // retained exact TCP captures; PID/FFmpeg checks alone do not prove rendering.
}

type EditorFile = { name: string; mimeType: string; buffer: Buffer };
type OperandEdit = {
  kind: 'local_foreground_operand'; page: 200; subcode: 0; row: 2; column: 8;
  original_command: 4; physical_word_index: number; designation: number; slot: number;
  expected_word: number; replacement_word: number;
  expected: { address: 8; mode: 0; data: 18 }; replacement: { address: 8; mode: 0; data: 4 };
  selection: 'fresh-tti-unique-qualified-row-sequence';
};

function qualifiedOperand(input: EditorFile, metadata: Record<string, any>): OperandEdit {
  expect(input.name).toBe('page-200-0000.tti');
  const association = metadata.project.pages.filter((page: { name: string }) => page.name === input.name);
  expect(association).toHaveLength(1);
  expect(association[0]).toMatchObject({ page: { page: 200, subcode: 0 },
    editor_page: 0x200, editor_ordinal: 0, tti_bytes: input.buffer.length, tti_sha256: sha256(input.buffer) });
  const records = input.buffer.toString('latin1').split(/\r?\n/);
  expect(records.filter(line => line.startsWith('PN,'))).toEqual(['PN,20000']);
  expect(records.filter(line => line.startsWith('SC,'))).toEqual(['SC,0000']);
  const packets = records.filter(line => line.startsWith('OL,26,')).map(line => {
    const payload = Buffer.from(line.slice(6), 'latin1');
    expect(payload.length).toBe(40);
    expect([...payload].every(byte => byte >= 0x40 && byte <= 0x7f)).toBe(true);
    const designation = payload[0] - 0x40; expect(designation).toBeLessThan(16);
    const words = Array.from({ length: 13 }, (_, slot) => ({ designation, slot,
      index: designation * 13 + slot,
      word: (payload[1 + slot * 3] - 0x40) | ((payload[2 + slot * 3] - 0x40) << 6)
        | ((payload[3 + slot * 3] - 0x40) << 12),
    }));
    return { designation, words };
  }).sort((left, right) => left.designation - right.designation);
  expect(packets.map(packet => packet.designation)).toEqual(packets.map((_, index) => index));
  const words = packets.flatMap(packet => packet.words);
  const literal = (address: number, mode: number, data: number) => address | (mode << 6) | (data << 11);
  const expected = [literal(41, 4, 8), literal(8, 0, 18), literal(8, 9, 65),
    literal(42, 4, 8), literal(8, 0, 18), literal(8, 9, 65), literal(9, 9, 66),
    literal(44, 4, 8), literal(8, 0, 18), literal(8, 9, 67)];
  // Fixture qualification only: no compiler command-position API is exposed to
  // this browser harness. Reject any unexpected lowering instead of guessing
  // that an authored ordinal equals a physical word or choosing the first FG.
  expect(words.filter(value => value.word !== literal(63, 31, 7)).map(value => value.word)).toEqual(expected);
  const matches = words.filter((_, index) => expected.slice(3, 7)
    .every((word, offset) => words[index + offset]?.word === word));
  expect(matches).toHaveLength(1);
  const selected = words.find(value => value.index === matches[0].index + 1)!;
  expect(selected.word).toBe(literal(8, 0, 18));
  return { kind: 'local_foreground_operand', page: 200, subcode: 0, row: 2, column: 8,
    original_command: 4, physical_word_index: selected.index, designation: selected.designation, slot: selected.slot,
    expected_word: selected.word, replacement_word: literal(8, 0, 4),
    expected: { address: 8, mode: 0, data: 18 }, replacement: { address: 8, mode: 0, data: 4 },
    selection: 'fresh-tti-unique-qualified-row-sequence' };
}

function assertOnlyOperandChanged(original: Buffer, edited: Buffer, edit: OperandEdit): void {
  const expected = original.toString('latin1').split(/\r?\n/).filter(Boolean).map(line => {
    if (!line.startsWith('OL,26,') || line.charCodeAt(6) - 0x40 !== edit.designation) return line;
    const offset = 7 + edit.slot * 3;
    const encoded = String.fromCharCode(...[0, 6, 12].map(shift => 0x40 + ((edit.replacement_word >> shift) & 0x3f)));
    return line.slice(0, offset) + encoded + line.slice(offset + 3);
  });
  // QTM canonicalizes hexadecimal case and relocates FL records. These are
  // explicit spelling/order differences, never permission to change packets.
  const canonical = (lines: string[]) => lines.map(line => /^(PS|FL),/.test(line) ? line.toLowerCase() : line).sort();
  expect(canonical(edited.toString('latin1').split(/\r?\n/).filter(Boolean))).toEqual(canonical(expected));
}
function editorFiles(archive: string): EditorFile[] {
  // Standard bounded ZIP extraction of this synthetic download. This exercises
  // browser/backend interchange, not opening or saving in QTeletextMaker.
  const script = String.raw`import base64,json,sys,zipfile
with zipfile.ZipFile(sys.argv[1]) as archive:
    entries=archive.infolist()
    assert 0<len(entries)<=64 and sum(e.file_size for e in entries)<=1048576
    assert len({e.filename for e in entries})==len(entries)
    print(json.dumps([{'name':e.filename,'base64':base64.b64encode(archive.read(e)).decode()} for e in entries]))
`;
  const raw = execFileSync('python3', ['-c', script, archive],
    { encoding: 'utf8', timeout: 10_000, maxBuffer: 2 * 1024 * 1024 });
  return (JSON.parse(raw) as { name: string; base64: string }[]).map(value => {
    expect(value.name).toMatch(/^[A-Za-z0-9_.-]+$/);
    return { name: value.name, mimeType: 'application/octet-stream', buffer: Buffer.from(value.base64, 'base64') };
  });
}
function editRow(input: EditorFile[], row: number, character: string): EditorFile[] {
  expect(character).toMatch(/^[QR]$/);
  expect(input.filter(file => file.name === 'page-200-0000.tti')).toHaveLength(1);
  return input.map(file => {
    if (file.name !== 'page-200-0000.tti') return file;
    let body = file.buffer.toString('latin1');
    const pattern = new RegExp(`^OL,${row},([^\\r\\n]*)(\\r?)(?=\\n|$)`, 'm');
    const changed = ' '.repeat(8) + character + ' '.repeat(31);
    if (pattern.test(body)) {
      body = body.replace(pattern, (_match, cells: string, ending: string) => {
        expect(cells).toBe(' '.repeat(40));
        return `OL,${row},${changed}${ending}`;
      });
    } else {
      const offset = body.indexOf('OL,'); expect(offset).toBeGreaterThan(-1);
      body = body.slice(0, offset) + `OL,${row},${changed}\n` + body.slice(offset);
    }
    return { ...file, buffer: Buffer.from(body, 'latin1') };
  });
}
function storedDocument(fixture: TeletextChannelFixture, channel: number): {
  DesignId: number; Revision: number; DocumentVersion: number; Digest: string; document: Record<string, unknown>;
} {
  const designs = fixture.designs(channel);
  expect(designs).toHaveLength(1);
  const value = fixture.database.prepare('SELECT "DesignId","Revision","DocumentVersion","Digest","Document" FROM "TeletextDesignRevision" WHERE "DesignId"=? AND "Revision"=?')
    .get(designs[0].Id, designs[0].Revision) as { DesignId: number; Revision: number; DocumentVersion: number; Digest: string; Document: Uint8Array };
  expect(value).toBeDefined();
  const { Document, ...identity } = value;
  return { ...identity, document: JSON.parse(Buffer.from(Document).toString('utf8')) };
}
function assertOperandOriginal(original: ReturnType<typeof storedDocument>, theme: 'day' | 'night'): void {
  const sources = original.document.documents as { contract: number; response: string }[];
  expect(sources).toHaveLength(1); expect(sources[0].contract).toBe(3);
  const response = JSON.parse(sources[0].response);
  expect(response.schema_version).toBe(3); expect(response.service.schema_version).toBe(2);
  expect(response.authoring.theme).toEqual({ id: theme, overrides: [] });
  const initial = [
    { kind: 'set_position', position: { row: 1, column: 8 } },
    { kind: 'at_column', column: 8, operation: { kind: 'foreground', colour: 18 } },
    { kind: 'at_column', column: 8, operation: { kind: 'g0', code: 65 } },
  ];
  expect(response.service.page_enhancements.map((page: any) => page.address)).toEqual([
    { page: 200, subcode: 0 }, { page: 201, subcode: 0 },
  ]);
  expect(response.service.page_enhancements[0].local_program.commands).toEqual([...initial,
    { kind: 'set_position', position: { row: 2, column: 8 } },
    { kind: 'at_column', column: 8, operation: { kind: 'foreground', colour: 18 } },
    { kind: 'at_column', column: 8, operation: { kind: 'g0', code: 65 } },
    { kind: 'at_column', column: 9, operation: { kind: 'g0', code: 66 } },
    { kind: 'set_position', position: { row: 4, column: 8 } },
    { kind: 'at_column', column: 8, operation: { kind: 'foreground', colour: 18 } },
    { kind: 'at_column', column: 8, operation: { kind: 'g0', code: 67 } },
  ]);
  expect(response.service.page_enhancements[1].local_program.commands).toEqual(initial);
  expect(response.authoring.pages[0].regions[0]).toEqual({ id: 'free-title', kind: 'free',
    area: 'main', row: 2, column: 1, width: 39, height: 1 });
  expect(response.authoring.pages[1].regions[0].width).toBe(20);
  expect(response.service.magazines[0].presentation.panels).toEqual({
    left: false, right: false, status: 'level25_and35', width: 0,
  });
}
async function assertOperandPreview(page: Page, theme: 'day' | 'night', edited: boolean): Promise<void> {
  await assertPreview(page, theme);
  const samples = await page.getByTestId('teletext-preview-model').locator('img').evaluate(element => {
    const canvas = document.createElement('canvas'); canvas.width = 480; canvas.height = 250;
    const context = canvas.getContext('2d')!; context.imageSmoothingEnabled = false;
    context.drawImage(element as HTMLImageElement, 0, 0, 480, 250);
    return [[2, 8], [2, 9], [4, 8]].map(([row, column]) => {
      const pixels = context.getImageData(column * 12, row * 10, 12, 10).data;
      let red = 0; let blue = 0;
      for (let offset = 0; offset < pixels.length; offset += 4) {
        if (pixels[offset] === 255 && pixels[offset + 1] === 0 && pixels[offset + 2] === 0) red++;
        if (pixels[offset] === 0 && pixels[offset + 1] === 0 && pixels[offset + 2] === 255) blue++;
      }
      return { red, blue };
    });
  });
  for (const [index, sample] of samples.entries()) {
    const colour = theme === 'night' || (edited && index < 2) ? 'blue' : 'red';
    expect(sample[colour]).toBeGreaterThan(0);
    expect(sample[colour === 'blue' ? 'red' : 'blue']).toBe(0);
  }
}
async function localizedOperand(page: Page, url: string, state: 'unrequested' | 'selected', js: boolean,
  testInfo: { outputPath(name: string): string }): Promise<void> {
  const dark = await page.locator('[data-theme]').first().getAttribute('data-theme') === 'dark';
  for (const [culture, module] of [['en-us', 'en_us'], ['nl', 'nl']]) {
    await language(page, culture, dark); await page.goto(url); await ready(page, js);
    const text = translations(module);
    await expect(page.getByRole('heading', { name: text('TeletextPreviewTitle'), exact: true })).toBeVisible();
    await expect(page.getByTestId('teletext-editor-import-form').locator('button[type="submit"]')).toHaveText(text('TeletextEditorImport'));
    await expect(page.getByTestId('teletext-publication-request-form').locator('button[type="submit"]')).toHaveText(text('TeletextPublicationRequest'));
    await expect(page.getByTestId('teletext-publication-publish-form').locator('button[type="submit"]')).toHaveText(text('TeletextPublicationPublish'));
    await expect(page.getByTestId('teletext-publication')).toHaveAttribute('data-state', state);
    await expect(page.getByTestId('teletext-publication-status')).toHaveText(text(
      state === 'selected' ? 'TeletextPublicationSelected' : 'TeletextPublicationUnrequested'));
    await assertOperandPreview(page, 'day', true);
    await page.getByTestId('teletext-preview-model').locator('img').scrollIntoViewIfNeeded();
    await page.screenshot({ path: testInfo.outputPath(`${culture}-operand-${state}.png`), fullPage: false });
  }
  await language(page, 'en-us', dark); await page.goto(url); await ready(page, js);
}
async function editPixels(page: Page, row: number): Promise<number> {
  const image = page.getByTestId('teletext-preview-model').locator('img');
  await expect.poll(() => image.evaluate(element => (element as HTMLImageElement).complete
    && (element as HTMLImageElement).naturalWidth > 0)).toBe(true);
  return image.evaluate((element, row) => {
    const canvas = document.createElement('canvas'); canvas.width = 480; canvas.height = 250;
    const context = canvas.getContext('2d')!; context.imageSmoothingEnabled = false;
    context.drawImage(element as HTMLImageElement, 0, 0, 480, 250);
    const pixels = context.getImageData(96, row * 10, 12, 10).data;
    let white = 0;
    for (let offset = 0; offset < pixels.length; offset += 4) {
      if (pixels[offset] === 255 && pixels[offset + 1] === 255 && pixels[offset + 2] === 255) white++;
    }
    return white;
  }, row);
}
function sha256(bytes: Buffer): string {
  return createHash('sha256').update(bytes).digest('hex');
}
function boundedEditorFile(path: string, maximum: number): Buffer {
  if (!lstatSync(path).isFile()) throw new Error('Editor proof requires a regular file.');
  // This opt-in shares the spec's intentional POSIX-only fixture boundary.
  const file = openSync(path, constants.O_RDONLY | constants.O_NOFOLLOW);
  try {
    const stat = fstatSync(file);
    if (!stat.isFile() || stat.size < 1 || stat.size > maximum) throw new Error('Editor proof file exceeds its bounds.');
    const bytes = Buffer.alloc(maximum + 1);
    const count = readSync(file, bytes, 0, bytes.length, 0);
    if (count < 1 || count > maximum || count !== stat.size) throw new Error('Editor proof file changed during bounded reading.');
    return bytes.subarray(0, count);
  } finally { closeSync(file); }
}
async function replayEditor(directory: string, exported: EditorFile[], metadata: Record<string, any>): Promise<EditorFile[]> {
  const binaryHash = process.env.ERSATZRS_TELETEXT_EFFECTIVE_QTM_BINARY_SHA256;
  const driverHash = process.env.ERSATZRS_TELETEXT_EFFECTIVE_QTM_DRIVER_SHA256;
  expect(binaryHash).toMatch(/^[a-f0-9]{64}$/); expect(driverHash).toMatch(/^[a-f0-9]{64}$/);
  const sourceCommit = 'cac2167bac933a6160cca45a45d5d4ee95355898';
  const input = exported.find(file => file.name === 'page-200-0000.tti')!;
  const sidecar = exported.find(file => file.name === 'project.ersatzrs.json')!;
  const operand = operandEditor ? qualifiedOperand(input, metadata) : undefined;
  expect(input.buffer.length).toBeLessThanOrEqual(65_536);
  expect(sidecar.buffer.length).toBeLessThanOrEqual(65_536);
  // New directory only; no stale completion or edited file may satisfy the gate.
  mkdirSync(directory);
  writeFileSync(join(directory, 'native-input.tti'), input.buffer, { flag: 'wx' });
  writeFileSync(join(directory, 'project.ersatzrs.json'), sidecar.buffer, { flag: 'wx' });
  const ready = Buffer.from(JSON.stringify({
    probe_version: 1, flow: 'effective-admin-native-import',
    channel_id: metadata.project.channel_id, original: metadata.project.original,
    receipt_id: metadata.receipt_id, receipt_digest: metadata.receipt_digest,
    effective_scope: metadata.project.effective_scope,
    editor_input: 'native-input.tti', input_sha256: sha256(input.buffer),
    sidecar: 'project.ersatzrs.json', sidecar_sha256: sha256(sidecar.buffer),
    editor_geometry: { left_panel_columns: 0, right_panel_columns: 0 },
    edit: operand ?? { row: 2, column: 8, code: 81 }, save_as: 'edited.tti', completion: 'editor-saved.json',
    editor: { name: 'QTeletextMaker', version: '1.0', source_commit: sourceCommit,
      binary_sha256: binaryHash, driver_sha256: driverHash,
      driver_mode: operand ? 'single-page-foreground' : 'single-page-zone-q' },
    publication: 'Only after native import, preview, explicit request and acceptance.',
  }, null, 2));
  expect(ready.length).toBeLessThanOrEqual(16_384);
  writeFileSync(join(directory, 'ready.json'), ready, { flag: 'wx' });
  await expect.poll(() => existsSync(join(directory, 'editor-saved.json')),
    { timeout: 90_000, intervals: [100] }).toBe(true);
  const marker = JSON.parse(boundedEditorFile(join(directory, 'editor-saved.json'), 4096).toString('utf8'));
  expect(Object.keys(marker).sort()).toEqual(['probe_version', 'editor_name', 'editor_version',
    'editor_source_commit', 'editor_binary_sha256', 'editor_driver_sha256', 'input_sha256',
    'saved_file', 'saved_sha256', 'saved_bytes'].sort());
  expect(marker).toMatchObject({ probe_version: 1, editor_name: 'QTeletextMaker', editor_version: '1.0',
    editor_source_commit: sourceCommit, editor_binary_sha256: binaryHash, editor_driver_sha256: driverHash,
    input_sha256: sha256(input.buffer), saved_file: 'edited.tti' });
  const edited = boundedEditorFile(join(directory, 'edited.tti'), 65_536);
  expect(marker.saved_sha256).toBe(sha256(edited)); expect(marker.saved_bytes).toBe(edited.length);
  expect(boundedEditorFile(join(directory, 'native-input.tti'), 65_536).equals(input.buffer)).toBe(true);
  expect(boundedEditorFile(join(directory, 'project.ersatzrs.json'), 65_536).equals(sidecar.buffer)).toBe(true);
  if (operand) assertOnlyOperandChanged(input.buffer, edited, operand);
  // The marker is an evidence handshake, not editor or publication authority.
  // Root independently observes the GUI and exact tool identities. Native
  // import still authenticates the unchanged sidecar against its stored receipt.
  return exported.map(file => file.name === input.name ? { ...file, buffer: edited } : file);
}

async function repeatEditorConcept(pages: Page[], fixture: TeletextChannelFixture, urls: string[], js: boolean,
  testInfo: { outputPath(name: string): string }, root: ReturnType<typeof storedDocument>,
  night: ReturnType<typeof storedDocument>): Promise<void> {
  const page = pages[0];
  await page.goto(urls[0]); await ready(page, js);
  const previous = storedDocument(fixture, fixture.channels[0]);
  expect(previous.DesignId).toBe(root.DesignId); expect(previous.Revision).toBe(root.Revision + 1);
  expect(publication(fixture).map(rows => rows.length)).toEqual([3, 2, 3, 2]);
  const beforePointers = publication(fixture);
  const beforeSafety = safety(fixture);
  const downloadReady = page.waitForEvent('download');
  await page.getByTestId('teletext-editor-export-all-form').getByRole('button', {
    name: translations('en_us')('TeletextEditorExportSet'), exact: true,
  }).click();
  const download = await downloadReady;
  expect(download.suggestedFilename()).toBe(`teletext-${previous.DesignId}-${previous.Revision}.zip`);
  const archive = testInfo.outputPath('effective-repeat-editor.zip'); await download.saveAs(archive);
  const exported = editorFiles(archive);
  const sidecar = exported.find(file => file.name === 'project.ersatzrs.json')!;
  expect(sidecar).toBeDefined();
  const metadata = JSON.parse(sidecar.buffer.toString('utf8'));
  expect(metadata.project.schema_version).toBe(2);
  expect(metadata.project.channel_id).toBe(fixture.channels[0]);
  expect(metadata.project.original).toEqual({ design_id: previous.DesignId, revision: previous.Revision,
    digest: previous.Digest, expected_head: previous.Revision, document_version: 2 });
  expect(metadata.project.producers[0].binding_revision).toBeNull();
  const page200 = exported.find(file => file.name === 'page-200-0000.tti')!;
  expect(page200).toBeDefined();
  const q = page200.buffer.toString('latin1').match(/^OL,2,([^\r\n]*)/m);
  expect(q).not.toBeNull(); expect(q![1][8]).toBe('Q');
  const edited = editRow(exported, 3, 'R');
  const retainedQ = edited.find(file => file.name === page200.name)!.buffer.toString('latin1').match(/^OL,2,([^\r\n]*)/m);
  expect(retainedQ).not.toBeNull(); expect(retainedQ![1]).toBe(q![1]);
  for (const file of exported.filter(file => file.name !== page200.name)) {
    expect(edited.find(candidate => candidate.name === file.name)!.buffer.equals(file.buffer)).toBe(true);
  }
  const directory = testInfo.outputPath('editor-repeat'); mkdirSync(directory);
  for (const file of edited) writeFileSync(join(directory, file.name), file.buffer, { flag: 'wx' });
  const form = page.getByTestId('teletext-editor-import-form');
  await expect(form).toHaveAttribute('enctype', 'multipart/form-data');
  await expect(form.locator('[name="expected_design_head"]')).toHaveValue(String(previous.Revision));
  await form.locator('[name="name"]').fill('Synthetic scoped second custom-zone edit');
  await form.locator('[name="sidecar"]').setInputFiles(edited.filter(file => file.name === 'project.ersatzrs.json'));
  await form.locator('[name="files"]').setInputFiles(edited.filter(file => file.name !== 'project.ersatzrs.json'));
  await submit(page, form, 'TeletextEditorImport', js);
  await expect(page.getByTestId('teletext-feedback')).toHaveAttribute('data-code', 'draft');
  urls[0] = page.url();
  const concept = storedDocument(fixture, fixture.channels[0]);
  expect(concept.DesignId).toBe(root.DesignId); expect(concept.Revision).toBe(root.Revision + 2);
  expect(concept.DocumentVersion).toBe(2); expect(concept.Digest).not.toBe(previous.Digest);
  const { editorial_overlay, ...immutable } = concept.document;
  // One cumulative overlay is rooted in the overlay-free original. A previous
  // concept's overlay must never become another nested original authority.
  expect(immutable).toEqual(root.document);
  const overlay = editorial_overlay as { schema_version: number; original_design_id: number;
    original_revision: number; original_digest: string; pages: { owner: { page: number; subcode: number };
      rows: { row: number; cells: number[] }[] }[] };
  expect(Object.keys(overlay).sort()).toEqual(['schema_version', 'export_receipt_id', 'export_receipt_digest',
    'original_design_id', 'original_revision', 'original_digest', 'pages'].sort());
  expect(overlay.schema_version).toBe(1);
  expect(overlay.original_design_id).toBe(root.DesignId); expect(overlay.original_revision).toBe(root.Revision);
  expect(overlay.original_digest).toBe(root.Digest);
  expect(overlay.pages).toHaveLength(1); expect(overlay.pages[0].owner).toEqual({ page: 200, subcode: 0 });
  expect(overlay.pages[0].rows.map(row => row.row)).toEqual([2, 3]);
  expect(overlay.pages[0].rows[0].cells[8]).toBe(81); expect(overlay.pages[0].rows[1].cells[8]).toBe(82);
  expect(storedDocument(fixture, fixture.channels[1])).toEqual(night);
  expect(publication(fixture)).toEqual(beforePointers); expect(safety(fixture)).toEqual(beforeSafety);
  expect(fixture.calls()).toEqual(['day', 'night']); expect(fixture.unchangedPackage()).toBe(true);
  await assertPreview(page, 'day');
  expect(await editPixels(page, 2)).toBeGreaterThan(0); expect(await editPixels(page, 3)).toBeGreaterThan(0);
  await expect(page.getByTestId('teletext-publication')).toHaveAttribute('data-state', 'unrequested');
  await pages[1].goto(urls[1]); await ready(pages[1], js); await assertPreview(pages[1], 'night');
  expect(await editPixels(pages[1], 2)).toBe(0); expect(await editPixels(pages[1], 3)).toBe(0);
  await page.getByTestId('teletext-preview-model').locator('img').scrollIntoViewIfNeeded();
  await page.screenshot({ path: testInfo.outputPath('effective-repeat-concept.png'), fullPage: false });
  await submit(page, page.getByTestId('teletext-publication-request-form'), 'TeletextPublicationRequest', js);
  expect(publication(fixture).map(rows => rows.length)).toEqual([4, 2, 3, 2]);
  await expect(page.getByTestId('teletext-publication')).toHaveAttribute('data-state', 'pending');
  await submit(page, page.getByTestId('teletext-publication-publish-form'), 'TeletextPublicationPublish', js);
  expect(publication(fixture).map(rows => rows.length)).toEqual([4, 2, 4, 2]);
  await expect(page.getByTestId('teletext-publication')).toHaveAttribute('data-state', 'selected');
  expect(safety(fixture)).toEqual(beforeSafety); expect(fixture.calls()).toEqual(['day', 'night']);
}

async function editorConcept(pages: Page[], fixture: TeletextChannelFixture, urls: string[], js: boolean,
  testInfo: { outputPath(name: string): string }, qtmDirectory?: string): Promise<void> {
  const repeat = process.env.ERSATZRS_TELETEXT_EFFECTIVE_REPEAT_EDITOR === '1';
  if (operandEditor) {
    expect(repeat, 'Operand GUI proof and cumulative Q/R mode are distinct fixtures.').toBe(false);
    expect(qtmDirectory, 'The positive operand proof requires an actual QTeletextMaker save.').toBeTruthy();
  }
  const page = pages[0];
  await page.goto(urls[0]); await ready(page, js);
  const original = storedDocument(fixture, fixture.channels[0]);
  const night = storedDocument(fixture, fixture.channels[1]);
  if (operandEditor) {
    assertOperandOriginal(original, 'day'); assertOperandOriginal(night, 'night');
    await assertOperandPreview(page, 'day', false);
  }
  const beforePointers = publication(fixture);
  const beforeSafety = safety(fixture);
  const downloadReady = page.waitForEvent('download');
  await page.getByTestId('teletext-editor-export-all-form').getByRole('button', {
    name: translations('en_us')('TeletextEditorExportSet'), exact: true,
  }).click();
  const download = await downloadReady;
  expect(download.suggestedFilename()).toBe(`teletext-${original.DesignId}-${original.Revision}.zip`);
  const archive = testInfo.outputPath('effective-original-editor.zip'); await download.saveAs(archive);
  const exported = editorFiles(archive);
  expect(exported.filter(file => file.name.endsWith('.tti')).map(file => file.name).sort())
    .toEqual(['page-200-0000.tti', 'page-201-0000.tti']);
  const sidecar = exported.find(file => file.name === 'project.ersatzrs.json')!;
  expect(sidecar).toBeDefined();
  const metadata = JSON.parse(sidecar.buffer.toString('utf8'));
  expect(metadata.schema_version).toBe(1);
  expect(metadata.project.schema_version).toBe(2);
  expect(metadata.project.channel_id).toBe(fixture.channels[0]);
  expect(metadata.project.original.document_version).toBe(2);
  expect(metadata.project.original.digest).toBe(original.Digest);
  if (operandEditor) {
    expect(metadata.project.original).toEqual({ design_id: original.DesignId, revision: original.Revision,
      digest: original.Digest, expected_head: original.Revision, document_version: 2 });
    expect(metadata.receipt_id).toBeGreaterThan(0); expect(metadata.receipt_digest).toMatch(/^[a-f0-9]{64}$/);
  }
  expect(metadata.project.effective_scope.binding_count).toBe(1);
  const scope = original.document.effective_selection as { valid_until: string };
  // Chrono's public RFC3339 projection uses +00:00 while stored UTC JSON may
  // use Z. Preserve every fractional digit and the exact original boundary.
  expect(metadata.project.effective_scope.valid_until.replace(/\+00:00$/, 'Z'))
    .toBe(scope.valid_until.replace(/\+00:00$/, 'Z'));
  expect(metadata.project.producers).toHaveLength(1);
  const producer = metadata.project.producers[0];
  expect(producer.binding_revision).toBeNull();
  expect(producer.effective_origin).toEqual(expect.objectContaining({ origin: 'scripted', binding_index: 0, playout_id: fixture.channels[0] }));
  expect(producer.authoring.template).toEqual({ id: 'synthetic-editor', revision: 'r1' });
  expect(producer.authoring.theme).toEqual({ id: 'day', overrides: [] });
  expect(producer.authoring.pages[0].regions.map((region: { kind: string }) => region.kind)).toEqual(['free', 'zone', 'generated']);
  expect(producer.authoring.pages[0].dynamic_fields).toEqual([{ id: 'guide-title', region: 'guide-content', binding: 'guide.title' }]);
  const edited = qtmDirectory ? await replayEditor(qtmDirectory, exported, metadata)
    : repeat ? editRow(exported, 2, 'Q') : editRow(editRow(exported, 2, 'Q'), 3, 'R');
  expect(edited.find(file => file.name === 'project.ersatzrs.json')!.buffer.equals(sidecar.buffer)).toBe(true);
  for (const file of exported.filter(file => file.name !== 'page-200-0000.tti')) {
    expect(edited.find(candidate => candidate.name === file.name)!.buffer.equals(file.buffer)).toBe(true);
  }
  for (const file of edited) writeFileSync(testInfo.outputPath(file.name), file.buffer, { flag: 'wx' });
  const form = page.getByTestId('teletext-editor-import-form');
  await expect(form).toHaveAttribute('enctype', 'multipart/form-data');
  await form.locator('[name="name"]').fill(operandEditor ? 'Synthetic scoped actual QTM foreground operand'
    : repeat ? 'Synthetic scoped first free edit' : 'Synthetic scoped free and custom-zone edit');
  await form.locator('[name="sidecar"]').setInputFiles(edited.filter(file => file.name === 'project.ersatzrs.json'));
  await form.locator('[name="files"]').setInputFiles(edited.filter(file => file.name !== 'project.ersatzrs.json'));
  await submit(page, form, 'TeletextEditorImport', js);
  await expect(page.getByTestId('teletext-feedback')).toHaveAttribute('data-code', 'draft');
  urls[0] = page.url();
  const concept = storedDocument(fixture, fixture.channels[0]);
  expect(concept.DesignId).toBe(original.DesignId); expect(concept.Revision).toBe(original.Revision + 1);
  expect(concept.DocumentVersion).toBe(2); expect(concept.Digest).not.toBe(original.Digest);
  const { editorial_overlay, ...immutable } = concept.document;
  expect(editorial_overlay).toBeDefined(); expect(immutable).toEqual(original.document);
  if (operandEditor) {
    expect(editorial_overlay).toEqual({ schema_version: 2,
      export_receipt_id: metadata.receipt_id, export_receipt_digest: metadata.receipt_digest,
      original_design_id: original.DesignId, original_revision: original.Revision, original_digest: original.Digest,
      pages: [], local_program_operands: [{ owner: { page: 200, subcode: 0 }, command: 4, column: 8,
        expected: { kind: 'foreground', colour: 18 }, replacement: { kind: 'foreground', colour: 4 } }],
    });
  }
  expect(storedDocument(fixture, fixture.channels[1])).toEqual(night);
  expect(publication(fixture)).toEqual(beforePointers); expect(safety(fixture)).toEqual(beforeSafety);
  expect(fixture.calls()).toEqual(['day', 'night']); expect(fixture.unchangedPackage()).toBe(true);
  await assertPreview(page, 'day');
  if (operandEditor) await assertOperandPreview(page, 'day', true);
  else {
    expect(await editPixels(page, 2)).toBeGreaterThan(0);
    if (qtmDirectory || repeat) expect(await editPixels(page, 3)).toBe(0);
    else expect(await editPixels(page, 3)).toBeGreaterThan(0);
  }
  await expect(page.getByTestId('teletext-publication')).toHaveAttribute('data-state', 'unrequested');
  await pages[1].goto(urls[1]); await ready(pages[1], js); await assertPreview(pages[1], 'night');
  if (operandEditor) await assertOperandPreview(pages[1], 'night', false);
  else { expect(await editPixels(pages[1], 2)).toBe(0); expect(await editPixels(pages[1], 3)).toBe(0); }
  await page.getByTestId('teletext-preview-model').locator('img').scrollIntoViewIfNeeded();
  await page.screenshot({ path: testInfo.outputPath('effective-edited-concept.png'), fullPage: false });
  if (operandEditor) await localizedOperand(page, urls[0], 'unrequested', js, testInfo);
  await submit(page, page.getByTestId('teletext-publication-request-form'), 'TeletextPublicationRequest', js);
  expect(publication(fixture).map(rows => rows.length)).toEqual([3, 2, 2, 2]);
  await expect(page.getByTestId('teletext-publication')).toHaveAttribute('data-state', 'pending');
  await submit(page, page.getByTestId('teletext-publication-publish-form'), 'TeletextPublicationPublish', js);
  expect(publication(fixture).map(rows => rows.length)).toEqual([3, 2, 3, 2]);
  await expect(page.getByTestId('teletext-publication')).toHaveAttribute('data-state', 'selected');
  expect(safety(fixture)).toEqual(beforeSafety); expect(fixture.calls()).toEqual(['day', 'night']);
  if (operandEditor) await localizedOperand(page, page.url(), 'selected', js, testInfo);
  if (repeat) await repeatEditorConcept(pages, fixture, urls, js, testInfo, original, night);
}

async function localized(page: Page, fixture: TeletextChannelFixture, url: string, theme: 'day' | 'night',
  state: 'unrequested' | 'pending' | 'selected', js: boolean, dark: boolean,
  testInfo: { outputPath(name: string): string }): Promise<void> {
  for (const [culture, module] of cultures) {
    await language(page, culture, dark); await page.goto(url); await ready(page, js);
    const text = translations(module);
    await expect(page.locator('[data-theme]').first()).toHaveAttribute('data-theme', dark ? 'dark' : 'light');
    const publishAdvanced = page.getByTestId('teletext-publish-advanced');
    if (await publishAdvanced.getAttribute('open') === null) await publishAdvanced.locator(':scope > summary').click();
    const active = page.getByTestId('teletext-effective-authoring');
    await expect(active).toHaveAttribute('data-state', 'available');
    await expect(active.getByRole('heading')).toHaveText(text('TeletextEffectiveAuthoringTitle'));
    await expect(active).toContainText(text('TeletextEffectiveAuthoringHelp'));
    await expect(active).toContainText(text('TeletextEffectiveScopeExpiry'));
    await expect(active.locator('[name="entry_addon_id"] option').filter({ hasText: 'Synthetic teletext themes' })).toHaveAttribute('value', fixture.addonId);
    await expect(page.getByTestId('teletext-authoring-form').getByRole('button')).toBeDisabled();
    await assertPreview(page, theme);
    const publicationView = page.getByTestId('teletext-publication');
    await expect(publicationView).toHaveAttribute('data-state', state);
    const key = { unrequested: 'TeletextPublicationUnrequested', pending: 'TeletextPublicationPending', selected: 'TeletextPublicationSelected' }[state];
    await expect(page.getByTestId('teletext-publication-status')).toHaveText(text(key));
    if (state === 'selected') {
      await expect(page.getByTestId('teletext-publication-publish-form').getByRole('button')).toBeDisabled();
      // Selection is not a fabricated sink acknowledgement.
      await expect(publicationView).not.toHaveAttribute('data-state', 'applied');
    }
    const editor = page.getByTestId('teletext-editor-controls');
    await expect(editor).toHaveAttribute('data-state', 'available');
    await expect(editor).toContainText(text('TeletextEditorHelp'));
    await expect(editor).toContainText(text('TeletextEditorOwnershipHelp'));
    await expect(editor).toContainText(text('TeletextEditorTemplateHelp'));
    await expect(editor.getByTestId('teletext-editor-export-all-form').getByRole('button'))
      .toHaveText(text('TeletextEditorExportSet'));
    await expect(editor.getByTestId('teletext-editor-export-page-form').getByRole('button'))
      .toHaveText(text('TeletextEditorExportPage'));
    await expect(editor.getByTestId('teletext-editor-import-form').getByRole('button', {
      name: text('TeletextEditorImport'), exact: true,
    }))
      .toHaveText(text('TeletextEditorImport'));
    await expect(editor.getByTestId('teletext-resource-bank-add-form'))
      .toHaveAttribute('action', '/admin-api/teletext/design/add-resource-bank');
    await expect(editor.getByTestId('teletext-resource-bank-add-form').getByRole('button'))
      .toHaveText(text('TeletextResourceBankAction'));
    await expect(editor.locator('form')).toHaveCount(4);
    await page.getByTestId('teletext-preview-model').locator('ul a').click(); await ready(page, js);
    await assertPreview(page, theme, '201');
    if ((culture === 'en-us' || culture === 'nl') && state === 'selected') {
      const target = page.getByTestId('teletext-publication-status');
      await target.scrollIntoViewIfNeeded(); await expect(target).toBeInViewport();
      await page.screenshot({ path: testInfo.outputPath(`${culture}-${theme}-selected.png`), fullPage: false });
    }
  }
}
async function responsive(page: Page, testInfo: { outputPath(name: string): string }): Promise<void> {
  await page.setViewportSize({ width: 320, height: 900 });
  const form = page.getByTestId('teletext-effective-authoring-form');
  const publishAdvanced = page.getByTestId('teletext-publish-advanced');
  if (await publishAdvanced.getAttribute('open') === null) await publishAdvanced.locator(':scope > summary').click();
  const name = form.locator('[name="name"]'); await name.focus(); await page.keyboard.press('Tab');
  const producer = form.locator('[name="entry_addon_id"]'); await expect(producer).toBeFocused();
  await expect.poll(() => producer.evaluate(element => Number.parseFloat(getComputedStyle(element).outlineWidth))).toBeGreaterThan(0);
  await producer.scrollIntoViewIfNeeded(); await expect(producer).toBeInViewport();
  await expect.poll(() => page.evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1)).toBe(true);
  await page.screenshot({ path: testInfo.outputPath('effective-320-keyboard.png'), fullPage: false });
  await page.setViewportSize({ width: 640, height: 900 });
  await page.evaluate(() => { document.documentElement.style.zoom = '2'; });
  await producer.scrollIntoViewIfNeeded(); await expect(producer).toBeInViewport();
  await expect.poll(() => page.evaluate(() => document.documentElement.getBoundingClientRect().right <= innerWidth + 1)).toBe(true);
  await page.screenshot({ path: testInfo.outputPath('effective-200-percent.png'), fullPage: false });
  await page.evaluate(() => { document.documentElement.style.zoom = ''; });
  await page.setViewportSize({ width: 1280, height: 900 });
}

test.describe.configure({ mode: 'serial' });
for (const js of [true, false]) for (const dark of [false, true]) {
  test(`effective Scripted authoring through real Lua publication (JavaScript ${js}, dark ${dark})`, async ({ browser, baseURL }, testInfo) => {
    test.skip(!databasePath, 'Requires the runner disposable E2E_DB_PATH.');
    test.skip(process.platform === 'win32', 'Intentional POSIX synthetic producer; no Windows acceptance claim.');
    test.skip(!process.env.FFMPEG_PATH, 'Requires the explicitly selected FFmpeg 9 playback fixture.');
    test.setTimeout(480_000);
    testInfo.annotations.push({ type: 'browser-version', description: browser.version() });
    const repeatEditor = process.env.ERSATZRS_TELETEXT_EFFECTIVE_REPEAT_EDITOR === '1';
    if (operandEditor && repeatEditor) throw new Error('The operand GUI proof cannot use cumulative Q/R editor mode.');
    test.skip(operandEditor && !(js && !dark && testInfo.project.name === 'chromium'),
      'Explicit operand GUI opt-in is scoped to headed Chromium with JavaScript and the light theme.');
    if (repeatEditor) testInfo.annotations.push({ type: 'repeat-editor', description: 'Two explicit export/import/publication rounds preserve one immutable Scripted original and cumulative Q/R edits.' });
    const qtmDirectory = js && !dark && testInfo.project.name === 'chromium'
      ? process.env.ERSATZRS_TELETEXT_EFFECTIVE_QTM_REPLAY_DIR : undefined;
    if (operandEditor && !qtmDirectory) throw new Error('The operand GUI proof requires a fresh actual QTeletextMaker replay directory.');
    if (operandEditor) testInfo.annotations.push({ type: 'operand-editor', description:
      'Fresh authenticated page200/0000 export; fixture-qualified physical X26 selection, actual GUI FG18-to4 save, native command4 overlay, English/Dutch concept and publication preview.' });
    if (qtmDirectory) testInfo.annotations.push({ type: 'editor-workflow', description: 'Actual separately observed pinned QTeletextMaker save is required before native import; the marker alone is not acceptance.' });
    testInfo.annotations.push({ type: 'scripted-setup', description: 'The existing hydrated Playouts Reset action prepares Lua scopes; the authoring pages retain their configured JavaScript/no-JS mode.' });
    const fixture = new TeletextChannelFixture(databasePath!, { authoredPages: true, editorPages: true,
      editorOperand: operandEditor, playback: { ffmpeg: process.env.FFMPEG_PATH! } });
    // The real Playouts search matches Channel.Name, not Channel.Number.
    // Unique initial owned names avoid ten-row pagination collisions with
    // retained fixtures; all later authoring/publication must preserve them.
    for (const channel of fixture.channels) {
      fixture.database.prepare('UPDATE "Channel" SET "Name"=? WHERE "Id"=?')
        .run(`Synthetic effective teletext ${channel}`, channel);
    }
    fixture.configureScripted(); fixture.setAuthoringPolicy();
    const capturePrefix = js && !dark && process.env.ERSATZRS_TELETEXT_EFFECTIVE_ADMIN_CAPTURE
      ? `${process.env.ERSATZRS_TELETEXT_EFFECTIVE_ADMIN_CAPTURE}.${testInfo.project.name}` : undefined;
    if (capturePrefix) {
      const policyFile = fileURLToPath(new URL('../../crates/ersatzrs-web/tests/fixtures/teletext-output-policy-v1.json', specUrl));
      const policy = JSON.parse(readFileSync(policyFile, 'utf8'));
      policy.version = 2;
      policy.live_updates = { poll_interval_ms: 25, recovery_timeout_ms: 2000 };
      fixture.setOutputPolicy(JSON.stringify(policy));
      mkdirSync(dirname(capturePrefix), { recursive: true });
      copyFileSync(join(fixture.root, 'synthetic-client-source.ts'), `${capturePrefix}.source.ts`, constants.COPYFILE_EXCL);
    }
    const foreignBefore = foreign(fixture);
    const context = await browser.newContext({ baseURL, javaScriptEnabled: js, viewport: { width: 1280, height: 900 } });
    const pages = [await context.newPage(), await context.newPage()];
    const schedulingContext = await browser.newContext({ baseURL, javaScriptEnabled: true, viewport: { width: 1280, height: 900 } });
    const schedulingPage = await schedulingContext.newPage();
    const errors: string[] = [];
    for (const page of [...pages, schedulingPage]) {
      page.on('pageerror', error => errors.push(error.message));
      page.on('console', message => { if (message.type() === 'error') errors.push(message.text()); });
    }
    try {
      await language(pages[0], 'en-us', dark);
      await pages[0].goto(`/channels/${fixture.channels[0]}/teletext`); await ready(pages[0], js);
      const day = await profile(pages[0], fixture, 'day', js);
      const night = await profile(pages[0], fixture, 'night', js);
      for (const [index, id] of [[0, day], [1, night]] as const) {
        await baseOff(pages[index], fixture, fixture.channels[index], id, js);
        await resetScript(schedulingPage, fixture, fixture.channels[index], id, 80);
      }
      expect(fixture.calls()).toEqual([]);
      const safe = safety(fixture);
      const urls: string[] = [];
      for (const [index, theme] of [[0, 'day'], [1, 'night']] as const) {
        await pages[index].goto(`/channels/${fixture.channels[index]}/teletext`); await ready(pages[index], js);
        urls.push(await prepare(pages[index], fixture, theme, js));
        await assertPreview(pages[index], theme);
      }
      expect(fixture.calls()).toEqual(['day', 'night']);
      expect(fixture.designs()).toHaveLength(2);
      for (const channel of fixture.channels) {
        const designs = fixture.designs(channel);
        expect(designs).toHaveLength(1);
        expect(designs[0].ChannelId).toBe(channel);
      }
      expect(fixture.request('day')?.channels).toEqual([]);
      expect(fixture.request('night')?.channels).toEqual([]);
      expect(publication(fixture).map(rows => rows.length)).toEqual([0, 0, 0, 0]);
      const versions = fixture.database.prepare('SELECT "DocumentVersion" FROM "TeletextDesignRevision" WHERE "DesignId" IN (?,?) ORDER BY "DesignId"')
        .all(...fixture.designs().map(design => design.Id));
      expect(versions).toEqual([{ DocumentVersion: 2 }, { DocumentVersion: 2 }]);
      for (const [index, theme] of [[0, 'day'], [1, 'night']] as const) await localized(pages[index], fixture, urls[index], theme, 'unrequested', js, dark, testInfo);
      expect(safety(fixture)).toEqual(safe);
      await language(pages[0], 'en-us', dark);
      for (const [index, page] of pages.entries()) {
        await page.goto(urls[index]); await ready(page, js);
        await submit(page, page.getByTestId('teletext-publication-request-form'), 'TeletextPublicationRequest', js);
        await expect(page.getByTestId('teletext-publication')).toHaveAttribute('data-state', 'pending');
      }
      expect(publication(fixture).map(rows => rows.length)).toEqual([2, 2, 0, 0]);
      if (capturePrefix) {
        const beforeCapture = publication(fixture);
        await Promise.all(fixture.channels.map((channel, index) =>
          captureOrdinary(fixture, baseURL!, channel, `${capturePrefix}.pending.${index === 0 ? 'day' : 'night'}.ts`, false)));
        expect(publication(fixture)).toEqual(beforeCapture);
        expect(safety(fixture)).toEqual(safe);
      }
      for (const [index, theme] of [[0, 'day'], [1, 'night']] as const) await localized(pages[index], fixture, urls[index], theme, 'pending', js, dark, testInfo);
      await language(pages[0], 'en-us', dark);
      for (const [index, page] of pages.entries()) {
        await page.goto(urls[index]); await ready(page, js);
        await page.getByTestId('teletext-preview-model').locator('ul a').click(); await ready(page, js);
        await assertPreview(page, index === 0 ? 'day' : 'night', '201');
        await submit(page, page.getByTestId('teletext-publication-publish-form'), 'TeletextPublicationPublish', js);
        await expect(page.getByTestId('teletext-feedback')).toHaveAttribute('data-code', 'publication-selected');
      }
      expect(publication(fixture).map(rows => rows.length)).toEqual([2, 2, 2, 2]);
      for (const [index, theme] of [[0, 'day'], [1, 'night']] as const) await localized(pages[index], fixture, urls[index], theme, 'selected', js, dark, testInfo);
      expect(safety(fixture)).toEqual(safe);
      expect(fixture.calls()).toEqual(['day', 'night']);
      await responsive(pages[0], testInfo);
      await language(pages[0], 'en-us', dark);
      await editorConcept(pages, fixture, urls, js, testInfo, qtmDirectory);
      expect(publication(fixture).map(rows => rows.length)).toEqual(repeatEditor ? [4, 2, 4, 2] : [3, 2, 3, 2]);
      if (capturePrefix) {
        const beforeCapture = publication(fixture);
        await Promise.all(fixture.channels.map((channel, index) =>
          captureOrdinary(fixture, baseURL!, channel, `${capturePrefix}.selected.${index === 0 ? 'day' : 'night'}.ts`, true)));
        expect(publication(fixture)).toEqual(beforeCapture);
        expect(safety(fixture)).toEqual(safe);
      }
      await resetScript(schedulingPage, fixture, fixture.channels[0], null, capturePrefix ? 8 : 4);
      await pages[0].goto(`/channels/${fixture.channels[0]}/teletext`); await ready(pages[0], js);
      await expect(pages[0].getByTestId('teletext-effective-authoring')).toHaveAttribute('data-state', 'absent');
      const absentUrl = await prepare(pages[0], fixture, 'absent', js, true);
      expect(new URL(absentUrl).searchParams.get('absence')).toBe('1');
      expect(new URL(absentUrl).searchParams.has('page')).toBe(false);
      if (capturePrefix) {
        const beforeRejection = publication(fixture);
        const fields = await pages[0].getByTestId('teletext-publication-publish-form').evaluate(element =>
          Object.fromEntries(new FormData(element as HTMLFormElement).entries()) as Record<string, string>);
        const rejected = await pages[0].request.post('/admin-api/teletext/design/publish', {
          form: { ...fields, digest: '0'.repeat(64) }, maxRedirects: 0,
        });
        expect(rejected.status()).toBe(303);
        expect(rejected.headers().location).toContain('error=publication-conflict');
        expect(publication(fixture)).toEqual(beforeRejection);
        await captureOrdinary(fixture, baseURL!, fixture.channels[0], `${capturePrefix}.rejected.ts`, false);
        expect(publication(fixture)).toEqual(beforeRejection);
      }
      await submit(pages[0], pages[0].getByTestId('teletext-publication-request-form'), 'TeletextPublicationRequest', js);
      await submit(pages[0], pages[0].getByTestId('teletext-publication-publish-form'), 'TeletextPublicationPublish', js);
      const selected = publication(fixture);
      expect(selected.map(rows => rows.length)).toEqual(repeatEditor ? [5, 2, 5, 2] : [4, 2, 4, 2]);
      if (capturePrefix) {
        await captureOrdinary(fixture, baseURL!, fixture.channels[0], `${capturePrefix}.off.ts`, false);
        expect(publication(fixture)).toEqual(selected);
      }
      for (const [culture, module] of cultures) {
        await language(pages[0], culture, dark); await pages[0].goto(absentUrl); await ready(pages[0], js);
        const text = translations(module);
        const model = pages[0].getByTestId('teletext-preview-model');
        await expect(model).toHaveAttribute('data-scope', 'effective');
        await expect(model).not.toHaveAttribute('data-page');
        await expect(model.locator('img')).toHaveCount(0);
        await expect(model.getByTestId('teletext-preview-absence')).toHaveText(text('TeletextPreviewAbsent'));
        await expect(pages[0].getByTestId('teletext-effective-authoring')).toContainText(text('TeletextEffectiveAuthoringAbsent'));
        await expect(pages[0].getByTestId('teletext-publication-publish-form').locator('[name="preview_absence"]')).toHaveValue('1');
        for (const name of ['preview_page', 'preview_subcode']) await expect(model.locator(`[name="${name}"]`)).toHaveCount(0);
      }
      const expires = utcMilliseconds(fixture.scriptedIntervals(fixture.channels[0])[0].Finish);
      await expect.poll(() => Date.now() > expires, { timeout: capturePrefix ? 105_000 : 55_000 }).toBe(true);
      await pages[0].goto(absentUrl); await ready(pages[0], js);
      await expect(pages[0].getByTestId('teletext-preview-model')).toBeVisible();
      await expect(pages[0].getByTestId('teletext-preview-model').locator('img')).toHaveCount(0);
      await expect(pages[0].getByTestId('teletext-publication')).toHaveAttribute('data-state', 'unavailable');
      await expect(pages[0].getByTestId('teletext-publication-publish-form').getByRole('button')).toBeDisabled();
      const rejectedFields = await pages[0].getByTestId('teletext-publication-publish-form').evaluate(element =>
        Object.fromEntries(new FormData(element as HTMLFormElement).entries()) as Record<string, string>);
      const rejection = await pages[0].request.post('/admin-api/teletext/design/publish', { form: rejectedFields, maxRedirects: 0 });
      expect(rejection.status()).toBe(303); expect(rejection.headers().location).toMatch(/error=(expired|unavailable|publication-conflict)/);
      expect(publication(fixture)).toEqual(selected);
      await pages[1].goto(urls[1]); await ready(pages[1], js); await assertPreview(pages[1], 'night');
      await expect(pages[1].getByTestId('teletext-publication')).toHaveAttribute('data-state', 'selected');
      expect(fixture.calls()).toEqual(['day', 'night']);
      expect(fixture.unchangedPackage()).toBe(true); expect(foreign(fixture)).toEqual(foreignBefore); expect(errors).toEqual([]);
    } finally {
      try {
        await pages[0].request.post('/admin-api/settings/ui/save', { form: {
          language: fixture.originalUi.get('pages.language') ?? 'en-us',
          is_dark_mode: String(/^(1|true)$/i.test(fixture.originalUi.get('pages.is_dark_mode') ?? 'True')),
        }, maxRedirects: 0 });
      } finally {
        try { await context.close(); }
        finally {
          try { await schedulingContext.close(); }
          finally {
            const published = publication(fixture);
            const foreignRows = foreign(fixture);
            const database = new DatabaseSync(databasePath!);
            database.exec('PRAGMA busy_timeout=10000;');
            const designs = fixture.designs().map(design => design.Id);
            const revisions = () => designs.map(id => database.prepare(
              'SELECT * FROM "TeletextDesignRevision" WHERE "DesignId"=? ORDER BY "Revision"').all(id));
            const otherPlayouts = () => database.prepare(
              'SELECT * FROM "Playout" WHERE "ChannelId" NOT IN (?,?) ORDER BY "Id"').all(...fixture.channels);
            const originalRevisions = revisions();
            const originalOtherPlayouts = otherPlayouts();
            try {
              fixture.close();
              expect(publication(fixture, database)).toEqual(published);
              expect(foreign(fixture, database)).toEqual(foreignRows);
              expect(revisions()).toEqual(originalRevisions);
              expect(otherPlayouts()).toEqual(originalOtherPlayouts);
              expect(database.prepare('SELECT COUNT(*) AS count FROM "Playout" WHERE "ChannelId" IN (?,?)')
                .get(...fixture.channels)).toEqual({ count: 0 });
              expect(database.prepare('SELECT COUNT(*) AS count FROM "PlayoutTeletextInterval" WHERE "PlayoutId" IN (?,?)')
                .get(...fixture.channels)).toEqual({ count: 0 });
            } finally { database.close(); }
          }
        }
      }
    }
  });
}
