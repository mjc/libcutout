// Browser-only design checks; no native/provider behavior is proved here.
const { chromium } = require(process.env.CUTOUT_MOCKUP_PLAYWRIGHT || 'playwright');
const assert = require('node:assert/strict');
const path = require('node:path');
const { pathToFileURL } = require('node:url');
const screens = ['community','standings','ride','review','status','connections','trail','challenge'];
(async () => {
  const browser = await chromium.launch({channel:'chrome',headless:true});
  const failures=[], external=[];
  try {
    const page=await browser.newPage({viewport:{width:1680,height:1900}});
    page.on('pageerror',e=>failures.push(e.message));
    page.on('request',r=>{if(/^https?:/.test(r.url()))external.push(r.url())});
    const url=pathToFileURL(path.join(__dirname,'index.html')).href;
    for(const theme of ['light','dark']) {
      const prefix=theme==='dark'?'dark-':'';
      await page.goto(`${url}?theme=${theme}&gallery=1`);
      assert.equal(await page.locator('.phone').count(),8);
      await page.screenshot({path:path.join(__dirname,`${prefix}contact-sheet.png`),fullPage:true});
      await page.screenshot({path:path.join(__dirname,`${prefix}contact-sheet.jpg`),fullPage:true,type:'jpeg',quality:86});
      for(const screen of screens){
        await page.goto(`${url}?theme=${theme}&screen=${screen}`);
        await page.locator('#demo .phone').screenshot({path:path.join(__dirname,`${prefix}${screen}.png`)});
        await page.locator('#demo .phone').screenshot({path:path.join(__dirname,`${prefix}${screen}.jpg`),type:'jpeg',quality:86});
      }
      await page.goto(`${url}?theme=${theme}`);
      await page.locator('#demo [data-provider="Hermes"]').click();
      assert.match(await page.locator('#demo .metric').innerText(),/02:41/);
      await page.locator('#demo [data-provider="EUC Stats"]').click();
      assert.match(await page.locator('#demo .metric').innerText(),/221.6/);
      await page.locator('#demo [data-provider="FloatRank"]').click();
      await page.locator('#demo [data-action="standings"]').click();
      assert.match(await page.locator('#demo .me').innerText(),/184.2/);
      await page.locator('#demo [data-field="period"]').selectOption('This week');
      assert.match(await page.locator('#demo .me').innerText(),/47.3/);
      assert.match(await page.locator('#demo .content').innerText(),/Oct 12–18/);
      await page.locator('#demo [data-field="period"]').selectOption('This month');
      for(const [scenario,message] of [['loading','Loading standings'],['empty','No eligible results'],['error','Provider unavailable'],['offline','Offline · cached snapshot']]){
        await page.selectOption('#scenario',scenario);
        assert.match(await page.locator('#demo .content').innerText(),new RegExp(message));
        await page.locator('#demo .phone').screenshot({path:path.join(__dirname,`${prefix}standings-${scenario}.png`)});
      }
      await page.selectOption('#scenario','normal');
      await page.locator('.fixture-controls [data-action="limited"]').click();
      assert.equal(await page.locator('#demo .rankrow').count(),0);
      assert.match(await page.locator('#demo .content').innerText(),/Standings are available on its website/);
      await page.locator('#demo .phone').screenshot({path:path.join(__dirname,`${prefix}standings-limited.png`)});
      await page.locator('.fixture-controls [data-action="limited"]').click();
      for(const provider of ['Hermes','EUC Stats']){
        await page.locator(`#demo [data-provider="${provider}"]`).click();
        await page.locator('#demo .phone').screenshot({path:path.join(__dirname,`${prefix}${provider==='Hermes'?'hermes':'eucstats'}-standings.png`)});
        await page.locator('#demo .phone').screenshot({path:path.join(__dirname,`${prefix}${provider==='Hermes'?'hermes':'eucstats'}-standings.jpg`),type:'jpeg',quality:86});
      }
      await page.selectOption('#screen','ride');
      await page.locator('#demo [data-destination="Hermes"]').uncheck();
      assert.equal(await page.locator('#demo [data-destination="FloatRank"]').isChecked(),false);
      await page.locator('#demo [data-destination="FloatRank"]').check();
      assert.equal(await page.locator('#demo [data-destination="Hermes"]').isChecked(),true);
      await page.locator('#demo [data-action="review"]').click();
      await page.locator('#demo [data-action="send"]').click();
      assert.match(await page.locator('#validation').innerText(),/Confirm/);
      await page.locator('#consent').check();
      await page.locator('#demo [data-action="send"]').click();
      assert.match(await page.locator('#demo .statusbox').innerText(),/Waiting for connection/);
      await page.locator('.fixture-controls [data-action="received"]').click();
      assert.match(await page.locator('#demo .statusbox').innerText(),/Ride received/);
      assert.doesNotMatch(await page.locator('#demo .statusbox').innerText(),/Ride counted/);
      await page.locator('.fixture-controls [data-action="unknown"]').click();
      assert.match(await page.locator('#demo .statusbox').innerText(),/Check for an existing receipt/);
      await page.locator('.fixture-controls [data-action="received"]').click();
      await page.locator('.fixture-controls [data-action="held"]').click();
      assert.match(await page.locator('#demo .statusbox').innerText(),/No rank is confirmed/);
      await page.locator('.fixture-controls [data-action="counted"]').click();
      assert.match(await page.locator('#demo .statusbox').innerText(),/Ride counted/);
      await page.locator('.fixture-controls [data-action="expired"]').click();
      await page.locator('#demo [data-action="reconnect"]').click();
      assert.match(await page.locator('#demo .statusbox').innerText(),/Waiting for connection/);
      await page.selectOption('#screen','connections');
      await page.locator('#demo [data-action="unlink"]').click();
      assert.match(await page.locator('#demo .content').innerText(),/Unlinked/);
      await page.selectOption('#screen','status');
      assert.match(await page.locator('#demo .statusbox').innerText(),/Queued send cancelled/);
      await page.selectOption('#screen','challenge');
      assert.equal(await page.locator('#demo [data-action="later"]').isDisabled(),true);
      await page.setViewportSize({width:375,height:950});
      await page.selectOption('#screen','review');
      assert.ok(await page.evaluate(()=>document.documentElement.scrollWidth<=375));
      await page.addStyleTag({content:'.phone{font-size:19px}.content{font-size:19px}.small,.intro,.check span,.check small{font-size:18px}'});
      await page.locator('#demo .phone').screenshot({path:path.join(__dirname,`${prefix}review-larger-text.png`)});
      await page.setViewportSize({width:1680,height:1900});
    }
    assert.deepEqual(failures,[]);assert.deepEqual(external,[]);
    console.log(JSON.stringify({screens:8,appearances:2,externalRequests:external.length,pageErrors:failures.length,checks:['provider/metric coherence','period/results coherence','loading/empty/error/offline states','limited reads','upstream destination dependency','explicit confirmation','queued vs receipt vs counted','unknown reconciliation','review verdict','authorization recovery','unlink cancellation','later contest disabled','375px layout','larger text rendering']},null,2));
  } finally {await browser.close()}
})().catch(e=>{console.error(e);process.exitCode=1});
