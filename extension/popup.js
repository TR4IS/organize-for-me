// Lists recent downloads and says where each one is now. Organize for Me answers through
// Chrome's native messaging; without the app, the list still works for files that never moved.
'use strict';

const HOST = 'com.tr4is.organizeforme';
const LIMIT = 25;
const REFRESH_MS = 2000;
const APP_PAGE = 'https://n3trunner.dev/organizeforme/';

const TEXT = {
  en: {
    brand: 'ORGANIZE FOR ME', title: 'YOUR DOWNLOADS', open: 'OPEN', show: 'SHOW FILE',
    moved: folder => `Moved to ${folder}/`, here: 'Not moved', gone: 'Not found', going: 'Downloading…',
    empty: 'No downloads yet.', unknown: 'Needs the app to find',
    noApp: 'Install Organize for Me 3.0.0-beta.6 or newer to see where it moved your files.',
    getApp: 'GET THE APP',
  },
  ar: {
    brand: 'رتب لي', title: 'تنزيلاتك', open: 'افتح', show: 'ورني الملف',
    moved: folder => `انتقل إلى ${folder}`, here: 'ما انتقل', gone: 'مو موجود', going: 'جاري التنزيل…',
    empty: 'ما فيه تنزيلات للحين.', unknown: 'يحتاج التطبيق عشان يلقاه',
    noApp: 'ثبّت «رتب لي» 3.0.0-beta.6 أو أحدث عشان تشوف وين نقل ملفاتك.',
    getApp: 'نزّل التطبيق',
  },
};

let lang = chrome.i18n.getUILanguage().startsWith('ar') ? 'ar' : 'en';
let lastDrawn = '';

const t = () => TEXT[lang];
const baseName = path => path.split(/[\\/]/).pop();

function ask(message) {
  return new Promise(resolve => {
    try {
      chrome.runtime.sendNativeMessage(HOST, message, reply => {
        resolve(chrome.runtime.lastError ? null : reply);
      });
    } catch {
      resolve(null);
    }
  });
}

async function load() {
  const all = await chrome.downloads.search({ orderBy: ['-startTime'], limit: LIMIT * 2 });
  const downloads = all.filter(d => d.filename && d.state !== 'interrupted').slice(0, LIMIT);
  const reply = await ask({ type: 'where', paths: downloads.map(d => d.filename) });
  if (reply && TEXT[reply.lang]) lang = reply.lang;
  return { downloads, found: reply && reply.paths, hasApp: Boolean(reply && reply.ok) };
}

function describe(d, found, hasApp) {
  if (d.state === 'in_progress') return { text: t().going };
  if (found && found.moved) {
    return found.exists
      ? { text: t().moved(found.folder || ''), moved: true, canOpen: found.canOpen }
      : { text: t().gone, gone: true };
  }
  if (d.exists) return { text: t().here, canOpen: true };
  // Without the app there is no way to tell a sorted file from a deleted one.
  return { text: hasApp ? t().gone : t().unknown, gone: true };
}

function button(label, onClick) {
  const b = document.createElement('button');
  b.type = 'button';
  b.textContent = label;
  b.addEventListener('click', onClick);
  return b;
}

function actions(d, info) {
  if (info.gone || d.state !== 'complete') return [];
  if (!info.moved) {
    // Chrome still knows where these are, so its own buttons work.
    return [
      button(t().open, () => { chrome.downloads.open(d.id); window.close(); }),
      button(t().show, () => { chrome.downloads.show(d.id); window.close(); }),
    ];
  }
  const viaApp = type => () => ask({ type, path: d.filename }).then(() => window.close());
  const list = [button(t().show, viaApp('show'))];
  // Programs are only shown in their folder, never started from here.
  if (info.canOpen) list.unshift(button(t().open, viaApp('open')));
  return list;
}

function draw({ downloads, found, hasApp }) {
  const rows = downloads.map((d, i) => ({ d, info: describe(d, found && found[i], hasApp) }));
  const key = JSON.stringify([lang, hasApp, rows.map(r => [r.d.id, r.d.state, r.info])]);
  if (key === lastDrawn) return;
  lastDrawn = key;

  document.documentElement.lang = lang;
  document.documentElement.dir = lang === 'ar' ? 'rtl' : 'ltr';
  document.getElementById('brand').textContent = t().brand;
  document.getElementById('title').textContent = t().title;

  const list = document.getElementById('list');
  list.replaceChildren();
  if (!rows.length) {
    const li = document.createElement('li');
    li.className = 'empty';
    li.textContent = t().empty;
    list.append(li);
  }
  for (const { d, info } of rows) {
    const li = document.createElement('li');
    const name = document.createElement('span');
    name.className = 'name';
    name.textContent = baseName(d.filename);
    name.title = baseName(d.filename);
    const where = document.createElement('span');
    where.className = info.gone ? 'where gone' : 'where';
    where.textContent = info.text;
    const acts = document.createElement('span');
    acts.className = 'acts';
    acts.append(...actions(d, info));
    li.append(name, acts, where);
    list.append(li);
  }

  const note = document.getElementById('note');
  note.hidden = hasApp;
  note.replaceChildren();
  if (!hasApp) {
    const text = document.createElement('span');
    text.textContent = t().noApp;
    note.append(text, button(t().getApp, () => {
      chrome.tabs.create({ url: APP_PAGE });
      window.close();
    }));
  }
}

async function refresh() {
  draw(await load());
}

refresh();
// The app sorts a file a few seconds after it arrives, so the list keeps up while it is open.
setInterval(refresh, REFRESH_MS);
chrome.downloads.onChanged.addListener(refresh);
