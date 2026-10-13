const { chromium } = require(process.env.CUTOUT_MOCKUP_PLAYWRIGHT || 'playwright');
const assert = require('node:assert/strict');
const path = require('node:path');
const { pathToFileURL } = require('node:url');

(async () => {
  const browser = await chromium.launch({ channel: 'chrome', headless: true });
  const failures = [];
  try {
    const page = await browser.newPage();
    const phone = page.locator('[data-phone="demo"]');
    const source = pathToFileURL(path.join(__dirname, 'index.html')).href;
    const checks = [
      ['Any plug clears its filter', 'browse', async () => {
        await phone.locator('[data-action="filters"]').first().click();
        await phone.locator('[data-field="query"]').selectOption('J1772');
        await phone.locator('[data-field="query"]').selectOption({ label: 'Any plug' });
        await phone.locator('[data-action="apply"]').click();
        assert.equal(await phone.locator('.marker').count(), 3);
      }],
      ['Choices expose their selected state', 'edit', async () => {
        assert.equal(await phone.locator('[data-action="cost"][data-value="Paid"]').getAttribute('aria-pressed'), 'true');
        assert.equal(await phone.locator('[data-action="cost"][data-value="Free"]').getAttribute('aria-pressed'), 'false');
        await phone.locator('[data-action="cost"][data-value="Free"]').click();
        assert.equal(await phone.locator('[data-action="cost"][data-value="Free"]').getAttribute('aria-pressed'), 'true');
        await phone.locator('[data-action="details"]').click();
        assert.equal(await phone.locator('[data-action="plug"][data-value="NEMA 5-15"]').getAttribute('aria-pressed'), 'true');
        await phone.locator('[data-action="plug"][data-value="Other"]').click();
        assert.equal(await phone.locator('[data-action="plug"][data-value="Other"]').getAttribute('aria-pressed'), 'true');
      }],
      ['Keyboard focus follows filters and returns to the opener', 'browse', async () => {
        const opener = phone.locator('.toolbar [data-action="filters"]');
        await opener.focus();
        await opener.press('Enter');
        assert.equal(await phone.locator('.filterpanel').evaluate(el => el.contains(document.activeElement)), true);
        const choice = phone.locator('.filterpanel [data-action="filter"][data-value="Free"]');
        await choice.focus();
        await choice.press('Enter');
        assert.equal(await choice.evaluate(el => document.activeElement === el), true);
        await phone.locator('[data-action="apply"]').focus();
        await phone.locator('[data-action="apply"]').press('Enter');
        assert.equal(await opener.evaluate(el => document.activeElement === el), true);
      }],
      ['Voting retains keyboard focus', 'browse', async () => {
        const vote = phone.locator('[data-action="vote"][data-value="up"]');
        await vote.focus();
        await vote.press('Enter');
        assert.equal(await vote.evaluate(el => document.activeElement === el), true);
        assert.equal(await vote.getAttribute('aria-pressed'), 'true');
      }],
      ['Form choices retain keyboard focus', 'edit', async () => {
        const choice = phone.locator('[data-action="cost"][data-value="Free"]');
        await choice.focus();
        await choice.press('Enter');
        assert.equal(await choice.evaluate(el => document.activeElement === el), true);
      }],
      ['Search updates while typing and keeps the input', 'list', async () => {
        const search = phone.locator('[data-field="query"]');
        await search.evaluate(el => { window.originalSearchInput = el; });
        await search.fill('J1772');
        assert.equal(await phone.locator('.listrow').count(), 1);
        assert.equal(await search.evaluate(el => el === window.originalSearchInput && document.activeElement === el), true);
        await search.press('Enter');
        assert.equal(await phone.locator('.listrow').count(), 1);
        assert.equal(await search.evaluate(el => document.activeElement === el), true);
      }],
      ['Typing an access note then clicking Save works once', 'edit', async () => {
        await phone.locator('[data-field="notes"]').fill('Ask staff for the outlet.');
        await phone.locator('[data-action="finish"]').click();
        const submission = await page.evaluate(() => state.demo.submission);
        assert.equal(submission?.accessNote, 'Ask staff for the outlet.');
      }],
      ['Plug edits do not confirm voltage', 'edit', async () => {
        await phone.locator('[data-action="details"]').click();
        await phone.locator('[data-action="plug"][data-value="Other"]').click();
        await phone.locator('[data-action="finish"]').click();
        const submission = await page.evaluate(() => state.demo.submission);
        assert.equal(Object.hasOwn(submission, 'voltage'), false);
        assert.equal(Object.hasOwn(submission, 'plugs'), false);
      }],
      ['Voltage edits need explicit inclusion', 'edit', async () => {
        await phone.locator('[data-action="details"]').click();
        await phone.locator('[data-field="voltage"]').fill('240');
        await phone.locator('[data-field="voltage"]').dispatchEvent('change');
        await phone.locator('[data-action="finish"]').click();
        const submission = await page.evaluate(() => state.demo.submission);
        assert.equal(Object.hasOwn(submission, 'voltage'), false);
        assert.equal(Object.hasOwn(submission, 'plugs'), false);
      }],
      ['Editing confirmed voltage updates its accessible confirmation', 'edit', async () => {
        await phone.locator('[data-action="details"]').click();
        await phone.locator('[data-action="confirm-power"]').click();
        await phone.locator('[data-field="voltage"]').fill('240');
        const confirmation = phone.locator('[data-action="confirm-power"]');
        assert.equal(await confirmation.getAttribute('aria-label'), 'Include voltage and plugs');
        assert.equal(await confirmation.textContent(), 'Include voltage and plugs');
        await phone.locator('[data-action="finish"]').click();
        const submission = await page.evaluate(() => state.demo.submission);
        assert.equal(Object.hasOwn(submission, 'voltage'), false);
      }],
      ['Changing place type requires confirming replacement defaults', 'edit', async () => {
        await phone.locator('[data-action="details"]').click();
        await phone.locator('[data-field="kind"]').selectOption('EV station');
        await phone.locator('[data-action="confirm-power"]').click();
        await phone.locator('[data-field="kind"]').selectOption('Outlet');
        await phone.locator('[data-action="finish"]').click();
        const submission = await page.evaluate(() => state.demo.submission);
        assert.equal(Object.hasOwn(submission, 'voltage'), false);
        assert.equal(Object.hasOwn(submission, 'plugs'), false);
      }],
      ['Explicit inclusion retains the selected supply', 'edit', async () => {
        await phone.locator('[data-action="details"]').click();
        await phone.locator('[data-field="voltage"]').fill('240');
        await phone.locator('[data-field="voltage"]').dispatchEvent('change');
        await phone.locator('[data-action="plug"][data-value="Other"]').click();
        await phone.locator('[data-action="confirm-power"]').click();
        await phone.locator('[data-action="finish"]').click();
        const submission = await page.evaluate(() => state.demo.submission);
        assert.equal(submission.voltage, '240');
        assert.ok(submission.plugs.includes('Other'));
      }],
    ];
    for (const theme of ['light', 'dark']) {
      for (const [name, screen, check] of checks) {
        await page.goto(`${source}?theme=${theme}&screen=${screen}`);
        try {
          await check();
          console.log(`${theme}: ${name} passed`);
        } catch (error) {
          failures.push(`${theme}: ${name}: ${error.message}`);
        }
      }
    }
    assert.deepEqual(failures, []);
  } finally {
    await browser.close();
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
