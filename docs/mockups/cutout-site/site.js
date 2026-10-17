import { canReplaySignupAttempt, fetchWithTimeout, isCurrentWidgetGeneration, removalAttempt, removalPayload, signupAttempt as buildSignupAttempt, signupPayload as normalizedSignupPayload } from './signup-contract.js?v=20261016-retry-removal';

const assetVersion = new URL(import.meta.url).search;
const dictionaries = await Promise.all(['en', 'es'].map(async language => {
  const response = await fetch(`locales/${language}.json${assetVersion}`);
  if (!response.ok) throw new Error(`Unable to load ${language} copy`);
  return [language, await response.json()];
}));
const copy = Object.fromEntries(dictionaries);
const pages = new Set(['home', 'guide', 'support', 'about', 'signup', 'privacy', 'remove-request']);
let selectedFeature = 0;
let state = readLocation();
let signupValues = { email: '', pev_type: '', pev_model: '', requested_accessories: '' };
let signupConfig = null;
let signupConfigPromise = null;
let signupToken = '';
let signupWidgetId = null;
let signupKey = '';
let signupPayload = '';
let signupAttempt = null;
let signupBusy = false;
let signupMessage = '';
let signupMessageKind = 'error';
let signupReceived = false;
let signupReceivedEmail = '';
let signupScriptPromise = null;
let signupWidgetGeneration = 0;
let signupWidgetError = false;
let removalAttemptState = null;
let removalKey = '';
let removalPayloadState = '';
let removalEmail = '';
let removalReceived = false;
let removalMessage = '';
let removalMessageKind = 'error';
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
function screenshot(file, alt, className = '', loading = 'lazy') {
  return `<figure class="screen-shot ${className}"><img src="/assets/screenshots/en/${file}" alt="${escape(alt)}" loading="${loading}" decoding="async"></figure>`;
}
function featureScreenshot(index, t) {
  const screenshots = [
    ['euc-main.png', t.screenshots.live],
    ['ride-tracking.png', t.screenshots.history],
    ['charging-map.png', t.screenshots.charging]
  ];
  const [file, alt] = screenshots[index];
  return screenshot(file, alt);
}
function guideScreenshot(index, t) {
  if (index === 0) return '';
  const screenshots = [
    ['euc-main.png', t.screenshots.live],
    ['ride-tracking.png', t.screenshots.history]
  ];
  const [file, alt] = screenshots[index - 1];
  return screenshot(file, alt);
}
function header(t) {
  return `<a class="skip" href="#main">${escape(t.skip)}</a><header class="site-header"><div class="wrap header-inner"><a class="brand" data-page="home" href="${href('home')}"><img src="assets/app-icon.png" alt="" width="40" height="40">CutOut</a><nav class="nav" id="site-navigation" aria-label="${escape(t.navigation)}">${link('home',t.nav.app)}${link('guide',t.nav.guide)}${link('support',t.nav.support)}</nav><div class="header-actions"><label class="language">${icon('globe')}<select id="language" aria-label="${escape(t.language)}"><option value="en"${state.language === 'en' ? ' selected' : ''}>English</option><option value="es"${state.language === 'es' ? ' selected' : ''}>Español</option></select></label>${link('signup',t.signup.join,'button header-signup') }<button class="menu-toggle" aria-label="${escape(t.menu)}" aria-controls="site-navigation" aria-expanded="false">${icon('menu')}</button></div></div></header>`;
}
function footer(t) {
  return `<footer class="site-footer"><div class="wrap"><div class="footer-top"><div><a class="brand" href="${href('home')}" data-page="home">CutOut<span aria-hidden="true" style="color:var(--yellow)">↗</span></a><p>${escape(t.footer.description)}</p></div><div class="footer-links">${link('about',t.nav.about)}${link('guide',t.nav.guide)}${link('support',t.nav.support)}${link('privacy',t.footer.signupPrivacy)}<a href="https://cutout.lol/privacy-policy/">${escape(t.footer.privacy)}</a><a href="https://github.com/mjc/libcutout">GitHub ${icon('arrow')}</a></div></div><div class="footer-bottom"><span>© 2026 CutOut</span><span>${escape(t.footer.built)}</span></div></div></footer>`;
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
  return `<section class="hero"><div class="wrap hero-grid"><div class="hero-copy"><p class="eyebrow">${escape(t.hero.eyebrow)}</p><h1>${escape(t.hero.title)}<br><span>${escape(t.hero.accent)}</span></h1><p class="lead">${escape(t.hero.body)}</p><div class="actions">${link('signup',t.signup.join,'button')}${link('guide',t.nav.guide,'button secondary')}</div><p class="hero-note">${escape(t.hero.note)}</p></div><div class="hero-art">${routeArt()}${screenshot('euc-main.png',t.screenshots.live,'hero-screenshot','eager')}</div></div></section><div class="feature-strip"><div class="wrap strip-inner">${t.features.map((feature,index) => `<div class="strip-item">${icon(['ride','map','plug'][index])}${escape(feature.title)}</div>`).join('')}</div></div>${vehicles(t)}<section class="section wrap" id="features"><div class="section-heading"><div><p class="eyebrow">${escape(t.featuresTag)}</p><h2>${escape(t.featuresTitle)}</h2></div><p>${escape(t.featuresBody)}</p></div><div class="feature-stage"><div class="feature-tabs">${t.features.map((feature,index) => `<button class="feature-tab" data-feature="${index}" aria-pressed="${selectedFeature === index}" aria-controls="feature-preview">${icon(['ride','map','plug'][index])}<span><span class="feature-title">${escape(feature.title)}</span><span class="feature-body">${escape(feature.body)}</span></span><span class="arrow">${icon('arrow')}</span></button>`).join('')}</div><div><div class="feature-preview" id="feature-preview">${featureScreenshot(selectedFeature,t)}</div></div></div></section>${accessories(t)}${tuneMarketing(t)}<section class="section steps-section wrap"><div class="steps-grid">${t.steps.map((step,index) => `<article><span class="step-number">0${index+1}</span><h3>${escape(step.title)}</h3><p>${escape(step.body)}</p></article>`).join('')}</div>${link('guide',t.stepsLink,'text-link')}</section><section class="section soundtrack-section"><div class="wrap soundtrack-grid"><div><p class="eyebrow">${escape(t.soundtrack.tag)}</p><h2>${escape(t.soundtrack.title)}</h2><p class="lead">${escape(t.soundtrack.body)}</p></div>${screenshot('euc-main.png',t.screenshots.live)}</div></section><section class="section wrap"><div class="section-heading"><div><p class="eyebrow">${escape(t.openSource.tag)}</p><h2>${escape(t.openSource.title)}</h2></div><p>${escape(t.openSource.body)}</p></div>${link('about',t.openSource.link,'text-link')}</section>${cta(t)}`;
}
function accessories(t) {
  return t.accessories.map((accessory,index) => `<section class="section wrap"><div class="soundtrack-grid ${index === 1 ? 'without-preview' : ''}"><div><p class="eyebrow">${escape(accessory.tag)}</p><h2>${escape(accessory.title)}</h2><p class="lead">${escape(accessory.body)}</p>${link('support',accessory.link,'text-link')}</div>${index === 0 ? screenshot('rgb.jpg',t.screenshots.rgb) : ''}</div></section>`).join('');
}
function tuneMarketing(t) {
  return `<section class="section tune-section"><div class="wrap soundtrack-grid"><div><p class="eyebrow">${escape(t.tune.tag)}</p><h2>${escape(t.tune.title)}</h2><p class="lead">${escape(t.tune.body)}</p></div>${screenshot('tune.png',t.screenshots.tune)}</div></section>`;
}
function about(t) {
  return `<section class="page-intro"><div class="wrap"><p class="eyebrow">${escape(t.nav.about)}</p><h1>${escape(t.about.title)}</h1><p class="lead">${escape(t.about.body)}</p></div></section><section class="section wrap"><div class="steps-grid">${t.about.sections.map(section => `<article><h2 class="about-heading">${escape(section.title)}</h2><p>${escape(section.body)}</p></article>`).join('')}</div><div class="actions"><a class="button" href="https://github.com/mjc/libcutout">${escape(t.about.source)}${icon('arrow')}</a>${link('support',t.about.faq,'text-link')}</div></section>${cta(t)}`;
}
function cta(t) { return `<section class="cta-section"><div class="wrap cta-inner"><h2>${escape(t.cta)}</h2>${link('signup',t.signup.join,'button')}</div></section>`; }
function guide(t) {
  return `<section class="page-intro"><div class="wrap"><p class="eyebrow">${escape(t.nav.guide)}</p><h1>${escape(t.guide.title)}</h1><p class="lead">${escape(t.guide.body)}</p></div></section><div class="wrap guide-layout"><nav class="guide-nav" aria-label="${escape(t.guide.contents)}">${t.guide.sections.map((section,index) => `<a href="#step-${index}"><span>0${index+1}</span>${escape(section.short)}</a>`).join('')}</nav><div>${t.guide.sections.map((section,index) => `<section class="guide-section${index === 0 ? ' guide-section-copy-only' : ''}" id="step-${index}"><div><span class="step-number">0${index+1}</span><h2>${escape(section.title)}</h2><p>${escape(section.body)}</p><ul>${section.points.map(point=>`<li>${escape(point)}</li>`).join('')}</ul></div>${guideScreenshot(index,t)}</section>`).join('')}<aside class="callout"><h3>${escape(t.guide.compatibilityTitle)}</h3><p>${escape(t.guide.compatibilityBody)}</p>${link('support',t.guide.compatibilityLink,'text-link')}</aside></div></div>${cta(t)}`;
}
function support(t) {
  return `<section class="page-intro"><div class="wrap"><p class="eyebrow">${escape(t.nav.support)}</p><h1>${escape(t.support.title)}</h1><p class="lead">${escape(t.support.body)}</p></div></section><div class="wrap support-layout"><section><label class="search">${icon('search')}<input type="search" id="faq-search" placeholder="${escape(t.support.search)}" aria-label="${escape(t.support.search)}"></label><h2 class="faq-heading">${escape(t.support.faq)}</h2><div class="faq">${t.support.questions.map((question,index)=>`<details data-faq="${index}"><summary>${escape(question.title)}${icon('plus')}</summary><p>${escape(question.body)}</p>${question.items ? `<ul>${question.items.map(item => `<li><strong>${escape(item.title)}</strong><br>${escape(item.body)} <a href="${escape(item.url)}">${escape(item.link)}</a></li>`).join('')}</ul>` : ''}</details>`).join('')}</div><p class="no-results" hidden>${escape(t.support.empty)}</p></section><aside class="support-aside">${icon('help')}<h2>${escape(t.support.contactTitle)}</h2><p>${escape(t.support.contactBody)}</p><a class="button" href="https://github.com/mjc/libcutout/issues/new?title=CutOut%20support%20request">${escape(t.support.contactLink)}${icon('arrow')}</a><p class="small">${escape(t.support.publicNote)}</p><hr style="border:0;border-top:1px solid #cbd1bf;margin:24px 0"><p class="small">${escape(t.support.reportNote)}</p></aside></div>`;
}
function signup(t) {
  if (signupReceived) {
    return `<section class="signup-layout wrap"><div class="signup-copy"><p class="eyebrow">${escape(t.signup.tag)}</p><h1 tabindex="-1">${escape(t.signup.receivedTitle)}</h1><p class="lead">${escape(t.signup.receivedBody)}</p></div><section class="signup-panel signup-received" aria-live="polite"><div class="received-mark" aria-hidden="true">✓</div><p>${escape(t.signup.receivedEmail)}</p><p class="received-address">${escape(signupReceivedEmail)}</p>${link('remove-request',t.signup.removeRequestLink,'text-link')}${link('home',t.signup.back,'button')}</section></section>`;
  }
  const enabled = signupConfig?.enabled === true && signupConfig.action === 'alpha_signup' && typeof signupConfig.site_key === 'string' && signupConfig.site_key.length > 0;
  const message = signupMessage || (signupConfig === null ? t.signup.loading : signupConfig.error ? t.signup.configError : signupWidgetError ? t.signup.challengeError : enabled ? '' : t.signup.closed);
  const messageKind = signupMessage ? signupMessageKind : signupConfig?.error ? 'error' : 'status';
  const typeOptions = t.signup.types.map(type => `<option value="${escape(type.value)}"${signupValues.pev_type === type.value ? ' selected' : ''}>${escape(type.label)}</option>`).join('');
  return `<section class="signup-layout wrap"><div class="signup-copy"><p class="eyebrow">${escape(t.signup.tag)}</p><h1>${escape(t.signup.title)}<span>${escape(t.signup.accent)}</span></h1><p class="lead">${escape(t.signup.intro)}</p><ul class="signup-benefits">${t.signup.benefits.map(benefit=>`<li>${escape(benefit)}</li>`).join('')}</ul></div><section class="signup-panel"><h2>${escape(t.signup.formTitle)}</h2><p>${escape(t.signup.formIntro)}</p><form class="signup-form" id="alpha-signup-form" novalidate aria-busy="${String(signupBusy)}"><div class="signup-message" id="signup-message" role="${messageKind === 'error' ? 'alert' : 'status'}" aria-live="${messageKind === 'error' ? 'assertive' : 'polite'}" aria-atomic="true">${escape(message)}</div>${signupConfig?.error || signupWidgetError ? `<button class="button secondary signup-retry-setup" id="signup-retry-setup" type="button">${escape(t.signup.retrySetup)}</button>` : ''}<fieldset class="signup-fields"${signupBusy ? ' disabled' : ''}><div class="field"><label for="signup-email">${escape(t.signup.email)}</label><input id="signup-email" name="email" type="email" autocomplete="email" autocapitalize="none" spellcheck="false" maxlength="254" required value="${escape(signupValues.email)}" aria-describedby="signup-email-help"><p class="field-help" id="signup-email-help">${escape(t.signup.emailHelp)}</p></div><div class="field"><label for="signup-pev">${escape(t.signup.pev)} <span class="optional">(${escape(t.signup.optional)})</span></label><select id="signup-pev" name="pev_type"><option value="">${escape(t.signup.choose)}</option>${typeOptions}</select></div><div class="field"><label for="signup-model">${escape(t.signup.model)} <span class="optional">(${escape(t.signup.optional)})</span></label><input id="signup-model" name="pev_model" type="text" maxlength="120" autocomplete="off" placeholder="${escape(t.signup.modelPlaceholder)}" value="${escape(signupValues.pev_model)}"><p class="field-help">${escape(t.signup.modelHelp)}</p></div><div class="field"><label for="signup-accessories">${escape(t.signup.accessories)} <span class="optional">(${escape(t.signup.optional)})</span></label><textarea id="signup-accessories" name="requested_accessories" rows="3" maxlength="600" placeholder="${escape(t.signup.accessoryPlaceholder)}">${escape(signupValues.requested_accessories)}</textarea><p class="field-help">${escape(t.signup.accessoryHelp)}</p></div><p class="signup-disclosure">${escape(t.signup.privacyLead)} <a href="${href('privacy')}" data-page="privacy">${escape(t.signup.privacy)}</a></p>${enabled ? `<div class="turnstile-reserved" role="group" aria-label="${escape(t.signup.turnstileLabel)}"><span class="field-help">${escape(t.signup.turnstileLabel)}</span><div id="alpha-turnstile"></div></div>` : ''}<div class="signup-honeypot" aria-hidden="true"><label for="signup-website">${escape(t.signup.honeypotLabel)}</label><input id="signup-website" name="website" type="text" tabindex="-1" autocomplete="off"></div><button class="button" id="signup-submit" type="submit"${!enabled || signupBusy ? ' disabled' : ''}>${escape(signupBusy?t.signup.sending:t.signup.submit)}${icon('arrow')}</button></fieldset></form></section></section>`;
}
function signupPrivacy(t) {
  return `<section class="page-intro"><div class="wrap"><p class="eyebrow">${escape(t.signup.tag)}</p><h1>${escape(t.signup.privacyTitle)}</h1><p class="lead">${escape(t.signup.privacyIntro)}</p></div></section><section class="section wrap signup-policy"><article><h2>${escape(t.signup.privacyDataTitle)}</h2><p>${escape(t.signup.privacyData)}</p></article><article><h2>${escape(t.signup.privacyUseTitle)}</h2><p>${escape(t.signup.privacyUse)}</p></article><article><h2>${escape(t.signup.privacyStorageTitle)}</h2><p>${escape(t.signup.privacyStorage)}</p></article><article><h2>${escape(t.signup.privacyCloudflareTitle)}</h2><p>${escape(t.signup.privacyCloudflare)}</p></article><article><h2>${escape(t.signup.privacyRetentionTitle)}</h2><p>${escape(t.signup.privacyRetention)}</p></article><p>${escape(t.signup.privacyContact)} <a href="https://cutout.lol/privacy-policy/">${escape(t.signup.privacyPolicyLink)}</a></p><div class="actions">${link('remove-request',t.signup.removeRequestLink,'text-link')}${link('signup',t.signup.privacyBack,'text-link')}</div></section>`;
}
function removalRequest(t) {
  if (removalReceived) {
    return `<section class="signup-layout wrap"><div class="signup-copy"><p class="eyebrow">${escape(t.signup.removeTag)}</p><h1 tabindex="-1">${escape(t.signup.removeReceivedTitle)}</h1><p class="lead">${escape(t.signup.removeReceivedBody)}</p></div><section class="signup-panel signup-received" aria-live="polite"><div class="received-mark" aria-hidden="true">✓</div><p>${escape(t.signup.removeReceivedGeneric)}</p>${link('privacy',t.signup.privacyTitle,'text-link')}${link('home',t.signup.back,'button')}</section></section>`;
  }
  const enabled = signupConfig?.enabled === true && signupConfig.action === 'alpha_signup' && typeof signupConfig.site_key === 'string' && signupConfig.site_key.length > 0;
  const message = removalMessage || (signupConfig === null ? t.signup.loading : signupConfig.error ? t.signup.configError : signupWidgetError ? t.signup.challengeError : enabled ? '' : t.signup.closed);
  const messageKind = removalMessage ? removalMessageKind : signupConfig?.error || signupWidgetError ? 'error' : 'status';
  return `<section class="signup-layout wrap"><div class="signup-copy"><p class="eyebrow">${escape(t.signup.removeTag)}</p><h1>${escape(t.signup.removeTitle)}</h1><p class="lead">${escape(t.signup.removeIntro)}</p><p>${link('privacy',t.signup.privacyTitle,'text-link')}</p></div><section class="signup-panel"><h2>${escape(t.signup.removeFormTitle)}</h2><p>${escape(t.signup.removeFormIntro)}</p><form class="signup-form" id="alpha-removal-form" novalidate aria-busy="${String(signupBusy)}"><div class="signup-message" id="removal-message" role="${messageKind === 'error' ? 'alert' : 'status'}" aria-live="${messageKind === 'error' ? 'assertive' : 'polite'}" aria-atomic="true">${escape(message)}</div>${signupConfig?.error || signupWidgetError ? `<button class="button secondary signup-retry-setup" id="signup-retry-setup" type="button">${escape(t.signup.retrySetup)}</button>` : ''}<fieldset class="signup-fields"${signupBusy ? ' disabled' : ''}><div class="field"><label for="removal-email">${escape(t.signup.email)}</label><input id="removal-email" name="email" type="email" autocomplete="email" autocapitalize="none" spellcheck="false" maxlength="254" required value="${escape(removalEmail)}" aria-describedby="removal-email-help"><p class="field-help" id="removal-email-help">${escape(t.signup.removeEmailHelp)}</p></div><p class="signup-disclosure">${escape(t.signup.removePrivacyLead)} <a href="${href('privacy')}" data-page="privacy">${escape(t.signup.privacy)}</a></p>${enabled ? `<div class="turnstile-reserved" role="group" aria-label="${escape(t.signup.turnstileLabel)}"><span class="field-help">${escape(t.signup.turnstileLabel)}</span><div id="alpha-turnstile"></div></div>` : ''}<div class="signup-honeypot" aria-hidden="true"><label for="removal-website">${escape(t.signup.honeypotLabel)}</label><input id="removal-website" name="website" type="text" tabindex="-1" autocomplete="off"></div><button class="button" id="removal-submit" type="submit"${!enabled || signupBusy ? ' disabled' : ''}>${escape(signupBusy ? t.signup.sending : t.signup.removeSubmit)}${icon('arrow')}</button></fieldset></form></section></section>`;
}
function showSignupMessage(message, kind = 'error') {
  signupMessage = message;
  signupMessageKind = kind;
  const element = document.querySelector('#signup-message');
  if (element) {
    element.textContent = message;
    element.setAttribute('role', kind === 'error' ? 'alert' : 'status');
    element.setAttribute('aria-live', kind === 'error' ? 'assertive' : 'polite');
    element.classList.toggle('is-error', kind === 'error' && Boolean(message));
    element.classList.toggle('is-status', kind === 'status' && Boolean(message));
    if (signupWidgetError && !document.querySelector('#signup-retry-setup')) {
      const retry = document.createElement('button');
      retry.className = 'button secondary signup-retry-setup';
      retry.id = 'signup-retry-setup';
      retry.type = 'button';
      retry.textContent = copy[state.language].signup.retrySetup;
      element.after(retry);
    }
  }
}
function destroySignupWidget() {
  signupWidgetGeneration += 1;
  if (signupWidgetId !== null && window.turnstile?.remove) {
    try { window.turnstile.remove(signupWidgetId); } catch {}
  }
  signupWidgetId = null;
  signupToken = '';
}
function reportTurnstileError(fatal = false) {
  if (fatal) signupWidgetError = true;
  const message = copy[state.language].signup.challengeError;
  if (state.page === 'remove-request') showRemovalMessage(message);
  else showSignupMessage(message);
}
function loadTurnstileApi() {
  if (window.turnstile) return Promise.resolve();
  if (!signupScriptPromise) {
    signupScriptPromise = new Promise((resolve, reject) => {
      let script = document.querySelector('script[data-turnstile-api]');
      if (!script) {
        script = document.createElement('script');
        script.src = 'https://challenges.cloudflare.com/turnstile/v0/api.js?render=explicit';
        script.async = true;
        script.defer = true;
        script.dataset.turnstileApi = 'true';
        document.head.append(script);
      }
      let settled = false;
      let timeout;
      const finish = error => {
        if (settled) return;
        settled = true;
        clearTimeout(timeout);
        if (error) {
          signupScriptPromise = null;
          script.remove();
          reject(error);
        } else {
          resolve();
        }
      };
      timeout = setTimeout(() => finish(new Error('Turnstile load timed out')), 12000);
      script.addEventListener('load', () => finish(), { once: true });
      script.addEventListener('error', () => finish(new Error('Turnstile unavailable')), { once: true });
      if (window.turnstile) finish();
    });
  }
  return signupScriptPromise;
}
function initializeSignup() {
  if (!['signup', 'remove-request'].includes(state.page) || (state.page === 'signup' && signupReceived) || (state.page === 'remove-request' && removalReceived) || signupWidgetId !== null || signupWidgetError || signupConfig?.error || signupConfig?.enabled === false) return;
  if (signupConfig === null) {
    if (signupConfigPromise) return;
    signupConfigPromise = fetchWithTimeout(window.fetch.bind(window), '/v1/alpha-signup/config', { headers: { accept: 'application/json' }, cache: 'no-store', credentials: 'same-origin' }, 10000)
      .then(response => {
        if (!response.ok) throw new Error('Signup configuration unavailable');
        return response.json();
      })
      .then(config => {
        if (typeof config.enabled !== 'boolean' || config.action !== 'alpha_signup' || (config.enabled && (typeof config.site_key !== 'string' || !config.site_key))) throw new Error('Invalid signup configuration');
        signupConfig = config;
        signupConfigPromise = null;
        if (['signup', 'remove-request'].includes(state.page)) render();
      })
      .catch(() => {
        signupConfig = { error: true, enabled: false };
        signupConfigPromise = null;
        if (['signup', 'remove-request'].includes(state.page)) render();
      });
    return;
  }
  if (signupConfig.enabled !== true || !signupConfig.site_key) return;
  loadTurnstileApi().then(() => {
    if (!['signup', 'remove-request'].includes(state.page) || (state.page === 'signup' && signupReceived) || (state.page === 'remove-request' && removalReceived) || signupWidgetId !== null || !document.querySelector('#alpha-turnstile')) return;
    signupWidgetError = false;
    const generation = ++signupWidgetGeneration;
    signupWidgetId = window.turnstile.render('#alpha-turnstile', {
      sitekey: signupConfig.site_key,
      action: 'alpha_signup',
      appearance: 'interaction-only',
      language: state.language,
      size: 'normal',
      callback(token) { if (isCurrentWidgetGeneration(generation, signupWidgetGeneration)) signupToken = token; },
      'expired-callback'() { if (isCurrentWidgetGeneration(generation, signupWidgetGeneration)) { signupToken = ''; reportTurnstileError(); } },
      'error-callback'() { if (isCurrentWidgetGeneration(generation, signupWidgetGeneration)) { signupToken = ''; reportTurnstileError(true); } }
    });
  }).catch(() => {
    signupWidgetError = true;
    reportTurnstileError(true);
    if (['signup', 'remove-request'].includes(state.page)) render();
  });
}
function retrySignupSetup() {
  signupMessage = '';
  removalMessage = '';
  signupWidgetError = false;
  if (signupConfig?.error) {
    signupConfig = null;
    signupConfigPromise = null;
  } else {
    destroySignupWidget();
    signupScriptPromise = null;
    if (document.querySelector('script[data-turnstile-api]') && !window.turnstile) {
      document.querySelector('script[data-turnstile-api]').remove();
    }
  }
  render();
}
function readSignupForm(form) {
  const data = new FormData(form);
  signupValues = {
    email: String(data.get('email') || ''),
    pev_type: String(data.get('pev_type') || ''),
    pev_model: String(data.get('pev_model') || ''),
    requested_accessories: String(data.get('requested_accessories') || '')
  };
  return { website: String(data.get('website') || '') };
}
function showRemovalMessage(message, kind = 'error') {
  removalMessage = message;
  removalMessageKind = kind;
  const element = document.querySelector('#removal-message');
  if (!element) return;
  element.textContent = message;
  element.setAttribute('role', kind === 'error' ? 'alert' : 'status');
  element.setAttribute('aria-live', kind === 'error' ? 'assertive' : 'polite');
  if (signupWidgetError && !document.querySelector('#signup-retry-setup')) {
    const retry = document.createElement('button');
    retry.className = 'button secondary signup-retry-setup';
    retry.id = 'signup-retry-setup';
    retry.type = 'button';
    retry.textContent = copy[state.language].signup.retrySetup;
    element.after(retry);
  }
}
function focusReceiptHeading() {
  scrollTo({ top: 0, behavior: 'instant' });
  document.querySelector('.signup-copy h1[tabindex="-1"]')?.focus({ preventScroll: true });
}
function readRemovalForm(form) {
  const data = new FormData(form);
  removalEmail = String(data.get('email') || '');
  return {
    email: removalEmail,
    website: String(data.get('website') || '')
  };
}
async function submitRemoval(form) {
  if (signupBusy) return;
  const submitted = readRemovalForm(form);
  const email = form.querySelector('#removal-email');
  if (!email.checkValidity()) {
    showRemovalMessage(copy[state.language].signup.emailInvalid);
    email.setAttribute('aria-invalid', 'true');
    email.focus();
    return;
  }
  const payload = removalPayload(submitted.email);
  if (removalPayloadState && removalPayloadState !== payload) {
    removalKey = '';
    removalPayloadState = '';
    removalAttemptState = null;
    signupToken = '';
    if (signupWidgetId !== null && window.turnstile?.reset) {
      try { window.turnstile.reset(signupWidgetId); } catch {}
    }
  }
  const canReplay = canReplaySignupAttempt(removalAttemptState, payload);
  if (!canReplay && !signupToken) {
    showRemovalMessage(copy[state.language].signup.challengeRequired);
    return;
  }
  if (!removalKey || removalPayloadState !== payload) {
    removalKey = crypto.randomUUID();
    removalPayloadState = payload;
    removalAttemptState = null;
  }
  if (!canReplay) {
    removalAttemptState = removalAttempt({
      email: submitted.email,
      idempotencyKey: removalKey,
      turnstileToken: signupToken,
      website: submitted.website
    });
  }
  const focusedControlId = document.activeElement?.id;
  signupBusy = true;
  form.querySelector('.signup-fields').disabled = true;
  const button = form.querySelector('#removal-submit');
  button.disabled = true;
  button.setAttribute('aria-busy', 'true');
  form.setAttribute('aria-busy', 'true');
  button.firstChild.textContent = copy[state.language].signup.sending;
  showRemovalMessage(copy[state.language].signup.sending, 'status');
  let refreshChallenge = false;
  try {
    const response = await fetchWithTimeout(window.fetch.bind(window), '/v1/alpha-signup-removals', {
      method: 'POST',
      headers: { 'content-type': 'application/json', accept: 'application/json' },
      credentials: 'same-origin',
      body: JSON.stringify(removalAttemptState.body)
    });
    if ([200, 201, 202].includes(response.status)) {
      removalReceived = true;
      removalAttemptState = null;
      removalKey = '';
      removalPayloadState = '';
      signupBusy = false;
      signupMessage = '';
      removalMessage = '';
      destroySignupWidget();
      render();
      focusReceiptHeading();
      return;
    }
    if (response.status === 422 || response.status === 429) {
      removalAttemptState = null;
      refreshChallenge = true;
    }
    const text = response.status === 422 ? copy[state.language].signup.invalid
      : response.status === 429 ? copy[state.language].signup.rateLimited
      : response.status === 503 ? copy[state.language].signup.serviceUnavailable
      : copy[state.language].signup.error;
    showRemovalMessage(text);
  } catch {
    showRemovalMessage(copy[state.language].signup.error);
  } finally {
    signupBusy = false;
    const currentForm = document.querySelector('#alpha-removal-form');
    if (currentForm) {
      currentForm.querySelector('.signup-fields').disabled = false;
      currentForm.setAttribute('aria-busy', 'false');
      const currentButton = currentForm.querySelector('#removal-submit');
      currentButton.disabled = signupConfig?.enabled !== true;
      currentButton.removeAttribute('aria-busy');
      currentButton.firstChild.textContent = copy[state.language].signup.removeSubmit;
      if (!removalReceived && focusedControlId) currentForm.querySelector(`#${focusedControlId}`)?.focus({ preventScroll: true });
    }
    if (!removalReceived && refreshChallenge) {
      signupToken = '';
      if (signupWidgetId !== null && window.turnstile?.reset) {
        try { window.turnstile.reset(signupWidgetId); } catch {}
      }
    }
  }
}
async function submitSignup(form) {
  if (signupBusy) return;
  const honeypot = readSignupForm(form).website;
  const email = form.querySelector('#signup-email');
  if (!email.checkValidity()) {
    showSignupMessage(copy[state.language].signup.emailInvalid);
    email.setAttribute('aria-invalid', 'true');
    email.focus();
    return;
  }
  const submittedValues = { ...signupValues, email: signupValues.email.trim() };
  const payload = normalizedSignupPayload(signupValues, state.language);
  if (signupPayload && signupPayload !== payload) {
    signupKey = '';
    signupPayload = '';
    signupAttempt = null;
    signupToken = '';
    if (signupWidgetId !== null && window.turnstile?.reset) {
      try { window.turnstile.reset(signupWidgetId); } catch {}
    }
  }
  const canReplay = canReplaySignupAttempt(signupAttempt, payload);
  if (!canReplay && !signupToken) {
    showSignupMessage(copy[state.language].signup.challengeRequired);
    return;
  }
  if (!signupKey || signupPayload !== payload) {
    signupKey = crypto.randomUUID();
    signupPayload = payload;
    signupAttempt = null;
  }
  if (!canReplay) {
    signupAttempt = buildSignupAttempt({
      values: signupValues,
      locale: state.language,
      idempotencyKey: signupKey,
      turnstileToken: signupToken,
      website: honeypot
    });
  }
  const body = signupAttempt.body;
  const focusedControlId = document.activeElement?.id;
  signupBusy = true;
  const button = form.querySelector('#signup-submit');
  button.disabled = true;
  button.setAttribute('aria-busy', 'true');
  form.setAttribute('aria-busy', 'true');
  button.firstChild.textContent = copy[state.language].signup.sending;
  showSignupMessage(copy[state.language].signup.sending, 'status');
  let refreshChallenge = false;
  try {
    const response = await fetchWithTimeout(window.fetch.bind(window), '/v1/alpha-signup/requests', {
      method: 'POST',
      headers: { 'content-type': 'application/json', accept: 'application/json' },
      credentials: 'same-origin',
      body: JSON.stringify(body)
    });
    if ([200, 201, 202].includes(response.status)) {
      signupReceived = true;
      signupReceivedEmail = submittedValues.email;
      signupValues = { email: '', pev_type: '', pev_model: '', requested_accessories: '' };
      signupKey = '';
      signupPayload = '';
      signupAttempt = null;
      destroySignupWidget();
      signupBusy = false;
      signupMessage = '';
      render();
      focusReceiptHeading();
      return;
    }
    if (response.status === 422 || response.status === 429) {
      signupAttempt = null;
      refreshChallenge = true;
    }
    const text = response.status === 422 ? copy[state.language].signup.invalid
      : response.status === 429 ? copy[state.language].signup.rateLimited
      : response.status === 503 ? copy[state.language].signup.serviceUnavailable
      : copy[state.language].signup.error;
    showSignupMessage(text);
  } catch {
    // A lost response or timeout may follow durable acceptance. Keep the exact key, payload and challenge for replay.
    showSignupMessage(copy[state.language].signup.error);
  } finally {
    signupBusy = false;
    const currentForm = document.querySelector('#alpha-signup-form');
    if (currentForm) {
      currentForm.querySelector('.signup-fields').disabled = false;
      currentForm.setAttribute('aria-busy', 'false');
      const currentButton = currentForm.querySelector('#signup-submit');
      currentButton.disabled = signupConfig?.enabled !== true;
      currentButton.removeAttribute('aria-busy');
      currentButton.firstChild.textContent = copy[state.language].signup.submit;
      if (!signupReceived && focusedControlId) currentForm.querySelector(`#${focusedControlId}`)?.focus({ preventScroll: true });
    }
    if (!signupReceived && refreshChallenge) {
      signupToken = '';
      if (signupWidgetId !== null && window.turnstile?.reset) {
        try { window.turnstile.reset(signupWidgetId); } catch {}
      }
    }
  }
}
function render() {
  const t = copy[state.language];
  document.documentElement.lang = state.language;
  const title = state.page === 'home' ? t.hero.fullTitle : state.page === 'signup' ? t.signup.join : state.page === 'privacy' ? t.signup.privacyTitle : state.page === 'remove-request' ? t.signup.removeTitle : t.nav[state.page];
  document.title = `${title} — CutOut`;
  app.innerHTML = `${header(t)}<main id="main" tabindex="-1">${({home,guide,support,about,signup,privacy:signupPrivacy,'remove-request':removalRequest})[state.page](t)}</main>${footer(t)}`;
  if (['signup', 'remove-request'].includes(state.page)) initializeSignup();
}
function navigate(page) {
  if (['signup', 'remove-request'].includes(state.page)) destroySignupWidget();
  state.page = page;
  history.pushState(null,'',href(page));
  render();
  scrollTo({top:0,behavior:'instant'});
  document.querySelector('#main').focus({preventScroll:true});
}
app.addEventListener('click', event => {
  if (event.target.closest('#signup-retry-setup')) { retrySignupSetup(); return; }
  const route = event.target.closest('a[data-page]');
  if (route && !event.metaKey && !event.ctrlKey && !event.shiftKey && event.button === 0) { event.preventDefault(); navigate(route.dataset.page); }
  const menu = event.target.closest('.menu-toggle');
  if (menu) { const open = menu.getAttribute('aria-expanded') !== 'true'; menu.setAttribute('aria-expanded',String(open)); document.querySelector('#site-navigation').dataset.open = String(open); }
  const feature = event.target.closest('[data-feature]');
  if (feature) {
    selectedFeature = Number(feature.dataset.feature);
    document.querySelectorAll('[data-feature]').forEach(button => button.setAttribute('aria-pressed',String(button === feature)));
    document.querySelector('#feature-preview').innerHTML = featureScreenshot(selectedFeature,copy[state.language]);
  }
});
app.addEventListener('change', event => {
  if (event.target.id === 'language') { if (['signup', 'remove-request'].includes(state.page)) destroySignupWidget(); state.language = event.target.value; history.replaceState(null,'',href(state.page)); render(); document.querySelector('#language').focus(); }
  if (event.target.closest('#alpha-signup-form')) readSignupForm(event.target.form);
  if (event.target.closest('#alpha-removal-form')) readRemovalForm(event.target.form);
});
app.addEventListener('input', event => {
  if (event.target.closest('#alpha-signup-form')) {
    readSignupForm(event.target.form);
    if (event.target.id === 'signup-email') event.target.removeAttribute('aria-invalid');
    if (signupMessage) showSignupMessage('', 'status');
    return;
  }
  if (event.target.closest('#alpha-removal-form')) {
    readRemovalForm(event.target.form);
    if (removalMessage) showRemovalMessage('', 'status');
    if (event.target.id === 'removal-email') event.target.removeAttribute('aria-invalid');
    return;
  }
  if (event.target.id !== 'faq-search') return;
  const query = event.target.value.toLocaleLowerCase(state.language).trim();
  let visible = 0;
  document.querySelectorAll('[data-faq]').forEach(detail => { const matches = detail.textContent.toLocaleLowerCase(state.language).includes(query); detail.hidden = !matches; if (matches) visible += 1; });
  document.querySelector('.no-results').hidden = visible !== 0;
});
app.addEventListener('submit', event => {
  if (event.target.id === 'alpha-signup-form') {
    event.preventDefault();
    void submitSignup(event.target);
  } else if (event.target.id === 'alpha-removal-form') {
    event.preventDefault();
    void submitRemoval(event.target);
  }
});
addEventListener('popstate',() => { if (['signup', 'remove-request'].includes(state.page)) destroySignupWidget(); state = readLocation(); render(); });
render();
