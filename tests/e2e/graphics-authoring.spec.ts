import { expect, test, Page, Locator, TestInfo } from '@playwright/test';
import { execFileSync } from 'node:child_process';
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { createHash } from 'node:crypto';

const database=process.env.E2E_DB_PATH;
const ffmpeg=process.env.ErsatzRS_FFMPEG__PATH ?? 'ffmpeg';
function sql(statement:string):string {
  if(!database) throw new Error('An isolated database is required');
  return execFileSync('sqlite3',[database],{input:`.timeout 10000\n${statement}`,encoding:'utf8'}).trim();
}
function quoted(value:string):string {return `'${value.replaceAll("'","''")}'`;}
function assets(info:TestInfo) {
  const root=join(dirname(database!),`authoring-${info.project.name}`);mkdirSync(root,{recursive:true});
  const image=join(root,'alpha.png'),motion=join(root,'motion.nut'),subtitle=join(root,'title.ass');
  execFileSync(ffmpeg,['-v','error','-y','-f','lavfi','-i','color=c=red@0.5:s=128x80:r=25,format=rgba','-frames:v','1',image],{timeout:15000});
  execFileSync(ffmpeg,['-v','error','-y','-f','lavfi','-i','color=c=green@0.5:s=128x80:r=25,format=bgra','-t','2','-an','-c:v','ffv1','-pix_fmt','bgra',motion],{timeout:15000});
  writeFileSync(subtitle,`[Script Info]\nScriptType: v4.00+\nPlayResX: 320\nPlayResY: 180\n[V4+ Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\nStyle: Default,Roboto,20,&H00FFFFFF,&H000000FF,&H00000000,&H00000000,0,0,0,0,100,100,0,0,1,0,0,7,8,8,8,1\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\nDialogue: 0,0:00:00.00,0:00:30.00,Default,,0,0,0,,{{ Caption }}\n`);
  return {image,motion,subtitle};
}
async function language(page:Page,code:string) {
  await page.request.post('/admin-api/settings/ui/save',{form:{is_dark_mode:'true',language:code},maxRedirects:0});
}
async function hydrated(page:Page) {
  await expect.poll(()=>page.evaluate(()=>(window as Window & {__ersatzrs_hydrated?:boolean}).__ersatzrs_hydrated===true)).toBe(true);
}
async function openFields(form:Locator,kind:string,js=false) {
  await form.locator('[name="kind"]').selectOption(kind);
  const group=form.locator(`[data-kind-fields="${kind}"]`);
  if (js) await expect(group).toHaveAttribute('open', '');
  else if(await group.getAttribute('open')===null) await group.locator('summary').click();
}
async function field(form:Locator,name:string,value:string) {await form.locator(`[name="${name}"]`).fill(value);}
async function preview(page:Page,form:Locator,info:TestInfo,label:string,expectVisible=true):Promise<Buffer> {
  const iframe = form.locator('[data-graphics-render-frame]');
  // Firefox can defer an offscreen initial document; exercise the visible,
  // initialized preview that the user inspects before checking its new image.
  await iframe.scrollIntoViewIfNeeded();
  const frame=iframe.contentFrame();
  await frame.locator('body').waitFor({ state: 'attached', timeout: 5000 });
  // Compare the displayed image with this exact POST response; Firefox may not
  // emit a frame-navigation event for an already named browsing context.
  const response=page.waitForResponse(response=>response.url().includes('/admin-api/graphics-elements/author') && response.request().postData()?.includes('intent=preview')===true, { timeout: 15000 });
  await form.locator('[data-graphics-render-submit]').click();
  const rendered = await response; expect(rendered.status()).toBe(200);
  const freshSource = (await rendered.text()).match(/src="(data:image\/png;base64,[^"]+)"/)?.[1];
  expect(freshSource).toBeTruthy();
  const image=frame.locator('[data-graphics-render-output]');
  let displayDeadline: ReturnType<typeof setTimeout> | undefined;
  try {
    await Promise.race([
      expect(image).toHaveAttribute('src', freshSource!, { timeout: 5000 }),
      new Promise((_, reject) => { displayDeadline = setTimeout(() => reject(new Error('Native preview did not display its returned PNG within eight seconds')), 8000); }),
    ]);
  } finally { clearTimeout(displayDeadline); }
  await expect(image).toBeVisible();
  const png=Buffer.from((await image.getAttribute('src'))!.split(',')[1],'base64');
  expect(png.subarray(0,8)).toEqual(Buffer.from([137,80,78,71,13,10,26,10]));
  const pixels=execFileSync(ffmpeg,['-v','error','-f','image2pipe','-i','pipe:0','-frames:v','1','-f','rawvideo','-pix_fmt','rgba','pipe:1'],{input:png,timeout:15000,maxBuffer:8*1024*1024});
  expect(pixels.length).toBe(320*180*4);
  let visible=0;for(let i=3;i<pixels.length;i+=4) if(pixels[i]>0) visible++;
  if (expectVisible) expect(visible,`${label} actual native canvas is empty`).toBeGreaterThan(20);
  else expect(visible,`${label} expired canvas is not transparent`).toBe(0);
  writeFileSync(info.outputPath(`${label}.png`),png);
  return png;
}

for(const culture of ['en-us','nl']) for(const js of [true,false]) {
  test(`finding 02 all-kind authoring and real preview (${culture}, ${js?'hydrated':'no JS'})`,async({browser},info)=>{
    test.skip(!database,'Requires a disposable migrated database');test.setTimeout(180000);
    const context=await browser.newContext({javaScriptEnabled:js,baseURL:process.env.E2E_BASE_URL});
    const page=await context.newPage();const errors:string[]=[];
    page.on('pageerror', error => { if (!(error.name === 'AbortError' && /operation was aborted/i.test(error.message))) errors.push(error.message); });
    page.on('console', message => { if (message.type() === 'error' && /failed_to_cast_marker_node|hydration|rust_panic/i.test(message.text())) errors.push(message.text()); });
    await language(page,culture);const fixture=assets(info);
    for(const kind of ['Image','Text','Subtitle','Motion','Script']) {
      await page.goto('/graphics-elements/add');if(js)await hydrated(page);
      const form=page.locator('[data-graphics-element-editor]').filter({visible:true});
      const name=`Author ${info.project.name} ${culture} ${js} ${kind}`;
      await field(form,'name',name);await openFields(form,kind,js);
      await field(form,'location_x','32');await field(form,'location_y','40');await field(form,'opacity_percent','100');await field(form,'z_index','7');await field(form,'epg_entries','8');
      if(kind==='Image')await field(form,'image',fixture.image);
      if(kind==='Motion') {await field(form,'video_path',fixture.motion);await field(form,'start_seconds','0');await form.locator('[name="end_behavior"]').selectOption('Loop');}
      if(kind==='Subtitle')await field(form,'template',fixture.subtitle);
      if(kind==='Script')await field(form,'command',"return 'setcolor green\\nrect 8 8 80 80 fill\\n'");
      if(kind==='Text') {
        await form.locator('[name="renderer"]').selectOption('drawtext');
        await field(form,'text','Épisode très long — naïve façade & Straße: actual font metrics');
        await field(form,'font_file','assets/fonts/Roboto-Regular.ttf');await field(form,'font_size','20');
        await field(form, 'text_color', 'white');
        await expect(form).toContainText(culture==='nl'?'Drawtext gebruikt het gekozen echte lettertype op vaste grootte':'Drawtext uses the selected real font at a fixed size');
      }
      await field(form,'preview_width','320');await field(form,'preview_height','180');await field(form,'preview_variables','{"Caption":"Épisode native subtitle"}');
      const literalTimestamp = '<img src=x onerror="throw new Error(\'unsafe timestamp\')">';
      if (kind === 'Script') await field(form, 'preview_variables', JSON.stringify({ Programme_LocalTime: literalTimestamp }));
      const countBefore=sql('SELECT COUNT(*) FROM "GraphicsElement";');
      const first=await preview(page,form,info,`${kind}-${culture}-${js}`);
      if (kind === 'Script') {
        const frame = form.locator('[data-graphics-render-frame]').contentFrame();
        await expect(frame.locator('[data-graphics-render-time]')).toHaveText(literalTimestamp);
        await expect(frame.locator('[data-graphics-render-time] img')).toHaveCount(0);
        await field(form, 'preview_variables', '{}');
      }
      expect(sql('SELECT COUNT(*) FROM "GraphicsElement";')).toBe(countBefore);
      if (kind === 'Subtitle') {
        await field(form, 'preview_variables', '{"Caption":"Changed actual subtitle"}');
        const changed = await preview(page, form, info, `Subtitle-changed-${culture}-${js}`);
        expect(createHash('sha256').update(changed).digest('hex')).not.toBe(createHash('sha256').update(first).digest('hex'));
        await field(form, 'preview_variables', '{"Caption":"Épisode native subtitle"}');
      }
      if(kind==='Text') {
        await field(form,'font_file','assets/fonts/Sen.ttf');
        const second=await preview(page,form,info,`Text-Sen-${culture}-${js}`);
        expect(createHash('sha256').update(second).digest('hex')).not.toBe(createHash('sha256').update(first).digest('hex'));
      }
      if (kind === 'Image') {
        await field(form, 'opacity_expression', 'between(content_seconds,1,3)');
        await preview(page, form, info, `Image-before-window-${culture}-${js}`, false);
        await field(form, 'preview_seek', '2');
        await preview(page, form, info, `Image-in-window-${culture}-${js}`);
        await field(form, 'opacity_expression', ''); await field(form, 'preview_seek', '0');
      }
      if (kind === 'Motion') {
        await form.locator('[name="end_behavior"]').selectOption('Disappear');
        await field(form, 'preview_seek', '4');
        await preview(page, form, info, `Motion-expired-${culture}-${js}`, false);
        await form.locator('[name="end_behavior"]').selectOption('Loop'); await field(form, 'preview_seek', '0');
      }
      await form.locator('[data-save-graphics-element]').click();await expect(page).toHaveURL(/\/graphics-elements\/\d+\?saved=1$/);
      const id=Number(new URL(page.url()).pathname.split('/').at(-1));
      expect(sql(`SELECT "ZIndex"||'|'||"EpgEntries"||'|'||"LocationX"||'|'||"LocationY" FROM "GraphicsElement" WHERE "Id"=${id};`)).toBe('7|8|32.0|40.0');
      const saved=page.locator('[data-graphics-element-editor]').filter({visible:true});
      const download=page.waitForEvent('download');await saved.locator('[data-graphics-yaml-export]').click();
      const yaml=readFileSync((await (await download).path())!,'utf8');
      expect(yaml).toContain('location_x: 32.0');expect(yaml).toContain('z_index: 7');
      const original=sql(`SELECT "Config" FROM "GraphicsElement" WHERE "Id"=${id};`);
      await field(saved,'yaml',yaml.replace(/^name:.*$/m,`name: ${name} imported`));
      await field(saved, 'preview_variables', '{"Caption":"Épisode native subtitle"}');
      await saved.locator('[data-graphics-yaml-import]').click();await expect(page).toHaveURL(new RegExp(`/graphics-elements/${id}\\?saved=1$`));
      expect(sql(`SELECT "Config" FROM "GraphicsElement" WHERE "Id"=${id};`)).toBe(original);
      expect(sql(`SELECT "Name" FROM "GraphicsElement" WHERE "Id"=${id};`)).toBe(`${name} imported`);
      await page.goto('/graphics-elements/add'); if (js) await hydrated(page);
      const importer = page.locator('[data-graphics-element-editor]').filter({ visible: true });
      await openFields(importer, kind, js);
      await expect(importer.locator('[name="name"]')).toHaveValue('');
      await field(importer, 'yaml', yaml.replace(/^name:.*$/m, `name: ${name} via YAML`));
      await field(importer, 'preview_variables', '{"Caption":"Épisode native subtitle"}');
      await importer.locator('[data-graphics-yaml-import]').click(); await expect(page).toHaveURL(/\/graphics-elements\/\d+\?saved=1$/);
      const importedId = Number(new URL(page.url()).pathname.split('/').at(-1));
      expect(importedId).not.toBe(id);
      expect(sql(`SELECT "Config" FROM "GraphicsElement" WHERE "Id"=${importedId};`)).toBe(original);
      expect(sql(`SELECT "Name" FROM "GraphicsElement" WHERE "Id"=${importedId};`)).toBe(`${name} via YAML`);
      await page.goto(`/graphics-elements/${id}`); if (js) await hydrated(page);

      const current=page.locator('[data-graphics-element-editor]').filter({visible:true});
      await field(current,'yaml','name: Invalid\nunsupported_typo: true');
      await current.locator('[data-graphics-yaml-import]').click();await expect(page).toHaveURL(/error=graphics-authoring-invalid$/);
      await expect(page.locator('[data-graphics-element-error]').filter({visible:true})).toBeVisible();
      expect(sql(`SELECT "Config" FROM "GraphicsElement" WHERE "Id"=${id};`)).toBe(original);
      if (kind === 'Image') {
        if (js) await hydrated(page);
        const convert = page.locator('[data-graphics-element-editor]').filter({ visible: true });
        await openFields(convert, 'Text', js); await field(convert, 'text', 'Converted native text');
        await convert.locator('[data-save-graphics-element]').click();
        await expect(page).toHaveURL(new RegExp(`/graphics-elements/${id}\\?saved=1$`));
        expect(sql(`SELECT "Kind"||'|'||"ZIndex"||'|'||"LocationX" FROM "GraphicsElement" WHERE "Id"=${id};`)).toBe('1|7|32.0');
      }
    }
    for (const [kind, error] of [['Text', 'missing-font'], ['Image', 'missing-media'], ['Text', 'missing-variable']] as const) {
      await page.goto('/graphics-elements/add'); if (js) await hydrated(page);
      const form = page.locator('[data-graphics-element-editor]').filter({ visible: true });
      await field(form, 'name', `Invalid ${error}`); await openFields(form, kind, js);
      if (error === 'missing-font') {
        await form.locator('[name="renderer"]').selectOption('drawtext');
        await field(form, 'text', 'Real text'); await field(form, 'font_file', 'missing/font.ttf');
      } else if (error === 'missing-media') {
        await field(form, 'image', 'missing/image.png');
      } else {
        await field(form, 'text', '{{ UnknownAuthoringVariable }}');
      }
      const before = sql('SELECT COUNT(*) FROM "GraphicsElement";');
      await form.locator('[data-save-graphics-element]').click();
      await expect(page).toHaveURL(new RegExp(`error=graphics-authoring-${error}$`));
      await expect(page.locator('[data-graphics-element-error]').filter({ visible: true })).toBeVisible();
      expect(sql('SELECT COUNT(*) FROM "GraphicsElement";')).toBe(before);
    }
    expect(errors).toEqual([]);await context.close();
  });
}

for (const js of [true, false]) test(`finding 02 every culture exposes native kind and capability controls (${js ? 'hydrated' : 'no JS'})`, async ({ browser }, info) => {
  test.skip(!database, 'Requires a disposable database'); test.setTimeout(90000);
  const context = await browser.newContext({ javaScriptEnabled: js, baseURL: process.env.E2E_BASE_URL });
  const page = await context.newPage();
  try {
    for (const culture of ['de-de', 'en-us', 'es-419', 'es-es', 'fr-fr', 'it-it', 'nl', 'pl', 'pt-br']) {
      await language(page, culture); await page.goto('/graphics-elements/add'); if (js) await hydrated(page);
      const form = page.locator('[data-graphics-element-editor]').filter({ visible: true });
      await expect(form.locator('[name="kind"] option')).toHaveCount(5);
      await expect(form).toContainText('Drawtext'); await expect(form).toContainText('Drawvg');
      await expect(form).toContainText('X/Y');
      await expect(form.locator('[data-graphics-yaml-import]')).toBeVisible();
      if (culture === 'nl') {
        await page.setViewportSize({ width: 390, height: 844 });
        const toggle = page.locator('#nav-toggle'); if (await toggle.getAttribute('aria-expanded') === 'true') await toggle.click();
        await form.locator('[name="name"]').scrollIntoViewIfNeeded();
        expect((await form.boundingBox())!.width).toBeLessThanOrEqual(390);
        expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(390);
        await page.screenshot({ path: info.outputPath(`authoring-mobile-${js}.png`) });
      }
    }
  } finally { await context.close(); }
});
