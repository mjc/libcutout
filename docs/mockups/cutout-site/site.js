const assetVersion = new URL(import.meta.url).search;
const dictionaries = await Promise.all(['en', 'es'].map(async language => {
  const response = await fetch(`locales/${language}.json${assetVersion}`);
  if (!response.ok) throw new Error(`Unable to load ${language} copy`);
  return [language, await response.json()];
}));
const copy = Object.fromEntries(dictionaries);
const pages = new Set(['home', 'guide', 'support', 'about']);
let selectedFeature = 0;
let state = readLocation();
const app = document.querySelector('#app');
const escape = value => String(value).replace(/[&<>"']/g, character => ({ '&':'&amp;', '<':'&lt;', '>':'&gt;', '"':'&quot;', "'":'&#39;' }[character]));
const icons = {
  arrow:'M5 12h14m-6-6 6 6-6 6',
  ride:'M4 16a8 8 0 1 1 16 0M12 13l5-5M4 16h16M7 20h10',
  map:'m3 5 6-2 6 2 6-2v16l-6 2-6-2-6 2V5Zm6-2v16m6-14v16',
  plug:'M8 3v5m8-5v5M6 8h12v3a6 6 0 0 1-6 6v4m0-4a6 6 0 0 1-6-6',
  lock:'M6 10h12v11H6V10Zm3 0V6a3 3 0 0 1 6 0v4m-3 5v2',
  music:'M8 18V5l12-2v13M8 8l12-2M8 18c0 2-5 3-5 0s5-3 5 0Zm12-2c0 2-5 3-5 0s5-3 5 0Z',
  check:'m5 12 4 4L19 6',
  globe:'M21 12a9 9 0 1 1-18 0 9 9 0 0 1 18 0ZM3 12h18M12 3c-5 5-5 13 0 18 5-5 5-13 0-18Z',
  menu:'M4 6h16M4 12h16M4 18h16',
  plus:'M5 12h14M12 5v14',
  search:'M15 15l6 6M17 10a7 7 0 1 1-14 0 7 7 0 0 1 14 0Z',
  help:'M9 9a3 3 0 1 1 5 2c-2 1-2 2-2 3m0 3v.1M21 12a9 9 0 1 1-18 0 9 9 0 0 1 18 0Z',
  image:'M3 4h18v16H3V4Zm0 11 6-5 5 4 3-2 4 4M15 8h.01',
};
const icon = name => `<svg class="icon" viewBox="0 0 24 24" aria-hidden="true"><path d="${icons[name] || icons.image}"/></svg>`;
const href = page => `?page=${page}&lang=${state.language}`;
const link = (page, label, className = '') => `<a href="${href(page)}" data-page="${page}" class="${className}"${state.page === page ? ' aria-current="page"' : ''}>${escape(label)}</a>`;
function readLocation() {
  const query = new URLSearchParams(location.search);
  return { page: pages.has(query.get('page')) ? query.get('page') : 'home', language: copy[query.get('lang')] ? query.get('lang') : 'en' };
}
function slot(name, className = '') {
  const t = copy[state.language];
  return `<div class="screen-slot ${className}" role="img" aria-label="${escape(`${name}. ${t.preview.label}. ${t.preview.description}`)}"><span class="slot-symbol">${icon('image')}</span><span class="slot-name">${escape(name)}</span><span class="slot-description">${escape(t.preview.description)}</span><span class="slot-label">${escape(t.preview.label)}</span></div>`;
}
function header(t) {
  return `<a class="skip" href="#main">${escape(t.skip)}</a><header class="site-header"><div class="wrap header-inner"><a class="brand" data-page="home" href="${href('home')}"><img src="assets/app-icon.png" alt="" width="40" height="40">CutOut</a><nav class="nav" id="site-navigation" aria-label="${escape(t.navigation)}">${link('home',t.nav.app)}${link('guide',t.nav.guide)}${link('support',t.nav.support)}</nav><div class="header-actions"><label class="language">${icon('globe')}<select id="language" aria-label="${escape(t.language)}"><option value="en"${state.language === 'en' ? ' selected' : ''}>English</option><option value="es"${state.language === 'es' ? ' selected' : ''}>Español</option></select></label><button class="button" data-download>${escape(t.get)}</button><button class="menu-toggle" aria-label="${escape(t.menu)}" aria-controls="site-navigation" aria-expanded="false">${icon('menu')}</button></div></div></header>`;
}
function footer(t) {
  return `<footer class="site-footer"><div class="wrap"><div class="footer-top"><div><a class="brand" href="${href('home')}" data-page="home">CutOut<span aria-hidden="true" style="color:var(--yellow)">↗</span></a><p>${escape(t.footer.description)}</p></div><div class="footer-links">${link('about',t.nav.about)}${link('guide',t.nav.guide)}${link('support',t.nav.support)}<a href="https://cutout.lol/privacy-policy/">${escape(t.footer.privacy)}</a><a href="https://github.com/mjc/libcutout">GitHub ${icon('arrow')}</a></div></div><div class="footer-bottom"><span>© 2026 CutOut</span><span>${escape(t.footer.built)}</span></div></div></footer><div class="prototype-note">${escape(t.preview.note)}</div><dialog id="download-dialog" aria-labelledby="download-title"><span class="eyebrow">${escape(t.preview.dialogTag)}</span><h2 id="download-title">${escape(t.preview.dialogTitle)}</h2><p>${escape(t.preview.dialogBody)}</p><form method="dialog"><button class="button">${escape(t.preview.close)}</button></form></dialog>`;
}
function routeArt() {
  return `<svg class="route-art" viewBox="0 0 780 670" aria-hidden="true"><defs><pattern id="grid" width="68" height="68" patternTransform="rotate(-22)" patternUnits="userSpaceOnUse"><path d="M68 0H0v68" fill="none" stroke="#2b3427" stroke-width="1"/></pattern></defs><rect width="780" height="670" fill="url(#grid)"/><path d="M160 85c150-80 170 125 320 20s260 65 185 155-20 130-140 170S180 650 75 490 45 145 160 85Z" fill="none" stroke="#303d28" stroke-width="1"/><path d="M205 140c140-65 100 140 245 25s195 65 120 150 5 90-110 135S210 580 130 450 115 190 205 140Z" fill="none" stroke="#303d28" stroke-width="1"/><path d="M90 545c90-30 160 25 190-105s-5-120 100-180 100 30 170-30 10-100 130-120" stroke="#43513a" stroke-width="20" fill="none"/><path d="M90 545c90-30 160 25 190-105s-5-120 100-180 100 30 170-30 10-100 130-120" stroke="#ffcf00" stroke-width="3" stroke-dasharray="1 9" stroke-linecap="round" fill="none"/><circle cx="90" cy="545" r="7" fill="#ffcf00"/><circle cx="680" cy="110" r="7" fill="#ffcf00"/></svg>`;
}
function vehicleIcon(kind) {
  const shapes = {
    onewheel:'<circle cx="32" cy="28" r="12"/><path d="M8 27h12m24 0h12M7 31h13m24 0h13M28 20v16m8-16v16"/>',
    euc:'<rect x="23" y="7" width="18" height="34" rx="8"/><path d="M26 15h12M11 33h12m18 0h12M28 41v4h8v-4"/>',
    eskate:'<path d="M6 24q26-9 52 0M9 28h46"/><circle cx="16" cy="34" r="5"/><circle cx="48" cy="34" r="5"/>',
    scooter:'<circle cx="12" cy="36" r="7"/><circle cx="52" cy="36" r="7"/><path d="M13 30h25L47 9h-8m8 0 5 27M13 30l-3 6"/>',
    ebike:'<circle cx="12" cy="35" r="9"/><circle cx="52" cy="35" r="9"/><path d="m12 35 12-20 14 20H12m12-20 8 20 13-20M20 11h9m16-6 7 30m-7-30h7"/>',
    emoto:'<circle cx="12" cy="35" r="9"/><circle cx="52" cy="35" r="9"/><path d="m13 31 10-12h16l13 16M17 27h23l6-14-4-5h9M21 17h12m-6 11-4 8h12l4-8"/>'
  };
  return `<svg class="vehicle-icon" viewBox="0 0 64 52" aria-hidden="true">${shapes[kind]}</svg>`;
}
function vehicles(t) {
  return `<section class="section vehicles-section wrap" id="vehicles"><div class="section-heading"><div><p class="eyebrow">${escape(t.vehicles.tag)}</p><h2>${escape(t.vehicles.title)}</h2></div><p>${escape(t.vehicles.body)}</p></div><div class="vehicle-grid">${t.vehicles.items.map(vehicle=>`<article class="vehicle-card">${vehicleIcon(vehicle.kind)}<h3>${escape(vehicle.name)}</h3><p>${escape(vehicle.detail)}</p><span class="coverage-label ${vehicle.owned ? 'owned' : 'needed'}">${vehicle.owned ? icon('check') : icon('plus')}${escape(vehicle.owned ? t.vehicles.owned : t.vehicles.needed)}</span></article>`).join('')}</div>${link('support',t.vehicles.link,'text-link')}</section>`;
}
function home(t) {
  return `<section class="hero"><div class="wrap hero-grid"><div class="hero-copy"><p class="eyebrow">${escape(t.hero.eyebrow)}</p><h1>${escape(t.hero.title)}<br><span>${escape(t.hero.accent)}</span></h1><p class="lead">${escape(t.hero.body)}</p><div class="actions"><button class="button" data-download>${escape(t.get)}${icon('arrow')}</button>${link('guide',t.nav.guide,'button secondary')}</div><p class="hero-note">${escape(t.hero.note)}</p></div><div class="hero-art">${routeArt()}<div class="phone">${slot(t.features[0].title)}</div><div class="hero-stamp"><strong>${escape(t.hero.stampTitle)}</strong>${escape(t.hero.stampBody)}</div></div></div></section><div class="feature-strip"><div class="wrap strip-inner">${t.features.map((feature,index) => `<div class="strip-item">${icon(['ride','map','plug'][index])}${escape(feature.title)}</div>`).join('')}</div></div>${vehicles(t)}<section class="section wrap" id="features"><div class="section-heading"><div><p class="eyebrow">${escape(t.featuresTag)}</p><h2>${escape(t.featuresTitle)}</h2></div><p>${escape(t.featuresBody)}</p></div><div class="feature-stage"><div class="feature-tabs">${t.features.map((feature,index) => `<button class="feature-tab" data-feature="${index}" aria-pressed="${selectedFeature === index}" aria-controls="feature-preview">${icon(['ride','map','plug'][index])}<span><span class="feature-title">${escape(feature.title)}</span><span class="feature-body">${escape(feature.body)}</span></span><span class="arrow">${icon('arrow')}</span></button>`).join('')}</div><div><div class="feature-preview" id="feature-preview">${slot(t.features[selectedFeature].title)}</div><p class="preview-caption">${escape(t.preview.caption)}</p></div></div></section>${accessories(t)}<section class="section steps-section wrap"><div class="steps-grid">${t.steps.map((step,index) => `<article><span class="step-number">0${index+1}</span><h3>${escape(step.title)}</h3><p>${escape(step.body)}</p></article>`).join('')}</div>${link('guide',t.stepsLink,'text-link')}</section><section class="section soundtrack-section"><div class="wrap soundtrack-grid"><div><p class="eyebrow">${escape(t.soundtrack.tag)}</p><h2>${escape(t.soundtrack.title)}</h2><p class="lead">${escape(t.soundtrack.body)}</p></div>${slot(t.soundtrack.preview)}</div></section><section class="section wrap"><div class="section-heading"><div><p class="eyebrow">${escape(t.openSource.tag)}</p><h2>${escape(t.openSource.title)}</h2></div><p>${escape(t.openSource.body)}</p></div>${link('about',t.openSource.link,'text-link')}</section>${cta(t)}`;
}
function accessories(t) {
  return t.accessories.map(accessory => `<section class="section wrap"><div class="soundtrack-grid"><div><p class="eyebrow">${escape(accessory.tag)}</p><h2>${escape(accessory.title)}</h2><p class="lead">${escape(accessory.body)}</p>${link('support',accessory.link,'text-link')}</div>${slot(accessory.preview)}</div></section>`).join('');
}
function about(t) {
  return `<section class="page-intro"><div class="wrap"><p class="eyebrow">${escape(t.nav.about)}</p><h1>${escape(t.about.title)}</h1><p class="lead">${escape(t.about.body)}</p></div></section><section class="section wrap"><div class="steps-grid">${t.about.sections.map(section => `<article><h2 class="about-heading">${escape(section.title)}</h2><p>${escape(section.body)}</p></article>`).join('')}</div><div class="actions"><a class="button" href="https://github.com/mjc/libcutout">${escape(t.about.source)}${icon('arrow')}</a>${link('support',t.about.faq,'text-link')}</div></section>${cta(t)}`;
}
function cta(t) { return `<section class="cta-section"><div class="wrap cta-inner"><h2>${escape(t.cta)}</h2><button class="button" data-download>${escape(t.get)}${icon('arrow')}</button></div></section>`; }
function guide(t) {
  return `<section class="page-intro"><div class="wrap"><p class="eyebrow">${escape(t.nav.guide)}</p><h1>${escape(t.guide.title)}</h1><p class="lead">${escape(t.guide.body)}</p></div></section><div class="wrap guide-layout"><nav class="guide-nav" aria-label="${escape(t.guide.contents)}">${t.guide.sections.map((section,index) => `<a href="#step-${index}"><span>0${index+1}</span>${escape(section.short)}</a>`).join('')}</nav><div>${t.guide.sections.map((section,index) => `<section class="guide-section" id="step-${index}"><div><span class="step-number">0${index+1}</span><h2>${escape(section.title)}</h2><p>${escape(section.body)}</p><ul>${section.points.map(point=>`<li>${escape(point)}</li>`).join('')}</ul></div>${slot(section.preview)}</section>`).join('')}<aside class="callout"><h3>${escape(t.guide.compatibilityTitle)}</h3><p>${escape(t.guide.compatibilityBody)}</p>${link('support',t.guide.compatibilityLink,'text-link')}</aside></div></div>${cta(t)}`;
}
function support(t) {
  return `<section class="page-intro"><div class="wrap"><p class="eyebrow">${escape(t.nav.support)}</p><h1>${escape(t.support.title)}</h1><p class="lead">${escape(t.support.body)}</p></div></section><div class="wrap support-layout"><section><label class="search">${icon('search')}<input type="search" id="faq-search" placeholder="${escape(t.support.search)}" aria-label="${escape(t.support.search)}"></label><h2 class="faq-heading">${escape(t.support.faq)}</h2><div class="faq">${t.support.questions.map((question,index)=>`<details data-faq="${index}"><summary>${escape(question.title)}${icon('plus')}</summary><p>${escape(question.body)}</p>${question.items ? `<ul>${question.items.map(item => `<li><strong>${escape(item.title)}</strong><br>${escape(item.body)} <a href="${escape(item.url)}">${escape(item.link)}</a></li>`).join('')}</ul>` : ''}</details>`).join('')}</div><p class="no-results" hidden>${escape(t.support.empty)}</p></section><aside class="support-aside">${icon('help')}<h2>${escape(t.support.contactTitle)}</h2><p>${escape(t.support.contactBody)}</p><a class="button" href="https://github.com/mjc/libcutout/issues/new?title=CutOut%20support%20request">${escape(t.support.contactLink)}${icon('arrow')}</a><p class="small">${escape(t.support.publicNote)}</p><hr style="border:0;border-top:1px solid #cbd1bf;margin:24px 0"><p class="small">${escape(t.support.reportNote)}</p></aside></div>`;
}
function render() {
  const t = copy[state.language];
  document.documentElement.lang = state.language;
  document.title = `${state.page === 'home' ? t.hero.fullTitle : t.nav[state.page]} — CutOut`;
  app.innerHTML = `${header(t)}<main id="main" tabindex="-1">${({home,guide,support,about})[state.page](t)}</main>${footer(t)}`;
}
function navigate(page) {
  state.page = page;
  history.pushState(null,'',href(page));
  render();
  scrollTo({top:0,behavior:'instant'});
  document.querySelector('#main').focus({preventScroll:true});
}
app.addEventListener('click', event => {
  const route = event.target.closest('a[data-page]');
  if (route && !event.metaKey && !event.ctrlKey && !event.shiftKey && event.button === 0) { event.preventDefault(); navigate(route.dataset.page); }
  const menu = event.target.closest('.menu-toggle');
  if (menu) { const open = menu.getAttribute('aria-expanded') !== 'true'; menu.setAttribute('aria-expanded',String(open)); document.querySelector('#site-navigation').dataset.open = String(open); }
  const feature = event.target.closest('[data-feature]');
  if (feature) {
    selectedFeature = Number(feature.dataset.feature);
    document.querySelectorAll('[data-feature]').forEach(button => button.setAttribute('aria-pressed',String(button === feature)));
    document.querySelector('#feature-preview').innerHTML = slot(copy[state.language].features[selectedFeature].title);
  }
  if (event.target.closest('[data-download]')) document.querySelector('#download-dialog').showModal();
});
app.addEventListener('change', event => {
  if (event.target.id === 'language') { state.language = event.target.value; history.replaceState(null,'',href(state.page)); render(); document.querySelector('#language').focus(); }
});
app.addEventListener('input', event => {
  if (event.target.id !== 'faq-search') return;
  const query = event.target.value.toLocaleLowerCase(state.language).trim();
  let visible = 0;
  document.querySelectorAll('[data-faq]').forEach(detail => { const matches = detail.textContent.toLocaleLowerCase(state.language).includes(query); detail.hidden = !matches; if (matches) visible += 1; });
  document.querySelector('.no-results').hidden = visible !== 0;
});
addEventListener('popstate',() => { state = readLocation(); render(); });
render();
