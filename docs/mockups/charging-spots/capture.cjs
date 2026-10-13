// Render the static design and exercise its principal click paths.
const { chromium } = require(process.env.CUTOUT_MOCKUP_PLAYWRIGHT || 'playwright');
const assert = require('node:assert/strict');
const path = require('node:path');
const { pathToFileURL } = require('node:url');

(async () => {
  const browser = await chromium.launch({ channel: 'chrome', headless: true });
  try {
    const page = await browser.newPage({ viewport: { width: 1300, height: 2100 } });
    const failures = [];
    const externalRequests = [];
    page.on('pageerror', error => failures.push(error.message));
    page.on('request', request => {
      if (/^https?:/.test(request.url())) externalRequests.push(request.url());
    });
    const fileUrl = pathToFileURL(path.join(__dirname, 'index.html')).href;
    for (const appearance of ['light', 'dark']) {
      const output = name => path.join(__dirname, `${appearance === 'dark' ? 'dark-' : ''}${name}`);
    const source = `${fileUrl}?theme=${appearance}`;
    await page.setViewportSize({ width: 1300, height: 2100 });
    await page.goto(source);
    assert.equal(await page.locator('#gallery .phone').count(), 6);
    assert.equal(await page.locator('[data-theme]').getAttribute('aria-pressed'), String(appearance === 'dark'));
    await page.locator('[data-theme]').click();
    assert.equal(await page.locator('[data-theme]').getAttribute('aria-pressed'), String(appearance !== 'dark'));
    await page.locator('[data-theme]').click();
    await page.screenshot({ path: output('contact-sheet.png'), fullPage: true });

    for (const screen of ['browse', 'coffee', 'list', 'place', 'edit', 'ev']) {
      await page.goto(`${source}&screen=${screen}`);
      await page.locator('[data-phone="demo"]').screenshot({ path: output(`${screen}.png`) });
    }

    await page.goto(`${source}&screen=browse`);
    const phone = page.locator('[data-phone="demo"]');
    await phone.locator('[data-action="filters"]').click();
    await phone.locator('[data-action="filter"][data-value="Free"]').click();
    await phone.locator('[data-action="apply"]').click();
    assert.equal(await phone.locator('.marker').count(), 1);
    assert.match(await phone.locator('.marker').innerText(), /Free/);
    await phone.locator('.marker').click();
    assert.equal(await phone.locator('.detail').count(), 0);
    const up = phone.locator('[data-action="vote"][data-value="up"]');
    const down = phone.locator('[data-action="vote"][data-value="down"]');
    await up.click();
    assert.equal(await up.getAttribute('aria-pressed'), 'true');
    assert.equal(await up.innerText(), '15');
    await down.click();
    assert.equal(await up.getAttribute('aria-pressed'), 'false');
    assert.equal(await up.innerText(), '14');
    assert.equal(await down.innerText(), '2');
    await down.click();
    assert.equal(await down.getAttribute('aria-pressed'), 'false');
    assert.equal(await down.innerText(), '1');
    await up.click();
    await phone.screenshot({ path: output('pin-voting.png') });
    await phone.locator('.pinvotes [data-action="select"]').click();
    assert.match(await phone.locator('.detail').innerText(), /No purchase required/);

    // The core form saves with no name/category/power fields supplied.
    await page.goto(`${source}&screen=edit`);
    assert.equal(await phone.locator('[data-field="share"]').count(), 0);
    assert.equal(await phone.locator('[data-field="name"]').count(), 0);
    await phone.locator('[data-action="cost"][data-value="Free"]').click();
    await phone.screenshot({ path: output('edit-free.png') });
    await phone.locator('[data-action="finish"]').click();
    assert.match(await phone.locator('.toast').innerText(), /Waiting to share/);
    const minimum = await page.evaluate(() => state.demo.submission);
    assert.deepEqual(Object.keys(minimum).sort(), ['accessNote', 'cost', 'pin']);

    await page.goto(`${source}&screen=edit`);
    await phone.locator('[data-action="details"]').click();
    assert.equal(await phone.locator('[data-field="name"]').inputValue(), '');
    assert.match(await phone.locator('.plugchoices .selected').innerText(), /NEMA 5-15/);
    await phone.locator('[data-field="region"]').selectOption('United Kingdom');
    assert.match(await phone.locator('.plugchoices .selected').innerText(), /Type G/);
    await phone.screenshot({ path: output('edit-uk.png') });
    await phone.locator('[data-action="plug"][data-value="Other"]').click();
    await phone.locator('[data-field="region"]').selectOption('Germany');
    assert.match(await phone.locator('.plugchoices .selected').allInnerTexts().then(v => v.join(' ')), /Other/);
    await phone.locator('[data-field="kind"]').selectOption('EV station');
    assert.equal(await phone.locator('[data-field="voltage"]').inputValue(), 'Unknown');
    assert.match(await phone.locator('.plugchoices .selected').innerText(), /Unknown/);
    await phone.screenshot({ path: output('edit-ev.png') });

    await page.goto(`${source}&screen=edit`);
    await phone.locator('[data-action="details"]').click();
    await phone.locator('[data-action="supply"]').click();
    await phone.locator('.modalbody').evaluate(el => { el.scrollTop = el.scrollHeight; });
    assert.match(await phone.innerText(), /Second power supply/);
    await phone.screenshot({ path: output('edit-more.png') });

    await page.goto(`${source}&screen=edit`);
    await phone.locator('[data-field="notes"]').fill('   ');
    await phone.locator('[data-field="notes"]').dispatchEvent('change');
    await phone.locator('[data-action="finish"]').click();
    assert.match(await phone.locator('.toast').innerText(), /Add an access note/);
    await phone.locator('[data-action="dismiss"]').click();
    await phone.locator('[data-field="notes"]').fill('Outlet beside the shelter.');
    await phone.locator('[data-field="notes"]').dispatchEvent('change');
    await page.evaluate(() => { state.demo.cost = ''; draw('demo'); });
    await phone.locator('[data-action="finish"]').click();
    assert.match(await phone.locator('.toast').innerText(), /Choose Free or Paid/);
    await page.evaluate(() => { state.demo.cost = 'Free'; state.demo.pinConfirmed = false; draw('demo'); });
    await phone.locator('[data-action="finish"]').click();
    assert.match(await phone.locator('.toast').innerText(), /Choose a map pin/);

    await page.goto(`${source}&screen=coffee&large=1`);
    const sheet = phone.locator('.detail');
    const sheetSize = await sheet.boundingBox();
    const frameSize = await phone.boundingBox();
    assert.ok(sheetSize.y >= frameSize.y && sheetSize.y + sheetSize.height <= frameSize.y + frameSize.height);
    await phone.screenshot({ path: output('detail-larger-text.png') });

    await page.setViewportSize({ width: 375, height: 900 });
    await page.goto(source);
    const overflow = await page.evaluate(() => document.documentElement.scrollWidth > innerWidth);
    assert.equal(overflow, false, 'The mobile screen set must not overflow horizontally');
    assert.deepEqual(failures, []);
    assert.deepEqual(externalRequests, []);
    console.log(appearance + ': Minimum shared save, required-field rejection, country plug defaults, manual override, EV unknowns, direct-pin check/X voting, larger text, mobile width and local-only loading passed.');
    }
  } finally {
    await browser.close();
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
