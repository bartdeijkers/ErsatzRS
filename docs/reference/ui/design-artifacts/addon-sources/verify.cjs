// Synthetic design checks only. Run from the repository root with the static
// server described in README.md; this never visits the live application.
const { chromium, firefox } = require('playwright');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { createHash } = require('node:crypto');

const base = 'http://127.0.0.1:8417/';
const output = path.resolve('test-results/oad-design');
const artifact = __dirname;
const documents = ['index.html', 'files.html', 'states.html', 'light.html'];
const report = { scope: 'Static OAD design only; no production acceptance', checks: [] };
report.revision = 'v0.1';
report.artifactSha256 = Object.fromEntries([...documents, 'styles.css'].map(file => [
  file, createHash('sha256').update(fs.readFileSync(path.join(artifact,file))).digest('hex'),
]));

async function inspect(page) {
  return page.evaluate(() => {
    const contrastFailures = [];
    const rgba = value => value.match(/[\d.]+/g).map(Number);
    const luminance = rgb => rgb.slice(0, 3).map(c => {
      c /= 255;
      return c <= 0.04045 ? c / 12.92 : ((c + .055) / 1.055) ** 2.4;
    }).reduce((sum, c, i) => sum + c * [.2126, .7152, .0722][i], 0);
    const visible = el => !!(el.getBoundingClientRect().width && el.getBoundingClientRect().height);
    for (const el of document.querySelectorAll('p,label,h1,h2,h3,dt,dd,a,button,span,strong,small,summary')) {
      if (!visible(el) || el.matches(':disabled') || ![...el.childNodes].some(n => n.nodeType === 3 && n.textContent.trim())) continue;
      const style = getComputedStyle(el);
      const foreground = rgba(style.color);
      let parent = el;
      let background;
      while (parent) {
        const color = rgba(getComputedStyle(parent).backgroundColor);
        if (color[3] === undefined || color[3] === 1) { background = color; break; }
        parent = parent.parentElement;
      }
      if (!background) throw new Error('No opaque background found');
      const a = luminance(foreground), b = luminance(background);
      const ratio = (Math.max(a,b)+.05)/(Math.min(a,b)+.05);
      const large = parseFloat(style.fontSize) >= 24 || (parseFloat(style.fontSize) >= 18.66 && parseFloat(style.fontWeight) >= 700);
      if (ratio < (large ? 3 : 4.5)) contrastFailures.push({ text: el.textContent.trim().slice(0,55), ratio });
    }
    const ids = [...document.querySelectorAll('[id]')].map(el => el.id);
    return {
      width: innerWidth,
      scrollWidth: document.documentElement.scrollWidth,
      mainCount: document.querySelectorAll('main').length,
      h1Count: document.querySelectorAll('h1').length,
      duplicateIds: ids.filter((id, i) => ids.indexOf(id) !== i),
      scripts: document.scripts.length,
      contrastFailures,
    };
  });
}

(async () => {
  fs.mkdirSync(output, { recursive: true });
  // All scenario links must resolve to local files and real fragment targets.
  for (const file of documents) {
    const source = fs.readFileSync(path.join(artifact,file),'utf8');
    for (const [,href] of source.matchAll(/href="([^"]+)"/g)) {
      const [relative,fragment] = href.split('#');
      const destination = path.resolve(artifact, relative || file);
      assert(fs.existsSync(destination), `${file}: missing ${href}`);
      if (fragment) assert(fs.readFileSync(destination,'utf8').includes(`id="${fragment}"`), `${file}: missing fragment ${href}`);
    }
  }
  for (const [name,type] of [['chromium',chromium],['firefox',firefox]]) {
    const browser = await type.launch({ headless: true });
    try {
      const context = await browser.newContext({ reducedMotion: 'reduce' });
      const page = await context.newPage();
      const errors = [];
      page.on('pageerror', error => errors.push(error.message));
      for (const file of documents) {
        for (const theme of ['dark','light']) {
          for (const width of [320,390,768,1440]) {
            await page.setViewportSize({ width, height: 1000 });
            await page.goto(base+file);
            // Theme inspection is test instrumentation, not prototype scripting.
            await page.evaluate(theme => document.body.dataset.theme = theme,theme);
            const data = await inspect(page);
            assert(data.scrollWidth <= width, `${name}/${file}/${theme}/${width}: overflow ${data.scrollWidth}`);
            assert.equal(data.mainCount,1);
            assert.equal(data.h1Count,1);
            assert.deepEqual(data.duplicateIds,[]);
            assert.equal(data.scripts,0);
            assert.deepEqual(data.contrastFailures,[],JSON.stringify(data.contrastFailures));
            report.checks.push({ browser:name,file,theme,width,result:'pass' });
          }
        }
      }
      await page.goto(base+'index.html');
      await page.setViewportSize({ width:1440,height:1100 });
      const toggle = page.getByRole('switch',{ name:'Automatically update this repository' });
      assert(await toggle.isChecked());
      await toggle.focus();
      await page.keyboard.press('Space');
      assert.equal(await toggle.isChecked(),false);
      assert.notEqual(await toggle.evaluate(el => getComputedStyle(el).outlineStyle),'none');
      // Firefox may restore native form controls on reload. A new document URL
      // checks the authored default without mistaking browser history for app state.
      await page.goto(base+'index.html?review=fresh');
      assert(await toggle.isChecked(), 'A fresh preview document must start with the authored default');
      const summary = page.locator('summary');
      await summary.focus();
      await page.keyboard.press('Enter');
      assert(await page.getByText('Checkout folder on the ErsatzRS server',{exact:true}).isVisible());
      await page.keyboard.press('Enter');
      assert.equal(await page.getByText('Checkout folder on the ErsatzRS server',{exact:true}).isVisible(),false);
      await page.locator('h1').click();
      await page.screenshot({ path:path.join(output,`${name}-desktop-dark.png`),fullPage:true });
      // 200% text enlargement, plus 720 CSS px = 1440px desktop at 200% zoom.
      await page.setViewportSize({ width:720,height:550 });
      await page.evaluate(() => document.documentElement.style.fontSize = '32px');
      const enlarged = await inspect(page);
      assert(enlarged.scrollWidth <= 720, `${name}: enlarged text overflow`);
      await page.goto(base+'files.html');
      assert.equal(await page.getByLabel('ZIP package',{exact:true}).count(),1);
      assert.equal(await page.getByLabel('ZIP package',{exact:true}).isDisabled(),false);
      await page.setViewportSize({ width:390,height:844 });
      await page.screenshot({ path:path.join(output,`${name}-mobile-files.png`),fullPage:true });
      await page.goto(base+'light.html');
      await page.setViewportSize({ width:1440,height:1100 });
      await page.screenshot({ path:path.join(output,`${name}-desktop-light.png`),fullPage:true });
      await page.goto(base+'states.html');
      await page.screenshot({ path:path.join(output,`${name}-states.png`),fullPage:true });
      assert.deepEqual(errors,[]);
      report.checks.push({ browser:name,keyboard:'switch and disclosure passed',enlargedText:'passed',fileLabel:'passed' });
      await context.close();
    } finally { await browser.close(); }
  }
  fs.writeFileSync(path.join(output,'report.json'),JSON.stringify(report,null,2)+'\n');
  console.log(`OAD prototype: ${report.checks.length} checks passed; reports in test-results/oad-design/`);
})().catch(error => { console.error(error); process.exitCode = 1; });
