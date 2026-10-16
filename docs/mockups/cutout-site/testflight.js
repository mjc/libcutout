const escape = value => String(value).replace(/[&<>"']/g, character => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[character]));
const copy = {
  en: {
    mockups:'Signup mockups', views:['Signup','Received','Retry','Lific document'], language:'Language', nav:['The app','How it works','Support'],
    join:'Join the alpha list', tag:'CutOut for iPhone · TestFlight', title:'Ride', accent:'the alpha.',
    intro:'CutOut is getting ready for TestFlight. Join the list and we’ll email you when the alpha opens.',
    benefits:['Live ride data and saved routes','External RGB lights and Wi-Fi cameras','The songs that went with your ride'],
    formTitle:'Be one of the first.', formIntro:'Leave your email. Tell us what you ride if you’d like.', email:'Email address', emailHelp:'Where you’d like to hear from us.', optional:'optional', pev:'What do you ride?', choose:'Choose a type', types:['VESC Onewheel / Refloat','EUC','Eskate','Scooter','Ebike','Emoto','Other / not sure'], model:'PEV brand and model', modelPlaceholder:'e.g. Begode Falcon or NOSFET Aero', accessories:'Accessories you’d like supported', accessoryHelp:'Brand and model for lights, cameras or other accessories.', accessoryPlaceholder:'e.g. INNOVV K6 camera',
    disclosure:'We’ll use your email for the CutOut alpha and related beta contact.', privacy:'Privacy policy', submit:'Join the alpha list', sending:'Joining…', invalid:'Enter a valid email address.',
    error:'We couldn’t save your request. Your details are still here—please try again.', retry:'Try again',
    receivedTitle:'You’re on the list.', receivedBody:'We’ll email you when the CutOut alpha opens. There’s nothing to install yet.', address:'We’ll contact you at', back:'Back to CutOut',
    footer:'For riders. With the community.', documentTag:'Private Lific intake · Example data', documentTitle:'CutOut alpha signup requests', documentIntro:'A review list for the maintainer. Requests arrive here; TestFlight invitations are a separate step.', documentAccess:'Libcutout project · Authorized members', currentCohort:'Early alpha', received:'Received', modelLabel:'PEV / accessories', status:'Review status', new:'New', reviewed:'Reviewed', later:'Contact later', note:'Operator notes', documentNote:'No Apple invitations sent. Request IDs keep retries from adding the same person twice. Manual review notes stay with each request.'
  },
  es: {
    mockups:'Propuestas de registro', views:['Formulario','Recibido','Reintentar','Documento Lific'], language:'Idioma', nav:['La aplicación','Cómo funciona','Ayuda'],
    join:'Apúntate a la alfa', tag:'CutOut para iPhone · TestFlight', title:'Sal a rodar', accent:'con la alfa.',
    intro:'Estamos preparando CutOut para TestFlight. Apúntate y te escribiremos cuando se abra la alfa.',
    benefits:['Datos en directo y rutas guardadas','Luces RGB externas y cámaras Wi-Fi','Las canciones de cada recorrido'],
    formTitle:'Sé de los primeros.', formIntro:'Deja tu correo y, si quieres, cuéntanos qué conduces.', email:'Correo electrónico', emailHelp:'Dónde quieres recibir noticias de la alfa.', optional:'opcional', pev:'¿Qué conduces?', choose:'Elige un tipo', types:['VESC Onewheel / Refloat','EUC','Eskate','Scooter','Ebike','Emoto','Otro / no lo sé'], model:'Marca y modelo del PEV', modelPlaceholder:'p. ej., Begode Falcon o NOSFET Aero', accessories:'Accesorios que quieres que sean compatibles', accessoryHelp:'Marca y modelo de luces, cámaras u otros accesorios.', accessoryPlaceholder:'p. ej., cámara INNOVV K6',
    disclosure:'Usaremos tu correo para la alfa de CutOut y el contacto relacionado con la beta.', privacy:'Política de privacidad', submit:'Apuntarme a la alfa', sending:'Enviando…', invalid:'Introduce un correo electrónico válido.',
    error:'No hemos podido guardar la solicitud. Tus datos siguen aquí; vuelve a intentarlo.', retry:'Reintentar',
    receivedTitle:'Ya estás en la lista.', receivedBody:'Te escribiremos cuando se abra la alfa de CutOut. Todavía no hay nada que instalar.', address:'Te contactaremos en', back:'Volver a CutOut',
    footer:'Para riders. Con la comunidad.', documentTag:'Lista privada en Lific · Datos de ejemplo', documentTitle:'Solicitudes para la alfa de CutOut', documentIntro:'Una lista para el mantenedor. Aquí llegan las solicitudes; las invitaciones de TestFlight son un paso aparte.', documentAccess:'Proyecto Libcutout · Miembros autorizados', currentCohort:'Primera alfa', received:'Recibido', modelLabel:'PEV / accesorios', status:'Estado de revisión', new:'Nuevo', reviewed:'Revisado', later:'Contactar después', note:'Notas', documentNote:'No se han enviado invitaciones de Apple. Los identificadores evitan solicitudes duplicadas al reintentar. Las notas de revisión se conservan con cada solicitud.'
  }
};
const views = ['form','received','retry','lific'];
const query = new URLSearchParams(location.search);
let language = query.get('lang') === 'es' ? 'es' : 'en';
let view = views.includes(query.get('view')) ? query.get('view') : 'form';
let values = {email:'',pev:'',model:'',accessories:''};
if (view === 'retry' || view === 'received') values = {email:'rider@example.com',pev:'1',model:'Begode Falcon',accessories:'INNOVV K6 camera'};
let submitting = false;
let submitTimer;
const app = document.querySelector('#signup-app');
const arrow = '<svg class="icon" viewBox="0 0 24 24" aria-hidden="true"><path d="M5 12h14m-6-6 6 6-6 6"/></svg>';
function url(next) { return `testflight.html?view=${next}&lang=${language}`; }
function controls(t) {
  return `<nav class="review-nav wrap" aria-label="${escape(t.mockups)}"><strong>${escape(t.mockups)}</strong>${views.map((name,index)=>`<a href="${url(name)}" data-view="${name}"${name===view?' aria-current="page"':''}>${escape(t.views[index])}</a>`).join('')}</nav>`;
}
function header(t) {
  return `<header class="site-header"><div class="wrap header-inner"><a class="brand" href="index.html?lang=${language}"><img src="assets/app-icon.png" alt="" width="40" height="40">CutOut</a><nav class="nav" aria-label="Navigation">${['home','guide','support'].map((page,index)=>`<a href="index.html?page=${page}&lang=${language}">${escape(t.nav[index])}</a>`).join('')}</nav><div class="header-actions"><label class="language"><select id="language" aria-label="${escape(t.language)}"><option value="en"${language==='en'?' selected':''}>English</option><option value="es"${language==='es'?' selected':''}>Español</option></select></label><a class="button" data-view="form" href="${url('form')}">${escape(t.join)}</a></div></div></header>`;
}
function form(t) {
  return `<section class="signup-panel"><h2>${escape(t.formTitle)}</h2><p>${escape(t.formIntro)}</p><form class="signup-form" novalidate>${view==='retry'?`<div class="signup-error" role="alert">${escape(t.error)}</div>`:''}<div class="field"><label for="email">${escape(t.email)}</label><input id="email" name="email" type="email" autocomplete="email" autocapitalize="none" spellcheck="false" maxlength="254" placeholder="rider@example.com" required value="${escape(values.email)}" aria-describedby="email-help email-error"><p class="field-help" id="email-help">${escape(t.emailHelp)}</p><p class="field-error" id="email-error" hidden>${escape(t.invalid)}</p></div><div class="field"><label for="pev">${escape(t.pev)} <span class="optional">(${escape(t.optional)})</span></label><select id="pev" name="pev"><option value="">${escape(t.choose)}</option>${t.types.map((type,index)=>`<option value="${index}"${values.pev===String(index)?' selected':''}>${escape(type)}</option>`).join('')}</select></div><div class="field"><label for="model">${escape(t.model)} <span class="optional">(${escape(t.optional)})</span></label><input id="model" name="model" type="text" maxlength="120" placeholder="${escape(t.modelPlaceholder)}" value="${escape(values.model)}"></div><div class="field"><label for="accessories">${escape(t.accessories)} <span class="optional">(${escape(t.optional)})</span></label><textarea id="accessories" name="accessories" rows="3" maxlength="600" placeholder="${escape(t.accessoryPlaceholder)}" aria-describedby="accessory-help">${escape(values.accessories)}</textarea><p class="field-help" id="accessory-help">${escape(t.accessoryHelp)}</p></div><p class="signup-disclosure">${escape(t.disclosure)} <a href="https://cutout.lol/privacy-policy/">${escape(t.privacy)}</a></p><button class="button" type="submit"${submitting?' disabled':''}>${escape(submitting?t.sending:view==='retry'?t.retry:t.submit)}${arrow}</button></form></section>`;
}
function received(t) {
  return `<section class="signup-panel received" aria-live="polite"><div class="received-mark" aria-hidden="true">✓</div><h2 tabindex="-1" id="received-title">${escape(t.receivedTitle)}</h2><p>${escape(t.receivedBody)}</p><div class="received-address"><small>${escape(t.address)}</small>${escape(values.email || 'rider@example.com')}</div><a class="button" href="index.html?lang=${language}">${escape(t.back)}${arrow}</a></section>`;
}
function signup(t) {
  return `<main class="wrap signup-layout"><div class="signup-copy"><p class="eyebrow">${escape(t.tag)}</p><h1>${escape(t.title)}<span>${escape(t.accent)}</span></h1><p class="lead">${escape(t.intro)}</p><ul class="signup-benefits">${t.benefits.map(benefit=>`<li>${escape(benefit)}</li>`).join('')}</ul></div>${view==='received'?received(t):form(t)}</main>`;
}
function intakeDocument(t) {
  const currentPev=[values.pev!==''?t.types[Number(values.pev)]:'',values.model].filter(Boolean).join(' · ') || '—';
  const rows=[['alpha-001',values.email || 'rider@example.com',values.email?currentPev:'EUC · Begode Falcon',t.new,values.email?(values.accessories || '—'):'INNOVV K6 camera'],['alpha-002','trail@example.com','VESC Onewheel · Refloat',t.reviewed,'Bluetooth RGB controller'],['alpha-003','commute@example.com','Ebike',t.later,'—']];
  return `<main class="intake wrap"><p class="eyebrow">${escape(t.documentTag)}</p><h1>${escape(t.documentTitle)}</h1><p class="lead">${escape(t.documentIntro)}</p><article class="document"><p>${escape(t.documentAccess)}</p><h2>${escape(t.currentCohort)}</h2><div class="intake-list">${rows.map(row=>`<section class="intake-row"><div><strong>${escape(t.email)}</strong>${escape(row[1])}<small>${escape(row[0])}</small></div><div><strong>${escape(t.received)}</strong>2026-10-16<small>12:30 MDT</small></div><div><strong>${escape(t.modelLabel)}</strong>${escape(row[2])}<small>${escape(t.accessories)}: ${escape(row[4])}</small></div><div><strong>${escape(t.status)}</strong><span class="intake-status">${escape(row[3])}</span></div></section>`).join('')}</div><p class="document-note">${escape(t.documentNote)}</p></article></main>`;
}
function render() {
  const t=copy[language];
  window.document.documentElement.lang=language;
  window.document.title=`${t.join} — CutOut`;
  app.innerHTML=`${controls(t)}${header(t)}${view==='lific'?intakeDocument(t):signup(t)}<footer class="site-footer"><div class="wrap footer-bottom"><span>© 2026 CutOut</span><span>${escape(t.footer)}</span></div></footer>`;
}
function remember() {
  const form=app.querySelector('form');
  if (form) values=Object.fromEntries(new FormData(form));
}
app.addEventListener('click',event=>{
  const link=event.target.closest('[data-view]');
  if (!link || event.metaKey || event.ctrlKey || event.shiftKey || event.button!==0) return;
  event.preventDefault();remember();clearTimeout(submitTimer);view=link.dataset.view;submitting=false;
  if ((view==='received'||view==='retry')&&!values.email) values={email:'rider@example.com',pev:'1',model:'Begode Falcon',accessories:'INNOVV K6 camera'};
  history.replaceState(null,'',url(view));render();
});
app.addEventListener('change',event=>{
  if (event.target.id!=='language') return;
  remember();language=event.target.value;history.replaceState(null,'',url(view));render();
});
app.addEventListener('input',event=>{
  if (event.target.id==='email') { event.target.removeAttribute('aria-invalid');app.querySelector('#email-error').hidden=true; }
});
app.addEventListener('submit',event=>{
  event.preventDefault();
  const email=app.querySelector('#email');email.value=email.value.trim();
  if (!email.checkValidity()) { email.setAttribute('aria-invalid','true');app.querySelector('#email-error').hidden=false;email.focus();return; }
  remember();submitting=true;render();
  // Prototype only: no HTTP submission, browser storage, Lific write or email.
  submitTimer=setTimeout(()=>{submitting=false;view='received';history.replaceState(null,'',url(view));render();app.querySelector('#received-title').focus();},350);
});
render();
