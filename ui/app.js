(function () {
'use strict';
/* Text, layout and drawing for the receipt. All decisions and all file moves happen in Rust. */
const COLS = 38;
const BIDI = /[\u202a-\u202e\u2066-\u2069\u200e\u200f]/g;
const CTRL = /[\u0000-\u001f\u007f]/g;
const segs = typeof Intl !== 'undefined' && Intl.Segmenter ? new Intl.Segmenter() : null;
const graphemes = s => segs ? [...segs.segment(s)].map(x => x.segment) : [...s];
function gw(g) {
  const c = g.codePointAt(0);
  if (/\p{Emoji_Presentation}/u.test(g)) return 3;
  return (c >= 0x1100 && c <= 0x115f) || (c >= 0x2e80 && c <= 0xa4cf) || (c >= 0xac00 && c <= 0xd7a3) || (c >= 0xf900 && c <= 0xfaff) || (c >= 0xff00 && c <= 0xff60) ? 2 : 1;
}
const width = s => graphemes(s).reduce((t, g) => t + gw(g), 0);
const clean = s => s.replace(BIDI, '').replace(CTRL, '?');
const extOf = n => { const i = n.lastIndexOf('.'); return i > 0 && i < n.length - 1 ? n.slice(i + 1).toLowerCase() : ''; };
function clip(s, n) {
  let out = '', w = 0;
  for (const g of graphemes(s)) { if (w + gw(g) > n) break; out += g; w += gw(g); }
  return out;
}
function fit(name, n) {
  if (width(name) <= n) return name;
  const e = extOf(name), tail = e && e.length <= 8 ? '.' + e : '';
  return clip(name, n - width(tail) - 2) + '..' + tail;
}
function size(b) {
  const u = ['B','KB','MB','GB','TB','PB','EB']; let v = b, i = 0;
  while (v >= 1024 && i < u.length - 1) { v /= 1024; i++; }
  if (v >= 1000 && i < u.length - 1) { v /= 1024; i++; }
  return (i ? v.toFixed(1) : String(Math.round(v))) + ' ' + u[i];
}
function count(n) {
  return n >= 1e9 ? (n / 1e9).toFixed(1) + 'B' : n >= 1e6 ? (n / 1e6).toFixed(1) + 'M' : n.toLocaleString('en-US');
}
const tallyOf = items => { const t = { moved: 0, void: 0, held: 0, wait: 0, skip: 0, fail: 0 }; items.forEach(i => { t[i.status] += i.count; }); t.out = t.moved + t.void + t.held + t.wait + t.skip + t.fail; return t; };
const EXTBINS = new Set(['pdf','doc','image','video','sound','zip','exe','random']);
/* Occasions: suggest the paper from 5 days before to 5 days after each date. */
function holidayFor(d) {
  const m = d.getMonth() + 1, x = d.getDate();
  if (m === 9 && x >= 18 && x <= 28) return 'national';
  if (m === 2 && x >= 17 && x <= 27) return 'founding';
  return null;
}
const carry = i => i.status === 'wait' || i.status === 'held';
const app = document.getElementById('app');
const reduce = matchMedia('(prefers-reduced-motion: reduce)').matches;
const T = {
  en: { dir:'ltr', watching:'WATCHING', paused:'PAUSED', receipt:'RECEIPT', opened:'OPENED', time:'TIME', item:'ITEM', size:'SIZE', voided:'VOIDED', held:'HELD FOR YOU', waiting:'WAITING', left:'LEFT ALONE', failed:'FAILED', total:'TOTAL MOVED', ledger:(a,b)=>'LEDGER '+a+' IN, '+b+' OUT', balanced:'BALANCED', check:'CHECK', all:'ALL TIME', file:'file', files:'files', none:'Nothing moved yet', fresh:'New receipt. Waiting for a file.', pause:'PAUSE', resume:'RESUME', next:'NEW', past:'PAST', settings:'SETTINGS', needs:'NEEDS YOU', other:'OTHER', pastT:'PAST RECEIPTS', back:'BACK TO CURRENT', tray:s=>s+' in tray', open:'OPEN', quit:'App closed. Watching stopped.', conf:'CONFIDENCE', moved:'moved',
    k:{ NAME:'NAME', SAVED:'SAVED', FROM:'FROM', TO:'TO', SITE:'SITE', TIME:'TIME', WHY:'WHY', NOTE:'NOTE', VOID:'VOID', BACK:'BACK', FILES:'FILES', SIZE:'SIZE' },
    a:{ void:'VOID, PUT BACK', show:'SHOW FILE', redo:'MOVE AGAIN', to:b=>'MOVE TO '+b.toUpperCase(), leave:'LEAVE IT', retry:'TRY AGAIN' },
    st:{ void:'VOID, put back', held:'! HELD, choose', wait:'in use, retrying', skip:'- left alone', fail:'X FAILED', renamed:' renamed', batch:' batch' }, tr:s=>s,
    s:{ title:'SETTINGS RECEIPT', watch:'WATCHING', folder:'Folder', change:'CHANGE', startup:'Start with Windows', delay:'Wait before sorting', closeBtn:'Close button', sort:'SORTING', preset:'Rules preset', folders:'Move folders too', learn:'Learn from my choices', look:'LOOK', paper:'Paper', layout:'Layout', lang:'Language', app:'APP', updates:'Check on startup', forget:'Forget learned choices', learned:n=>n+' saved', sure:'CONFIRM', done:'BACK TO RECEIPT', foot:'Changes save right away.', picker:'The real app opens a folder picker here.', on:'ON', off:'OFF', tray:'Keep in tray', quit:'Quit app', sec:'s',
      paper_thermal:'Thermal', paper_night:'Night', paper_green:'Green bar', paper_carbon:'Carbon', paper_book:'Ledger book', paper_national:'National Day', paper_founding:'Foundation Day', layout_list:'List', layout_depts:'Departments', lang_en:'English', lang_ar:'Arabic', Default:'Default', Developer:'Developer', Designer:'Designer', Media:'Media', Minimal:'Minimal' } },
  ar: { dir:'rtl', watching:'يراقب', paused:'واقف', receipt:'فاتورة', opened:'انفتحت', time:'الوقت', item:'الملف', size:'الحجم', voided:'ملغية', held:'تنتظر قرارك', waiting:'تنتظر', left:'ما انتقلت', failed:'ما ضبطت', total:'كل اللي انتقل', ledger:(a,b)=>'السجل: دخل '+a+'، خرج '+b, balanced:'مضبوط', check:'راجع', all:'من أول يوم', file:'ملف', files:'ملفات', none:'ما انتقل شي للحين', fresh:'فاتورة جديدة. ننتظر ملف.', pause:'وقّف', resume:'كمّل', next:'جديد', past:'اللي فاتت', settings:'الإعدادات', needs:'تحتاج قرارك', other:'غيرها', pastT:'الفواتير اللي فاتت', back:'ارجع للحالية', tray:s=>s+' في شريط المهام', open:'افتح', quit:'انقفل التطبيق ووقفت المراقبة.', conf:'نسبة التأكد', moved:'انتقلت',
    k:{ NAME:'الاسم', SAVED:'باسم', FROM:'من', TO:'إلى', SITE:'الموقع', TIME:'الوقت', WHY:'السبب', NOTE:'ملاحظة', VOID:'إلغاء', BACK:'رجع', FILES:'الملفات', SIZE:'الحجم' },
    a:{ void:'ألغِ ورجّعه', show:'ورني الملف', redo:'انقله مرة ثانية', to:b=>'نقل إلى '+b, leave:'خله مكانه', retry:'جرب مرة ثانية' },
    st:{ void:'ملغي، رجع', held:'! ينتظر قرارك', wait:'مستخدم الحين', skip:'- ما انتقل', fail:'X ما ضبط', renamed:' أُعيدت تسميته', batch:' دفعة' },
    tr:s=>{ const m={ 'disk is almost full, sorting waits':'القرص شبه ممتلي، الفرز واقف لين تفضي مساحة','cannot save records, sorting waits':'ما قدرنا نحفظ السجل، الفرز واقف','watched folder is missing':'المجلد اللي نراقبه مو موجود','file vanished before moving':'الملف اختفى قبل ما ينتقل','this is a destination folder':'هذا من مجلدات الفرز','folders stay unless enabled':'المجلدات ما تنتقل إلا لو شغّلتها','unfinished download':'التنزيل ما خلص','Windows or Office system file':'ملف نظام','hidden file':'ملف مخفي','shortcut, moving could break it':'اختصار، ممكن يخرب لو انتقل','reserved Windows name':'اسم محجوز في ويندوز','in use, will retry':'مستخدم الحين، بنجرب بعدين','may be a disguised program':'قد يكون برنامجًا متخفيًا','name and contents disagree':'الاسم لا يطابق المحتوى','you undid this source before':'لغيت ملف من هذا المصدر قبل','Windows denied access':'ويندوز ما سمح','link, moving could break it':'رابط، ممكن يخرب لو انتقل','destination is on another drive':'الوجهة في قرص ثاني','file name cannot be read':'اسم الملف ما ينقرأ','could not move, file stayed in place':'ما انتقل، والملف مكانه','this folder is blocked':'هذا المجلد ممنوع','recovered after a crash':'رجعنا سجّلناه بعد توقف مفاجئ','not moved, stayed in place':'ما انتقل وبقي مكانه','not found after a crash':'ما لقيناه بعد توقف مفاجئ','not enough space on target drive':'المساحة ما تكفي','you chose to leave it':'اخترت تخليه','still failing, nothing changed':'للحين ما ضبط، ما تغير شي','Name hides its true direction':'الاسم يخفي اتجاهه الحقيقي','Empty file, 0 bytes':'ملف فارغ','Arrived together as one batch':'وصلت مع بعض دفعة وحدة','Same contents as a file already there':'نفس ملف موجود','Looks like a program in disguise':'يبدو برنامجًا متخفيًا','No type evidence':'لا دليل على النوع','Folder move is on':'نقل المجلدات مفعّل','Office or app package':'حزمة مكتب أو تطبيق','File was deleted after moving':'حُذف الملف بعد نقله' };
      if (m[s]) return m[s];
      let r;
      if ((r = /^Extension (.+)$/.exec(s))) return 'الامتداد ' + r[1];
      if ((r = /^Contents are (.+)$/.exec(s))) return 'المحتوى ' + r[1];
      if ((r = /^Contents say (.+)$/.exec(s))) return 'المحتوى يقول ' + r[1];
      if ((r = /^Name says (.+)$/.exec(s))) return 'الاسم يقول ' + r[1];
      if ((r = /^Came from (.+)$/.exec(s))) return 'من ' + r[1];
      if ((r = /^You voided this from (.+)$/.exec(s))) return 'ألغيته من ' + r[1];
      if ((r = /^You chose this for (.+)$/.exec(s))) return 'اخترته لـ ' + r[1];
      if ((r = /^You chose (.+)$/.exec(s))) return 'اخترت ' + r[1];
      if ((r = /^(.+) already had this name$/.exec(s))) return r[1] + ' فيه الاسم نفسه';
      if ((r = /^Recreated missing folder (.+)$/.exec(s))) return 'أُعيد إنشاء ' + r[1];
      if ((r = /^Long path handled, (\d+) characters$/.exec(s))) return 'مسار طويل: ' + r[1] + ' حرفًا';
      if (s === 'Read-only flag preserved') return 'بقي للقراءة فقط';
      return s;
    },
    s:{ title:'فاتورة الإعدادات', watch:'المراقبة', folder:'المجلد', change:'غيّر', startup:'يشتغل مع ويندوز', delay:'كم ينتظر قبل الفرز', closeBtn:'زر الإغلاق', sort:'الفرز', preset:'القواعد', folders:'ينقل المجلدات بعد', learn:'يتعلم من اختياراتي', look:'الشكل', paper:'الورق', layout:'الترتيب', lang:'اللغة', app:'التطبيق', updates:'يشيّك أول ما يشتغل', forget:'انسى اللي تعلمه', learned:n=>n+' محفوظ', sure:'متأكد؟', done:'ارجع للفاتورة', foot:'التغييرات تنحفظ على طول.', picker:'يفتح التطبيق الحقيقي منتقي مجلدات هنا.', on:'شغّال', off:'طافي', tray:'خله في الشريط', quit:'قفّل التطبيق', sec:' ث',
      paper_thermal:'حراري', paper_night:'ليلي', paper_green:'شريط أخضر', paper_carbon:'كربوني', paper_book:'دفتر حسابات', paper_national:'اليوم الوطني', paper_founding:'يوم التأسيس', layout_list:'قائمة', layout_depts:'أقسام', lang_en:'English', lang_ar:'العربية', Default:'افتراضي', Developer:'مطور', Designer:'مصمم', Media:'وسائط', Minimal:'بسيط' } }
};
const X = {
  en: { share:'Share files-moved count', shareNote:'Sends only a number to n3trunner.dev. No file names and no personal data.', newVersion:'NEW VERSION', noNet:'NO CONNECTION', logs:'Logs', open:'OPEN', logsT:'LOGS RECEIPT', search:'Search name or site', showing:(a,b)=>'Showing '+a+' of '+b, more:(n,l)=>'SHOW '+n+' MORE ('+l+' LEFT)', end:'END OF LOG', noMatch:'Nothing matches this search.', clear:'CLEAR', back:'BACK TO SETTINGS', check:'Check for updates now', checkNow:'CHECK', checking:'CHECKING', latest:'UP TO DATE', last:t=>'Last checked '+t, tips:'Show tips', version:'VERSION', madeBy:'MADE BY', thanks:'THANK YOU', best:'BEST', putBack:'PUT BACK', putDone:'DONE', putNo:'FILE GONE', stats:['HELD','FAILED','WAITING','VOIDED','SKIPPED'], emptyOn:'No customers yet. The cashier is pretending to be busy.', emptyOnDo:'Download any file and it gets sorted.', emptyOff:'The cashier is on break.', emptyOffDo:'Nothing gets sorted until you press RESUME.', newItems:n=>'^ '+n+' NEW', resize:'Drag to resize',
    lv:{ MOVE:'MOVE', HOLD:'HOLD', WAIT:'WAIT', SKIP:'SKIP', FAIL:'FAIL', VOID:'VOID', SYS:'APP' },
    m:{ paused:'Watching paused', resumed:'Watching resumed', newr:'New receipt started', upd:'Checked for updates, none found', back:'Put back where it was', again:'Moved again', left:'You chose to leave it', retry:'Retry failed', started:'App started' },
    tip:{ pause:['Stops sorting new files. Files that arrive during a pause stay where they are.','Leave it running unless you are busy in this folder.'], next:['Starts a clean receipt. The current one is saved under Past. Files still waiting or held carry over.','Use it when the list gets long.'], past:['Shows receipts you finished earlier.',''], setbtn:['Opens the settings receipt.',''], min:['Shrinks the receipt to a small strip. It keeps working.',''], close:['Closes the window. The Close button setting decides if watching continues.',''], logs:['Shows the full history of everything the app did, 30 lines at a time.','Open this when something looks wrong.'], checkupd:['Checks right now for a newer version.',''], pick:['Chooses the folder the app watches.','Downloads is the best place to start.'], startup:['Starts the app when you turn on the PC.','Turn this ON if you want sorting all the time.'], delay:['How long a new file sits before it is sorted.','2 seconds suits most people. Choose 5 or 10 if you rename files right after downloading.'], closeTo:['What the close button does.','Keep in tray, so files keep sorting.'], preset:['A starting set of rules for your kind of work.','Default, unless you are sure another one fits.'], folders:['Also moves new folders, not only files.','Keep OFF. Moving folders can break projects.'], learn:['Remembers your voids and choices for each site.','Keep ON. It only stays on this PC.'], paper:['Changes the receipt paper style. Nothing else changes.','Pick your favorite.'], layout:['List shows newest first. Departments groups files by folder.','Departments is easiest to scan.'], lang:['Switches between English and Arabic.',''], updates:['Asks GitHub for the newest version each time the app starts. Nothing else is sent.','Turn ON to hear about fixes.'], forget:['Erases what the app learned from your choices. Files are not touched.','Only use this if sorting keeps going wrong.'], shareMoved:['Adds your count to the total shown on the app\'s homepage. Off unless you turn it on.','Your choice. The app works the same either way.'], tips:['Shows or hides these small tips.','Turn OFF when you know the app.'], search:['Finds lines by file name, site or action, across the whole log.',''], logmore:['Adds the next 30 older lines.',''], void:['Puts this file back where it was.','Use it when the file went to the wrong place.'], show:['Opens the file location in Windows.',''], redo:['Sorts this file again.',''], pickbin:['Moves the file to this folder and remembers it for this site.','Choose this only if you trust the file.'], leave:['Keeps the file where it is and does nothing else.','Best if you are not sure about the file.'], retry:['Tries the same move again.','Close any app using the file first.'], welcome:['Shows the first-run guide again. It moves nothing.',''], logundo:['Puts this file back where it was.','Use it when the file went to the wrong place.'], grip:['Drag up or down to change the height. The file list gets the extra room. Double-click to reset.',''] } },
  ar: { share:'شارك عدد الملفات المنقولة', shareNote:'يُرسل رقمًا فقط إلى n3trunner.dev، دون أسماء ملفات أو بيانات شخصية.', newVersion:'نسخة جديدة', noNet:'ما فيه اتصال', logs:'السجلات', open:'افتح', logsT:'فاتورة السجلات', search:'دور بالاسم أو الموقع', showing:(a,b)=>'يعرض '+a+' من '+b, more:(n,l)=>'ورني '+n+' زيادة (باقي '+l+')', end:'نهاية السجل', noMatch:'ما لقينا شي بهالبحث.', clear:'امسح', back:'ارجع للإعدادات', check:'شيّك على التحديثات الحين', checkNow:'شيّك', checking:'يشيّك', latest:'عندك آخر نسخة', last:t=>'آخر تشييك '+t, tips:'وريني التلميحات', version:'الإصدار', madeBy:'مين الحلو الي سوا البرنامج ؟؟', thanks:'يعطيك العافية', best:'الأفضل', putBack:'رجّعه', putDone:'تم', putNo:'الملف مو موجود', stats:['قرارك','ما ضبطت','انتظار','ملغية','متروكة'], emptyOn:'ما فيه زباين للحين. الكاشير يسوي نفسه مشغول.', emptyOnDo:'نزّل أي ملف ويترتب.', emptyOff:'الكاشير في استراحة.', emptyOffDo:'ما يترتب شي لين تضغط كمّل.', newItems:n=>'^ '+n+' جديد', resize:'اسحب لتغيير الارتفاع',
    lv:{ MOVE:'نقل', HOLD:'حجز', WAIT:'انتظار', SKIP:'تخطٍ', FAIL:'فشل', VOID:'إلغاء', SYS:'التطبيق' },
    m:{ paused:'المراقبة وقفت', resumed:'المراقبة كملت', newr:'بدت فاتورة جديدة', upd:'شيّكنا، ما فيه تحديثات', back:'رجع مكانه', again:'انتقل مرة ثانية', left:'اخترت تخليه', retry:'جربنا وما ضبط', started:'التطبيق اشتغل' },
    tip:{ pause:['يوقف فرز الملفات الجديدة، واللي توصل وهو واقف تبقى مكانها.','خله شغّال إلا لو كنت تشتغل في هذا المجلد.'], next:['يبدأ فاتورة جديدة ويحفظ الحالية في اللي فاتت. الملفات اللي تنتظر تنتقل معها.','استخدمه إذا طالت القائمة.'], past:['يعرض الفواتير اللي خلصتها قبل.',''], setbtn:['يفتح فاتورة الإعدادات.',''], min:['يصغّر الفاتورة لشريط صغير وتبقى شغّالة.',''], close:['يغلق النافذة. إعداد زر الإغلاق يحدد استمرار المراقبة.',''], logs:['يعرض كل اللي سواه التطبيق، 30 سطر كل مرة.','افتحه لو حسيت فيه شي غلط.'], checkupd:['يشيّك الحين لو فيه نسخة أحدث.',''], pick:['يحدد المجلد اللي يراقبه التطبيق.','مجلد التنزيلات أحسن بداية.'], startup:['يشتغل التطبيق أول ما تشغّل الجهاز.','شغّله لو تحب الفرز يكون دايم.'], delay:['كم يجلس الملف الجديد قبل ما ينفرز.','ثانيتين تناسب أغلب الناس. اختر 5 أو 10 لو تغيّر أسماء الملفات أول ما تنزل.'], closeTo:['اللي يسويه زر الإغلاق.','خله في الشريط عشان يكمل الفرز.'], preset:['قواعد جاهزة تناسب شغلك.','الافتراضي، إلا لو كنت متأكد من غيره.'], folders:['ينقل المجلدات الجديدة بعد، مو بس الملفات.','خله طافي، نقل المجلدات ممكن يخرّب المشاريع.'], learn:['يتذكر اختياراتك واللي لغيته لكل موقع.','خله شغّال، كله يبقى على جهازك بس.'], paper:['يغيّر شكل ورق الفاتورة بس.','اختر اللي يعجبك.'], layout:['القائمة تعرض الأحدث أول، والأقسام تجمع الملفات حسب المجلد.','الأقسام أسهل.'], lang:['يبدّل بين العربي والإنجليزي.',''], updates:['يسأل GitHub عن آخر نسخة كل ما اشتغل، وما يرسل شي ثاني.','شغّله عشان تعرف عن الإصلاحات.'], forget:['يمسح اللي تعلمه التطبيق من اختياراتك. الملفات ما تتأثر.','استخدمه بس لو الفرز قعد يغلط.'], shareMoved:['يضيف عددك إلى المجموع في صفحة البرنامج. طافي إلا لو شغّلته.','الخيار لك، والبرنامج يشتغل نفس الشي في الحالتين.'], tips:['يطلّع هذي التلميحات أو يخفيها.','طفّيه لما تتعود على التطبيق.'], search:['يدور في كل السجل بالاسم أو الموقع.',''], logmore:['يضيف 30 سطر أقدم.',''], void:['يرجّع هذا الملف لمكانه.','استخدمه لو الملف راح لمكان غلط.'], show:['يفتح مكان الملف في ويندوز.',''], redo:['يفرز هذا الملف مرة ثانية.',''], pickbin:['ينقل الملف إلى هذا المجلد ويتذكر ذلك لهذا الموقع.','اختره فقط إذا كنت تثق بالملف.'], leave:['يخلي الملف مكانه وما يسوي شي.','الأحسن لو مو متأكد من الملف.'], retry:['يجرب نفس النقل مرة ثانية.','قفّل أي برنامج يستخدم الملف أول.'], welcome:['يعرض دليل البداية مرة ثانية وما ينقل شي.',''], logundo:['يرجّع هذا الملف لمكانه القديم.','استخدمه لو الملف راح لمكان غلط.'], grip:['اسحب لأعلى أو لأسفل لتغيير الارتفاع. قائمة الملفات تأخذ المساحة الإضافية. انقر مرتين للإعادة.',''] } }
};
const W = {
  en: { step:(a,b)=>'STEP '+a+' OF '+b, t1:'READ BEFORE FIRST USE', does:'WHAT THIS APP DOES', doesT:'It MOVES files out of the folder you choose and into subfolders. A game or app that expects a file in its old place can stop working.', never:'WHAT IT NEVER DOES', neverT:'It never deletes a file and never overwrites one. Every move can be put back.', missing:'IF A FILE LOOKS MISSING', missingT:'It moved. Open Settings, then Logs, search its name, and void the move.', nothing:'NOTHING HAS MOVED YET.', app:'ORGANIZE FOR ME', aiBuilt:'AI tools were used to help build this app.', resp:'RESPONSIBILITY', respT:'The developer is not responsible for lost or deleted files. This app only moves files. It has no delete function.', t2:'CHOOSE A FOLDER', t2s:'Downloads is the safest choice. Never use a folder that holds games or installed apps.', ok:'OK', risk:'RISK', blocked:'BLOCKED', use:'USE IT ANYWAY', riskOk:'RISK ACCEPTED',
    why:{ games:'Blocked: this looks like a games folder. Moving these files would break your games.', root:'Blocked: this folder holds your whole drive or profile.', system:'Blocked: Windows and installed apps need these files in place.', personal:'Risk: this folder may hold shortcuts and files that other apps expect to find here.', network:'Risk: other people may use this folder, and it can disconnect.' },
    partial:'This folder is big, so only its first files are shown.', checking:'Checking the folder…', t3:'PREVIEW RECEIPT', t3s:'Your real files, checked but not touched. Files already here stay where they are. Only files that arrive after you start get sorted.', browse:'CHOOSE ANOTHER FOLDER', wouldMove:'WOULD MOVE IF NEW', leftAlone:'LEFT ALONE', heldYou:'HELD FOR YOU', deleted:'DELETED', t4:'READY TO START', folder:'FOLDER', ack:'I understand that files will move, and I accept this note', start:'START WATCHING', later:'NOT NOW, STAY PAUSED', next:'NEXT', back:'BACK', useFolder:'USE THIS FOLDER', cancel:'CANCEL',
    welcome:'Show first-run guide', _:0 },
  ar: { step:(a,b)=>'الخطوة '+a+' من '+b, t1:'اقرأ قبل أول استخدام', does:'ماذا يفعل التطبيق', doesT:'ينقل الملفات من المجلد الذي تختاره إلى مجلدات فرعية. قد تتوقف لعبة أو برنامج يتوقع وجود ملف في مكانه القديم.', never:'ما لا يفعله أبدًا', neverT:'لا يحذف أي ملف ولا يستبدله. ويمكن التراجع عن كل نقل.', missing:'إذا بدا أن ملفًا اختفى', missingT:'لقد نُقل. افتح الإعدادات ثم السجلات، وابحث عن اسمه، وألغِ النقل.', nothing:'ما انتقل شي للحين.', app:'رتب لي', aiBuilt:'استُخدم الذكاء الاصطناعي للمساعدة في بناء هذا التطبيق.', resp:'المسؤولية', respT:'المطور غير مسؤول عن فقدان الملفات أو حذفها. هذا التطبيق ينقل الملفات فقط، وليس فيه أي وظيفة للحذف.', t2:'اختار مجلد', t2s:'مجلد التنزيلات هو الخيار الأكثر أمانًا. لا تستخدم مجلدًا فيه ألعاب أو برامج مثبتة.', ok:'آمن', risk:'خطر', blocked:'ممنوع', use:'استخدمه رغم ذلك', riskOk:'قبلت الخطر',
    why:{ games:'ممنوع: يبدو أنه مجلد ألعاب، ونقل ملفاته سيعطل ألعابك.', root:'ممنوع: هذا المجلد يضم القرص أو ملفك الشخصي بالكامل.', system:'ممنوع: يحتاج ويندوز والبرامج المثبتة إلى بقاء هذه الملفات مكانها.', personal:'خطر: قد يحتوي على اختصارات وملفات تتوقعها برامج أخرى هنا.', network:'خطر: قد يستخدمه آخرون، وقد ينقطع الاتصال به.' },
    partial:'المجلد كبير، فهذي أول ملفاته بس.', checking:'جاري فحص المجلد…', t3:'فاتورة المعاينة', t3s:'هذه ملفاتك الفعلية، فُحصت دون لمسها. تبقى الملفات الموجودة هنا في مكانها، ولا يُفرز إلا ما يصل بعد البدء.', browse:'اختر مجلد ثاني', wouldMove:'بينتقل لو جديد', leftAlone:'ما بينتقل', heldYou:'تنتظر قرارك', deleted:'حُذف', t4:'جاهزين نبدأ', folder:'المجلد', ack:'أفهم أن الملفات ستُنقل، وأقبل هذه الملاحظة', start:'ابدأ المراقبة', later:'مو الحين، خله واقف', next:'التالي', back:'رجوع', useFolder:'استخدم هذا المجلد', cancel:'إلغاء',
    welcome:'ورني دليل البداية', _:0 }
};
const FEST = {
  en: { national: 'SAUDI NATIONAL DAY', founding: 'SAUDI FOUNDATION DAY', ask: { national: 'It is National Day season. Try its paper?', founding: 'It is Foundation Day season. Try its paper?' }, over: 'The occasion is over. Return to your old paper?', yes: 'TRY IT', no: 'NO THANKS', back: 'RETURN', keep: 'KEEP IT' },
  ar: { national: 'اليوم الوطني السعودي', founding: 'يوم التأسيس السعودي', ask: { national: 'هذي أيام اليوم الوطني. تجرب ورقته؟', founding: 'هذي أيام يوم التأسيس. تجرب ورقته؟' }, over: 'خلصت المناسبة. ترجع لورقك القديم؟', yes: 'جرب', no: 'لا، شكرًا', back: 'ارجع', keep: 'خله' }
};
function suggestion() {
  const now = new Date(), h = holidayFor(now), f = FEST[S.cfg.lang], key = h + now.getFullYear();
  const box = (msg, a, al, b, bl) => '<div class="suggest">' + msg + '<div class="acts"><button type="button" data-w="' + a + '">' + al + '</button><button type="button" data-w="' + b + '">' + bl + '</button></div></div>';
  if (h && S.cfg.paper !== h && S.cfg.dismissed !== key) return box(f.ask[h], 'holtry', f.yes, 'holno', f.no);
  if (!h && S.cfg.prevPaper) return box(f.over, 'holback', f.back, 'holkeep', f.keep);
  return '';
}
const OPTS = { paper:['thermal','night','green','carbon','book','national','founding'], layout:['list','depts'], lang:['en','ar'], delay:[1,2,5,10], closeTo:['tray','quit'] };
const invoke = (cmd, args) => window.__TAURI__.core.invoke(cmd, args);
const DEFAULTS = { paper:'thermal', layout:'list', lang:String(navigator.language).toLowerCase().startsWith('ar') ? 'ar' : 'en', delay:2, closeTo:'tray', startup:false, folders:false, learn:true, tips:true, updates:false, shareMoved:false, height:880, welcomed:false, folder:'' };
const S = { w: { step: 0, pick: 0, warnOk: false, ack: false, blocked: null, fromSettings: false, list: [], err: '' }, newAbove: 0, logs: [], logQ: '', logN: 30, in: 0, view: 'full', paused: true, past: false, openId: null, freshId: null, no: 1, allTime: 0, items: [], history: [], opened: new Date(), sure: false, learned: 0, problem: null, version: '', choices: [], cfg: Object.assign({}, DEFAULTS) };
const folderName = () => String(S.cfg.folder).replace(/\\+$/, '').split('\\').pop();
const hydrate = e => Object.assign(e, { at: new Date(e.at), f: {} });
function applyState(s) {
  const known = new Set(S.items.map(i => i.id)), fresh = s.items.filter(i => !known.has(i.id));
  const roll = app.querySelector('.roll');
  if (fresh.length && known.size) { if (roll && (roll.scrollTop > 1 || S.openId)) S.newAbove += fresh.length; else S.freshId = fresh[0].id; }
  S.items = s.items.map(hydrate); S.in = s.seen; S.no = s.no; S.opened = new Date(s.opened); S.allTime = s.allTime; S.history = s.history; S.paused = s.paused; S.learned = s.learned; S.problem = s.problem;
  const keep = { lang: S.cfg.lang };
  S.cfg = Object.assign({}, DEFAULTS, keep, s.cfg);
}
const GUIDE_H = 780;
function welcomePanel() {
  const x = W[S.cfg.lang], w = S.w, cur = w.list[w.pick];
  const btn = (a, label, off) => '<button type="button" data-w="' + a + '"' + (off ? ' disabled' : '') + '>' + label + '</button>';
  const page = (t, body, left, right) => '<div class="panel" data-panel="welcome"><div class="wlang">' + [['en', 'English'], ['ar', 'العربية']].map(([l, n]) => '<button type="button" data-w="lang" data-l="' + l + '" lang="' + l + '" aria-pressed="' + (S.cfg.lang === l) + '"' + (S.cfg.lang === l ? ' class="on"' : '') + '>' + n + '</button>').join('') + '</div><div class="dim wstep">' + (w.fromSettings ? '' : x.step(ltr(w.step + 1), ltr(4))) + '</div><h2>' + t + '</h2><div class="wbody">' + body + '</div><div class="wnav">' + left + right + '</div></div>';
  if (w.step === 0) return page(x.t1, '<div class="wsec"><h3>' + x.does + '</h3><div class="note ink">' + x.doesT + '</div></div><div class="wsec"><h3>' + x.never + '</h3><div class="note ink">' + x.neverT + '</div></div><div class="wsec"><h3>' + x.missing + '</h3><div class="note ink">' + x.missingT + '</div></div><div class="wsec"><h3>' + x.resp + '</h3><div class="note ink">' + x.respT + '</div></div>' + (w.review ? '' : '<div class="risk c">' + x.nothing + '</div>'), w.review ? btn('cancel', x.back) : btn('back', x.back, true), btn('next', x.next));
  if (w.step === 1) {
    const shown = w.blocked !== null ? w.blocked : cur, ready = cur && cur.level !== 'block' && (cur.level === 'ok' || w.warnOk);
    const rows = w.list.map((p, i) => '<button type="button" class="set wf ' + p.level + (i === w.pick ? ' on' : '') + '" data-w="f" data-i="' + i + '"><span><bdi dir="ltr">' + (i === w.pick ? '&gt; ' : '') + html(p.path) + '</bdi></span><b>[ ' + x[p.level === 'ok' ? 'ok' : p.level === 'warn' ? 'risk' : 'blocked'] + ' ]</b></button>').join('');
    const msg = shown && shown.why ? '<div class="risk">' + x.why[shown.why] + (w.blocked === null && cur.level === 'warn' ? (w.warnOk ? '<div class="b">' + x.riskOk + '</div>' : '<div class="acts"><button type="button" data-w="warnok">' + x.use + '</button></div>') : '') + '</div>' : '';
    return page(x.t2, (w.busy ? '<div class="note ink b">' + x.checking + '</div>' : '') + '<div class="note ink">' + x.t2s + '</div><div class="wsec">' + rows + '</div><div class="acts"><button type="button" data-w="browse">' + x.browse + '</button></div>' + msg + (w.err ? '<div class="risk">' + html(L().tr(w.err)) + '</div>' : ''), w.fromSettings ? btn('cancel', x.cancel) : btn('back', x.back), btn('next', w.fromSettings ? x.useFolder : x.next, !ready));
  }
  if (w.step === 2) {
    const p = w.preview || { rows: [], moved: 0, skip: 0, held: 0 };
    const rows = p.rows.map(hydrate);
    return page(x.t3, '<div class="note ink">' + x.t3s + '</div>' + (p.partial ? '<div class="note">' + x.partial + '</div>' : '') + '<div class="wsec"><div class="logcount"></div>' + rows.map(e => '<div class="logrow ' + e.status + '"><span class="line"><span class="tm"></span><bdi>' + html(fit(displayName(e), nameCols())) + '</bdi></span><span class="l2"><span>' + (e.status === 'skip' ? '- ' + html(L().tr(e.reason)) : leftOf(e)) + '</span></span></div>').join('') + '</div><div class="sec">' + kv(x.wouldMove, ltr(p.moved)) + kv(x.leftAlone, ltr(p.skip)) + kv(x.heldYou, ltr(p.held)) + kv(x.deleted, ltr(0), 'b') + '</div>', btn('back', x.back), btn('next', x.next));
  }
  return page(x.t4, (w.busy ? '<div class="note ink b">' + x.checking + '</div>' : '') + '<div class="wsec">' + kv(x.folder, '') + '<div class="note ink"><bdi dir="ltr">' + html(cur.path) + '</bdi></div></div>' + (cur.level === 'warn' ? '<div class="risk">' + x.why[cur.why] + '</div>' : '') + (w.err ? '<div class="risk">' + html(L().tr(w.err)) + '</div>' : '') + '<button type="button" class="set" data-w="ack"><span>' + x.ack + '</span><b>[ ' + (w.ack ? 'X' : '&nbsp;') + ' ]</b></button><div class="note ink">' + x.respT + '</div>' + (w.review ? '' : '<div class="acts wlater"><button type="button" data-w="later">' + x.later + '</button></div>'), btn('back', x.back), btn('start', x.start, !w.ack));
}
const isGroup = e => e.isDir || e.count > 1;
function displayName(e) {
  const n = e.saved || e.shown;
  if (!isGroup(e)) return n;
  const x = extOf(n);
  return (x && !e.isDir ? n.slice(0, -(x.length + 1)) : n) + '/';
}
const LX = () => X[S.cfg.lang];
const nameCols = () => (S.cfg.lang === 'ar' ? COLS - 18 : COLS - 9);
function logText(l) { const x = LX(); return l.level === 'SYS' ? (x.m[l.msg] || l.msg) : (x.m[l.msg] || L().tr(l.msg)); }
function logAction(l, x) {
  if (l.level !== 'MOVE' || !l.seq) return '<span></span>';
  if (l.failed) return '<span class="logno">' + x.putNo + '</span>';
  return '<button type="button" class="logput" data-w="logundo" data-l="' + l.id + '" data-tip="logundo"' + (l.undone ? ' disabled' : '') + '>' + (l.undone ? x.putDone : x.putBack) + '</button>';
}
function logBody() {
  const x = LX(), q = S.logQ.trim().toLowerCase();
  const hits = q ? S.logs.filter(l => [l.name, l.msg, logText(l), l.level, x.lv[l.level]].join(' ').toLowerCase().includes(q)) : S.logs;
  const shown = hits.slice(0, S.logN), all = hits.length, left = all - shown.length;
  let h = '<div class="dim logcount">' + x.showing(ltr(count(shown.length)), ltr(count(all))) + '</div>';
  if (!hits.length) return h + '<div class="empty">' + x.noMatch + '</div>';
  h += shown.map(l => '<div class="logrow ' + l.level.toLowerCase() + (S.logOpen === l.id ? ' open' : '') + '"><button type="button" class="logname" data-w="logopen" data-l="' + l.id + '" aria-expanded="' + (S.logOpen === l.id) + '"><span class="line"><span class="tm">' + clock(l.at) + '</span><bdi>' + html(l.name ? fit(l.name, nameCols()) : x.lv.SYS) + '</bdi></span></button><span class="l2 lg"><span class="dim">' + ltr(l.at.toLocaleDateString('en-GB').slice(0, 5)) + '</span><span>' + (l.level === 'SYS' ? '' : '<b>' + x.lv[l.level] + '</b> ') + (l.msg.endsWith('/') ? '<bdi dir="ltr">' + html(l.msg) + '</bdi>' : html(logText(l))) + '</span>' + '</span>' + (S.logOpen === l.id ? '<div class="more logfull">' + (l.name ? pair('NAME', l.name) : '') + (l.msg.endsWith('/') ? pair('TO', '...\\' + folderName() + '\\' + l.msg.slice(0, -1) + '\\', 1) : '') + pair('TIME', l.at.toLocaleDateString('en-GB') + ' ' + clock(l.at), 1) + logAction(l, x) + '</div>' : '') + '</div>').join('');
  return h + (left ? '<div class="acts"><button type="button" id="logmore" data-tip="logmore">' + x.more(ltr(Math.min(30, left)), ltr(count(left))) + '</button></div>' : '<div class="note endlog">' + x.end + '</div>');
}
function logsPanel() {
  const x = LX();
  return '<div class="panel" data-panel="logs"><h2>' + x.logsT + '</h2><div class="search" data-tip="search"><span aria-hidden="true">&gt;</span><input id="logq" type="text" dir="auto" autocomplete="off" spellcheck="false" aria-label="' + x.search + '" placeholder="' + x.search + '" value="' + html(S.logQ).replace(/"/g, '&quot;') + '">' + '<button type="button" id="logclear"' + (S.logQ ? '' : ' class="off"') + '>' + x.clear + '</button></div><div id="loglist">' + logBody() + '</div><div class="acts"><button type="button" id="logback">' + x.back + '</button></div></div>';
}
function persist() { invoke('save_config', { cfg: S.cfg }).catch(() => {}); }
const L = () => T[S.cfg.lang];
const clock = d => d.toLocaleTimeString('en-GB');
function html(s) { return String(s).replace(/&/g, '&amp;').replace(/</g, '&lt;'); }
const ltr = s => '<bdi class="num" dir="ltr">' + html(s) + '</bdi>';
function pair(k, v, isPath) { return '<p><b>' + L().k[k] + '</b><bdi' + (isPath ? ' dir="ltr"' : '') + '>' + html(v) + '</bdi></p>'; }
function kv(a, b, cls) { return '<div class="kv' + (cls ? ' ' + cls : '') + '"><span>' + a + '</span><span>' + b + '</span></div>'; }
function details(e) {
  const t = L();
  let h = '<div class="more">' + pair('NAME', isGroup(e) ? displayName(e) : e.shown);
  if (e.count > 1 || e.f.count) h += pair('FILES', count(e.count), 1) + pair('SIZE', size(e.bytes), 1);
  if (e.saved) h += pair('SAVED', e.saved);
  if (e.status === 'moved' || e.status === 'void') h += pair('FROM', '...\\' + folderName() + '\\', 1) + pair('TO', '...\\' + folderName() + '\\' + e.bin + '\\', 1);
  if (e.host) h += pair('SITE', e.host, 1);
  h += pair('TIME', e.at.toLocaleDateString('en-GB') + ' ' + clock(e.at), 1);
  if (e.reason) h += pair('WHY', t.tr(e.reason));
  e.notes.forEach(n => { h += pair('NOTE', t.tr(n)); });
  if (e.undoFail) h += pair('VOID', t.tr(e.undoFail));
  if (e.backAs && e.status === 'void') h += pair('BACK', e.backAs);
  if (e.ev.length) h += '<div class="ev">' + e.ev.map(([x, p]) => kv('<span class="dim">' + html(t.tr(x)) + '</span>', ltr(p > 0 ? '+' + p : p))).join('') + kv('<b>' + t.conf + '</b>', '<b>' + ltr(e.conf + '%') + '</b>') + '</div>';
  const btn = (a, x, bin) => '<button type="button" data-tip="' + (a === 'pick' ? (bin === 'random' ? 'pickbin' : 'pickbin') : a) + '" data-a="' + a + '" data-id="' + e.id + '"' + (bin ? ' data-bin="' + bin + '"' : '') + '>' + html(x) + '</button>';
  let acts = '';
  if (e.status === 'moved') acts = btn('void', t.a.void) + btn('show', t.a.show);
  if (e.status === 'void') acts = btn('redo', t.a.redo);
  if (e.status === 'held') acts = btn('pick', t.a.to(e.bin), e.bin) + btn('pick', t.a.to('random'), 'random') + btn('leave', t.a.leave);
  if (e.status === 'fail') acts = btn('retry', t.a.retry);
  return h + (acts ? '<div class="acts">' + acts + '</div>' : '') + '</div>';
}
function leftOf(e) {
  const t = L().st;
  if (e.status === 'moved') return '> <bdi dir="ltr">' + e.bin + '/</bdi>' + (e.saved ? t.renamed : '') + (e.count > 1 ? t.batch : '');
  return html(t[e.status]);
}
function item(e) {
  const open = S.openId === e.id, t = L();
  const sz = e.isDir && !e.f.count ? (S.cfg.lang === 'ar' ? 'مجلد' : 'folder') : isGroup(e) ? count(e.count) + ' ' + (e.count === 1 ? t.file : t.files) : size(e.bytes);
  return '<div class="item ' + e.status + (S.freshId === e.id && !reduce ? ' new' : '') + '"><button type="button" data-a="open" data-id="' + e.id + '" aria-expanded="' + open + '">' +
    '<span class="line"><span class="tm">' + clock(e.at) + '</span><bdi>' + html(displayName(e).endsWith('/') ? fit(displayName(e).slice(0, -1), nameCols() - 1) + '/' : fit(displayName(e), nameCols())) + '</bdi></span>' +
    '<span class="l2"><span>' + leftOf(e) + '</span><span>' + (isGroup(e) ? html(sz) : ltr(sz)) + '</span></span></button>' + (open ? details(e) : '') + '</div>';
}
function rollHtml() {
  const t = L();
  if (!S.items.length) { const x = LX(); return '<div class="empty"><pre class="face" dir="ltr">' + (S.paused ? '[ -_- ] zzz' : '[ o_o ]') + '</pre><div class="joke">' + (S.paused ? x.emptyOff : x.emptyOn) + '</div><div>' + (S.paused ? x.emptyOffDo : x.emptyOnDo) + '</div></div>'; }
  if (S.cfg.layout !== 'depts') return S.items.map(item).join('');
  const section = (name, list) => list.length ? '<div class="group"><span>' + name + '</span><span>' + count(list.reduce((x, i) => x + i.count, 0)) + '</span></div>' + list.map(item).join('') : '';
  const needs = S.items.filter(i => carry(i)), others = S.items.filter(i => i.status === 'skip' || i.status === 'fail');
  const done = S.items.filter(i => i.status === 'moved' || i.status === 'void');
  return section(t.needs, needs) + [...EXTBINS].map(b => section('<bdi dir="ltr">' + b.toUpperCase() + '/</bdi>', done.filter(i => i.bin === b))).join('') + section(t.other, others);
}
const filesLabel = (n, s) => ltr(count(n)) + ' ' + (n === 1 ? L().file : L().files) + ' / ' + ltr(size(s));
function setRow(label, value, key) { return '<button type="button" class="set" data-tip="' + key + '" data-set="' + key + '"><span>' + label + '</span><b>[ ' + value + ' ]</b></button>'; }
/* Sharing: if the user turns it on, sends how many files have moved since the last report, as one number and nothing else. */
const MOVED_URL = 'https://n3trunner.dev/organizeforme/moved';
const SHARE_EVERY_MS = 15 * 60 * 1000;
const movedTotal = () => S.allTime + tallyOf(S.items).moved;
async function shareMoved() {
  if (!S.cfg.shareMoved) return;
  const unsent = movedTotal() - (S.cfg.sharedTotal || 0);
  if (unsent <= 0) return;
  try {
    // Sent as plain text, so no extra permission request goes first. Counted only once the server says so.
    const r = await fetch(MOVED_URL, { method: 'POST', headers: { 'Content-Type': 'text/plain' }, body: JSON.stringify({ moved: unsent }) });
    if (!r.ok) return;
    S.cfg.sharedTotal = (S.cfg.sharedTotal || 0) + (await r.json()).accepted;
    persist();
  } catch (e) { /* No connection: the number waits for the next try. */ }
}

/* Updates: asks GitHub for the latest release and only ever opens its page. Nothing downloads by itself. */
const RELEASES_API = 'https://api.github.com/repos/TR4IS/organize-for-me/releases?per_page=1';
function isNewer(tag, current) {
  const parse = v => { const [main, pre] = String(v).replace(/^v/, '').split('-'); return { n: main.split('.').map(Number), pre }; };
  const a = parse(tag), b = parse(current);
  for (let i = 0; i < 3; i++) { const d = (a.n[i] || 0) - (b.n[i] || 0); if (d) return d > 0; }
  return !a.pre && !!b.pre;
}
function updateLabel() {
  const x = LX(), u = S.upd;
  return !u ? x.checkNow : u.state === 'checking' ? x.checking : u.state === 'latest' ? x.latest : u.state === 'new' ? x.newVersion : x.noNet;
}
async function checkUpdate() {
  S.upd = { state: 'checking' };
  if (S.past === 'settings') draw();
  try {
    const r = await fetch(RELEASES_API, { headers: { Accept: 'application/vnd.github+json' } });
    if (!r.ok) throw new Error(String(r.status));
    const tag = (await r.json())[0].tag_name;
    S.upd = isNewer(tag, S.version) ? { state: 'new', tag } : { state: 'latest' };
  } catch (e) { S.upd = { state: 'fail' }; }
  draw();
}
function settings() {
  const x = LX(), t = L().s, c = S.cfg, bool = v => v ? t.on : t.off;
  return '<div class="panel" data-panel="settings"><h2>' + t.title + '</h2>' +
    '<h3>' + t.watch + '</h3>' +
    '<div class="note"><bdi dir="ltr">' + html(c.folder) + '</bdi></div>' + setRow(t.folder, t.change, 'pick') +
    setRow(t.startup, bool(c.startup), 'startup') + setRow(t.delay, c.delay + t.sec, 'delay') + setRow(t.closeBtn, c.closeTo === 'tray' ? t.tray : t.quit, 'closeTo') +
    '<h3>' + t.sort + '</h3>' + setRow(t.folders, bool(c.folders), 'folders') + setRow(t.learn, bool(c.learn), 'learn') +
    '<h3>' + t.look + '</h3>' + setRow(t.paper, t['paper_' + c.paper], 'paper') + setRow(t.layout, t['layout_' + c.layout], 'layout') + setRow(t.lang, t['lang_' + c.lang], 'lang') +
    '<h3>' + t.app + '</h3>' + setRow(x.logs, x.open, 'logs') + setRow(W[c.lang].welcome, x.open, 'welcome') + setRow(x.tips, bool(c.tips), 'tips') + setRow(t.updates, bool(c.updates), 'updates') + setRow(x.check, updateLabel(), 'checkupd') + setRow(x.share, bool(c.shareMoved), 'shareMoved') + '<div class="note">' + x.shareNote + '</div>' + setRow(t.forget, S.sure ? t.sure : t.learned(S.learned), 'forget') +
    '<div class="status" aria-live="polite">' + t.foot + '</div>' +
    '<div class="sign">' + [[x.version, ltr(S.version)], [x.madeBy, '<a href="#" id="repo" dir="ltr">TR4IS</a>']].map(([a, b]) => '<div class="lead"><span>' + a + '</span><span class="dots">' + '.'.repeat(60) + '</span><span class="signv">' + b + '</span></div>').join('') + '<div class="thanks"><bdi dir="ltr">***</bdi> ' + x.thanks + ' <bdi dir="ltr">***</bdi></div><div class="origin"><span lang="ar" dir="rtl">منتج سعودي</span>' + (S.cfg.lang === 'ar' ? '' : '<span>SAUDI PRODUCT</span>') + '</div><div class="note c">' + W[c.lang].respT + '</div><div class="note c">' + W[c.lang].aiBuilt + '</div></div>' +
    '<div class="acts" style="margin-top:auto"><button type="button" id="pastclose">' + t.done + '</button></div></div>';
}
function draw() {
  if (S.composing || S.dragging) return;
  const gliding = glide ? { to: glide.target, from: glide.el.scrollTop, panel: glide.el.classList.contains('panel') } : null;
  stopGlide();
  const tipKeep = tipFor ? { key: tipKey(tipFor), shown: !!app.querySelector('.tipslip') } : null;
  hideTip();
  const qEl = document.getElementById('logq'), caret = qEl && document.activeElement === qEl ? qEl.selectionStart : null;
  const roll = app.querySelector('.roll'), keep = roll ? roll.scrollTop : 0;
  let anchor = null;
  if (roll && (roll.scrollTop > 1 || S.openId)) {
    const top = roll.getBoundingClientRect().top;
    const it = [...roll.querySelectorAll('.item')].find(i => i.getBoundingClientRect().bottom > top + 1 && i.querySelector('[data-a="open"]').dataset.id !== String(S.skipAnchor));
    if (it) anchor = { id: it.querySelector('[data-a="open"]').dataset.id, off: it.getBoundingClientRect().top - top };
  }
  const panelEl = app.querySelector('.panel'), keepP = panelEl ? panelEl.scrollTop : 0;
  const a = document.activeElement, fk = a && a.dataset && a.dataset.set ? 'set' + a.dataset.set : a && a.dataset && a.dataset.a ? a.dataset.a + a.dataset.id : a && a.id;
  const t = L(), st = tallyOf(S.items), moved = S.items.filter(i => i.status === 'moved');
  const bins = [...EXTBINS].map(b => { const g = moved.filter(i => i.bin === b); return [b, g.reduce((x, i) => x + i.count, 0), g.reduce((x, i) => x + i.bytes, 0)]; });
  const totalB = moved.reduce((x, i) => x + i.bytes, 0);
  const ctl = '<div class="ctl"><button type="button" id="min" data-tip="min" aria-label="Minimize">_</button><button type="button" id="close" data-tip="close" class="x" aria-label="Close">x</button></div>';
  const need = st.held ? ' ! ' + count(st.held) : '';
  const state = S.paused ? t.paused : t.watching, cls = S.cfg.paper === 'thermal' ? '' : S.cfg.paper;
  const shell = (extra, inner) => '<div class="win ' + cls + extra + (S.past === 'welcome' ? ' guide' : '') + '" dir="' + t.dir + '" lang="' + S.cfg.lang + '"' + (extra ? ' style="height:' + (S.past === 'welcome' ? (S.guideH || GUIDE_H) : Math.min(S.cfg.height, screen.availHeight)) + 'px"' : '') + '><div class="edge t"></div>' + inner + (extra ? '<div class="grip" id="grip" data-tip="grip" role="separator" aria-orientation="horizontal" aria-label="' + LX().resize + '" tabindex="0"></div>' : '') + '<div class="edge"></div></div>';
  if (S.view === 'min') {
    app.innerHTML = shell('', '<div class="bar"><span>' + W[S.cfg.lang].app + '</span><div class="ctl"><button type="button" id="restore" aria-label="Restore">^</button><button type="button" id="close" class="x" aria-label="Close">x</button></div></div><div class="mini">' + kv(state, filesLabel(st.moved, totalB), 'b') + '<div class="minilast">' + (S.items[0] ? '<span class="line"><span class="tm">' + clock(S.items[0].at) + '</span><bdi>' + html(fit(displayName(S.items[0]), nameCols())) + '</bdi></span><span class="l2 ' + S.items[0].status + '"><span>' + leftOf(S.items[0]) + '</span></span>' : '<span class="dim">' + LX().emptyOn + '</span>') + '</div><div class="minic"><span' + (st.held ? ' class="b"' : '') + '>' + t.held + ' ' + ltr(count(st.held)) + '</span><span>' + t.waiting + ' ' + ltr(count(st.wait)) + '</span><span' + (st.fail ? ' class="b"' : '') + '>' + t.failed + ' ' + ltr(count(st.fail)) + '</span></div></div>');
  } else {
    const inner = '<div class="bar"><span>' + W[S.cfg.lang].app + '</span>' + ctl + '</div>' +
      '<div class="head">' + (FEST.en[S.cfg.paper] ? '<div class="fest">' + FEST[S.cfg.lang][S.cfg.paper] + '</div>' : '') + '<div class="path" dir="rtl"><bdi dir="ltr">' + html(S.cfg.folder) + '</bdi></div><div class="dim">' + kv(t.receipt + ' ' + ltr('#' + String(S.no).padStart(6, '0')), ltr(S.opened.toLocaleDateString('en-GB'))) + kv(t.opened + ' ' + ltr(clock(S.opened)), state) + '</div></div>' +
      (S.upd && S.upd.state === 'new' ? '<button type="button" class="set" id="updnew"><span>' + LX().newVersion + ' <bdi dir="ltr">' + html(S.upd.tag) + '</bdi></span><b>[ ' + LX().open + ' ]</b></button>' : '') +
      (S.problem ? '<div class="problem">' + html(t.tr(S.problem)) + '</div>' : '') + suggestion() + '<div class="cols"><span>' + t.time + '</span><span>' + t.item + '</span><span>' + t.size + '</span></div>' +
      '<div class="rollwrap"><div class="roll" tabindex="-1">' + rollHtml() + '</div><div class="sb" aria-hidden="true"><i></i></div>' + (S.newAbove ? '<button type="button" id="newtop" class="newpill">' + LX().newItems(S.newAbove) + '</button>' : '') + '</div>' +
      '<div class="sum"><div style="--numw:' + Math.max(...bins.map(x => count(x[1]).length)) + '">' + (bins.length ? bins.map(([b, n, s]) => '<div class="lead' + (n ? '' : ' zero') + '"><bdi dir="ltr">' + b + '</bdi><span class="dots">' + '.'.repeat(40) + '</span><span class="cnt">' + ltr(count(n)) + '<span class="w">' + (n === 1 ? t.file : t.files) + '</span></span><span>' + ltr(n ? size(s) : '-') + '</span></div>').join('') : '<span class="dim">' + t.none + '</span>') + '</div>' +
      '<div class="sec stats">' + [[st.held, 1], [st.fail, 1], [st.wait, 0], [st.void, 0], [st.skip, 0]].map(([n, alert], i) => '<div class="stat' + (!n ? ' zero' : alert ? ' alert' : '') + '"><b>' + ltr(count(n)) + '</b><span>' + LX().stats[i] + '</span></div>').join('') + '</div>' +
      '<div class="sec">' + kv(t.total, filesLabel(st.moved, totalB), 'b') + kv(t.ledger(ltr(count(S.in)), ltr(count(st.out))), S.in === st.out ? t.balanced : t.check) + '<div class="dim">' + kv(t.all, ltr(count(S.allTime + st.moved)) + ' ' + t.files) + '</div></div></div>' +
      '<div class="tools"><button type="button" id="pause" data-tip="pause">' + (S.paused ? t.resume : t.pause) + '</button><button type="button" id="next" data-tip="next">' + t.next + '</button><button type="button" id="pastbtn" data-tip="past">' + t.past + '</button><button type="button" id="setbtn" data-tip="setbtn">' + t.settings + '</button></div>' +
      (S.past === 'settings' ? settings() : S.past === 'welcome' ? welcomePanel() : S.past === 'logs' ? logsPanel() : S.past ? '<div class="panel"><h2>' + t.pastT + '</h2>' + S.history.map(h => kv(ltr('#' + String(h.no).padStart(6, '0')), filesLabel(h.moved, h.bytes))).join('') + '<div class="acts"><button type="button" id="pastclose">' + t.back + '</button></div></div>' : '');
    app.innerHTML = shell(' full', inner);
    const r = app.querySelector('.roll');
    r.scrollTop = keep;
    S.skipAnchor = null;
    if (anchor) {
      const b = [...r.querySelectorAll('[data-a="open"]')].find(x => x.dataset.id === anchor.id), el = b && b.closest('.item');
      if (el) r.scrollTop += el.getBoundingClientRect().top - r.getBoundingClientRect().top - anchor.off;
    }
    const sync = () => { const i = app.querySelector('.sb i'), sb = i.parentNode, on = r.scrollHeight > r.clientHeight + 1; sb.hidden = !on; if (on) { const h = sb.clientHeight, th = Math.max(24, h * r.clientHeight / r.scrollHeight); i.style.height = th + 'px'; i.style.top = (h - th) * r.scrollTop / (r.scrollHeight - r.clientHeight) + 'px'; } };
    r.addEventListener('scroll', () => {
      sync();
      if (r.scrollTop <= 1 && S.newAbove) { S.newAbove = 0; const pill = document.getElementById('newtop'); if (pill) pill.remove(); }
    });
    sync();
    if (r.scrollTop <= 1) { S.newAbove = 0; const pill = document.getElementById('newtop'); if (pill) pill.remove(); }
    const p = app.querySelector('.panel');
    if (p) {
      p.scrollTop = keepP;
      p.insertAdjacentHTML('afterend', '<div class="sb psb" aria-hidden="true"><i></i></div>');
      const bar = p.nextElementSibling, thumb = bar.firstChild;
      const fit = () => { const on = p.scrollHeight > p.clientHeight + 1; bar.hidden = !on; if (on) { const h = bar.clientHeight, th = Math.max(24, h * p.clientHeight / p.scrollHeight); thumb.style.height = th + 'px'; thumb.style.top = (h - th) * p.scrollTop / (p.scrollHeight - p.clientHeight) + 'px'; } };
      p.addEventListener('scroll', fit);
      new MutationObserver(fit).observe(p, { childList: true, subtree: true });
      new ResizeObserver(fit).observe(p);
      fit();
    }
  }
  S.freshId = null;
  if (fk) { const el = document.getElementById(fk) || [...app.querySelectorAll('[data-a],[data-set]')].find(x => (x.dataset.set ? 'set' + x.dataset.set : x.dataset.a + x.dataset.id) === fk); if (el) el.focus(); }
  if (caret !== null) { const q = document.getElementById('logq'); if (q) { q.focus(); q.setSelectionRange(caret, caret); } }
  const guideWin = app.querySelector('.win.guide');
  if (guideWin) {
    const edge = parseFloat(getComputedStyle(guideWin).getPropertyValue('--edge')) || 0;
    guideWin.style.transition = 'none';
    guideWin.style.height = '1px';
    const end = app.querySelector('.panel').lastElementChild.getBoundingClientRect().bottom - guideWin.getBoundingClientRect().top;
    guideWin.style.height = (S.guideH || GUIDE_H) + 'px'; void guideWin.offsetHeight;
    // The first draw opens at its size. Only later steps animate from the old height.
    if (S.guideH) guideWin.style.transition = '';
    const to = Math.ceil(end + 12 + edge);
    if (S.guideH && S.guideH !== to && !reduce) { guideWin.style.height = S.guideH + 'px'; void guideWin.offsetHeight; }
    guideWin.style.height = to + 'px';
    if (!S.guideH) { void guideWin.offsetHeight; guideWin.style.transition = ''; }
    S.guideH = to;
  } else S.guideH = null;
  if (gliding) { const el = app.querySelector(gliding.panel ? '.panel' : '.roll'); if (el) startGlide(el, gliding.to + el.scrollTop - gliding.from); }
  if (tipKeep) {
    const t = [...app.querySelectorAll('[data-tip]')].find(x => tipKey(x) === tipKeep.key);
    if (t) { clearTimeout(tipTimer); tipFor = t; if (tipKeep.shown) showTip(t); else tipTimer = setTimeout(() => showTip(t), 350); }
  }
}
async function openWizard(step, fromSettings) {
  const list = S.choices.slice();
  if (S.cfg.folder && !list.some(c => c.path === S.cfg.folder)) list.unshift({ path: S.cfg.folder, level: 'ok', why: '' });
  let pick = Math.max(0, list.findIndex(c => c.path === S.cfg.folder));
  if (!S.cfg.folder) { const safe = list.findIndex(c => c.level === 'ok'); if (safe >= 0) pick = safe; }
  S.w = { step, pick, warnOk: !!S.cfg.folder && list[pick] && list[pick].level === 'warn', ack: false, blocked: null, fromSettings, list, err: '' };
  S.past = 'welcome';
}
async function refreshLogs() { const failed = new Set(S.logs.filter(l => l.failed).map(l => l.id)); S.logs = (await invoke('get_logs')).map(l => Object.assign(l, { at: new Date(l.at), failed: failed.has(l.id) })); }
app.addEventListener('click', async ev => {
  const link = ev.target.closest('#repo');
  if (link) { ev.preventDefault(); invoke('open_repo'); return; }
  const b = ev.target.closest('button') || (String(getSelection()) ? null : ev.target.closest('.item')?.querySelector('[data-a="open"]') || ev.target.closest('.logrow')?.querySelector('.logname')); if (!b) return;
  hideTip();
  const id = b.id, key = b.dataset.set;
  const wa = b.dataset.w;
  if (wa) {
    const w = S.w;
    if (wa === 'logopen') { S.logOpen = S.logOpen === +b.dataset.l ? null : +b.dataset.l; document.getElementById('loglist').innerHTML = logBody(); return; }
    if (wa === 'logundo') {
      const l = S.logs.find(i => i.id === +b.dataset.l), p = app.querySelector('.panel'), was = p.scrollHeight, top = p.scrollTop;
      if (l && !(await invoke('undo_log', { logId: l.id }))) l.failed = true;
      await refreshLogs(); S.logN++;
      document.getElementById('loglist').innerHTML = logBody();
      if (top > 1) p.scrollTop = top + p.scrollHeight - was;
      applyState((await invoke('load')).state);
      return;
    }
    if (wa === 'lang') { S.cfg.lang = b.dataset.l; persist(); }
    if (wa === 'holtry') { S.cfg.prevPaper = S.cfg.paper; S.cfg.paper = holidayFor(new Date()); persist(); }
    if (wa === 'holno') { const n = new Date(); S.cfg.dismissed = holidayFor(n) + n.getFullYear(); persist(); }
    if (wa === 'holback') { S.cfg.paper = S.cfg.prevPaper; S.cfg.prevPaper = null; persist(); }
    if (wa === 'holkeep') { S.cfg.prevPaper = null; persist(); }
    if (w.busy) return;
    const busy = async work => { w.busy = true; draw(); try { return await work(); } finally { w.busy = false; } };
    if (wa === 'next') {
      w.err = '';
      if (w.fromSettings) {
        try { applyState(await busy(() => invoke('choose_folder', { path: w.list[w.pick].path, riskAccepted: w.warnOk }))); S.past = 'settings'; }
        catch (e) { w.err = String(e); }
      } else {
        if (w.step === 1) w.preview = await busy(() => invoke('preview', { path: w.list[w.pick].path }));
        w.step++;
      }
    }
    if (wa === 'back') w.step--;
    if (wa === 'cancel') S.past = 'settings';
    if (wa === 'f') { const c = w.list[+b.dataset.i]; if (c.level === 'block') w.blocked = c; else { w.pick = +b.dataset.i; w.blocked = null; w.warnOk = false; w.ack = false; w.err = ''; } }
    if (wa === 'browse') {
      const c = await invoke('pick_folder');
      if (c) { w.err = ''; if (c.level === 'block') w.blocked = c; else { let i = w.list.findIndex(x => x.path === c.path); if (i < 0) { w.list.push(c); i = w.list.length - 1; } w.pick = i; w.blocked = null; w.warnOk = false; w.ack = false; } }
    }
    if (wa === 'warnok') w.warnOk = true;
    if (wa === 'ack') w.ack = !w.ack;
    if (wa === 'start' || wa === 'later') {
      try {
        await invoke('save_config', { cfg: S.cfg });
        applyState(await busy(() => invoke('start_watching', { path: w.list[w.pick].path, riskAccepted: w.warnOk, paused: wa === 'later' })));
        S.past = false;
      } catch (e) { w.err = String(e); }
    }
    draw(); return;
  }
  if (key === 'welcome' || key === 'pick') { await openWizard(key === 'pick' ? 1 : 0, key === 'pick'); if (key === 'welcome') S.w.review = true; draw(); return; }
  if (id === 'newtop') { app.querySelector('.roll').scrollTop = 0; S.newAbove = 0; b.remove(); return; }
  if (id === 'updnew') { invoke('open_releases'); return; }
  if (key === 'updates') { S.cfg.updates = !S.cfg.updates; persist(); draw(); return; }
  if (key === 'shareMoved') { S.cfg.shareMoved = !S.cfg.shareMoved; persist(); draw(); shareMoved(); return; }
  if (key === 'checkupd') { if (S.upd && S.upd.state === 'new') invoke('open_releases'); else checkUpdate(); return; }
  if (key === 'logs') { await refreshLogs(); S.past = 'logs'; S.logQ = ''; S.logN = 30; S.logOpen = null; }
  else if (id === 'logmore') {
    const p = app.querySelector('.panel'), top = p.scrollTop;
    S.logN += 30;
    document.getElementById('loglist').innerHTML = logBody(); p.scrollTop = top; return;
  }
  else if (id === 'logclear') { S.logQ = ''; S.logN = 30; }
  else if (id === 'logback') { S.past = 'settings'; }
  else if (key) {
    if (OPTS[key]) { const o = OPTS[key]; S.cfg[key] = o[(o.indexOf(S.cfg[key]) + 1) % o.length]; }
    else if (key === 'forget') { if (S.sure) { applyState(await invoke('forget')); S.sure = false; } else S.sure = true; }
    else if (key === 'startup') { try { applyState(await invoke('set_startup', { on: !S.cfg.startup })); } catch (e) { /* the setting stays as it was */ } }
    else S.cfg[key] = !S.cfg[key];
    if (key !== 'forget') S.sure = false;
    if (key !== 'forget' && key !== 'startup') persist();
  }
  else if (id === 'min') S.view = 'min';
  else if (id === 'close') { invoke('close_window'); return; }
  else if (id === 'restore') S.view = 'full';
  else if (id === 'pause') applyState(await invoke('set_paused', { paused: !S.paused }));
  else if (id === 'pastbtn') S.past = 'past';
  else if (id === 'setbtn') S.past = 'settings';
  else if (id === 'pastclose') { S.past = false; S.sure = false; }
  else if (id === 'next') applyState(await invoke('new_receipt'));
  else {
    const e = S.items.find(x => x.id === +b.dataset.id); if (!e) return;
    const act = b.dataset.a;
    if (act === 'open') S.openId = S.openId === e.id ? null : e.id;
    else if (act === 'show') { invoke('show_item', { id: e.id }); return; }
    else {
      if (act === 'pick' || act === 'leave') { S.skipAnchor = e.id; if (S.openId === e.id) S.openId = null; }
      const action = act === 'void' || act === 'redo' || act === 'pick' || act === 'leave' || act === 'retry' ? act : null;
      if (action) applyState(await invoke('item_action', { id: e.id, action, bin: b.dataset.bin || null }));
    }
  }
  draw();
});
app.addEventListener('input', ev => {
  if (ev.target.id !== 'logq') return;
  S.logQ = ev.target.value; S.logN = 30;
  document.getElementById('loglist').innerHTML = logBody();
  document.getElementById('logclear').classList.toggle('off', !S.logQ);
});
app.addEventListener('mousedown', ev => {
  if (ev.button === 0 && ev.target.closest('.bar') && !ev.target.closest('button')) invoke('start_drag');
});
document.addEventListener('contextmenu', ev => { if (!ev.target.closest('input')) ev.preventDefault(); });
/* Wheel: each notch travels a shorter distance, in about the same time as a normal notch. */
const WHEEL_STEP = 0.6;
let glide = null;
function stopGlide() { if (glide) { cancelAnimationFrame(glide.raf); glide = null; } }
function startGlide(el, to) {
  stopGlide();
  glide = { el, target: Math.max(0, Math.min(el.scrollHeight - el.clientHeight, to)), raf: 0 };
  const step = () => {
    const left = glide.target - el.scrollTop, was = el.scrollTop;
    if (Math.abs(left) < 4) { el.scrollTop = glide.target; glide = null; return; }
    el.scrollTop += left * 0.3;
    if (el.scrollTop === was) { glide = null; return; }
    glide.raf = requestAnimationFrame(step);
  };
  glide.raf = requestAnimationFrame(step);
}
app.addEventListener('wheel', ev => {
  if (ev.ctrlKey) return;
  const el = ev.target.closest('.panel, .roll'); if (!el) return;
  const max = el.scrollHeight - el.clientHeight; if (max <= 0) return;
  const unit = ev.deltaMode === 1 ? 16 : ev.deltaMode === 2 ? el.clientHeight : 1;
  const from = glide && glide.el === el ? glide.target : el.scrollTop;
  const target = Math.max(0, Math.min(max, from + ev.deltaY * unit * WHEEL_STEP));
  if (target === from) return;
  ev.preventDefault();
  if (reduce || document.hidden) { el.scrollTop = target; return; }
  if (glide && glide.el === el) { glide.target = target; return; }
  startGlide(el, target);
}, { passive: false });

/* Height only: the window grows or shrinks and the file list takes the difference. */
const MIN_ROLL = 120, BASE_H = 880;
function setHeight(px) {
  const win = app.querySelector('.win.full'); if (!win) return;
  const roll = win.querySelector('.rollwrap'), floor = win.offsetHeight - roll.offsetHeight + MIN_ROLL;
  S.cfg.height = Math.round(Math.min(screen.availHeight, Math.max(floor, px)));
  win.style.height = S.cfg.height + 'px';
}
app.addEventListener('pointerdown', ev => {
  const g = ev.target.closest('#grip'); if (!g) return;
  ev.preventDefault(); hideTip(); S.dragging = true; g.classList.add('drag');
  document.body.style.userSelect = 'none';
  g.setPointerCapture(ev.pointerId);
  const startY = ev.clientY, startH = S.cfg.height;
  const move = e => setHeight(startH + e.clientY - startY);
  const up = () => { S.dragging = false; g.classList.remove('drag'); document.body.style.userSelect = ''; persist(); removeEventListener('pointermove', move); removeEventListener('pointerup', up); removeEventListener('pointercancel', up); };
  addEventListener('pointermove', move); addEventListener('pointerup', up); addEventListener('pointercancel', up);
});
app.addEventListener('dblclick', ev => { if (ev.target.closest('#grip')) { setHeight(BASE_H); persist(); } });
app.addEventListener('keydown', ev => {
  if (!ev.target.closest || !ev.target.closest('#grip')) return;
  if (ev.key === 'ArrowUp' || ev.key === 'ArrowDown') { ev.preventDefault(); setHeight(S.cfg.height + (ev.key === 'ArrowUp' ? -20 : 20)); persist(); }
});

/* Receipt-style tips */
let tipTimer = null, tipFor = null;
function tipKey(t) { return [t.dataset.tip, t.dataset.id || '', t.dataset.bin || ''].join('|'); }
function hideTip() { clearTimeout(tipTimer); tipFor = null; const old = app.querySelector('.tipslip'); if (old) old.remove(); }
function showTip(target) {
  const win = target.closest('.win'), data = LX().tip[target.dataset.tip];
  if (!win || !data || !S.cfg.tips) return;
  const old = app.querySelector('.tipslip'); if (old) old.remove();
  const el = document.createElement('div');
  el.className = 'tipslip'; el.setAttribute('role', 'tooltip');
  el.innerHTML = '<div>' + html(data[0]) + '</div>' + (data[1] ? '<div class="tipbest"><b>' + LX().best + ':</b> ' + html(data[1]) + '</div>' : '');
  win.appendChild(el);
  const w = win.getBoundingClientRect(), r = target.getBoundingClientRect(), h = el.offsetHeight;
  const below = r.bottom - w.top + 6, above = r.top - w.top - h - 6;
  el.style.top = (below + h < w.height - 8 ? below : Math.max(8, above)) + 'px';
}
function tipTarget(ev) { const t = ev.target.closest && ev.target.closest('[data-tip]'); return t && app.contains(t) ? t : null; }
function armTip(ev, wait) {
  if (S.dragging) return;
  const t = tipTarget(ev);
  if (!t) { hideTip(); return; }
  if (t === tipFor) return;
  hideTip(); tipFor = t;
  tipTimer = setTimeout(() => showTip(t), wait);
}
app.addEventListener('mouseover', ev => armTip(ev, 350));
app.addEventListener('focusin', ev => { if (ev.target.matches(':focus-visible')) armTip(ev, 200); });
app.addEventListener('mouseleave', hideTip);
app.addEventListener('focusout', hideTip);
app.addEventListener('keydown', ev => { if (ev.key === 'Escape') hideTip(); });
/* ---------- Start ---------- */
let lastH = 0;
function syncWindow() {
  const win = app.querySelector('.win'); if (!win) return;
  const h = Math.ceil(win.getBoundingClientRect().height);
  if (h && h !== lastH) { lastH = h; invoke('set_window_height', { height: h }); }
}
new ResizeObserver(syncWindow).observe(app);
(async () => {
  // Fonts load only when first used, and the guide measures its text, so load them first.
  await Promise.all(['400 13px "JetBrains Mono"', '700 13px "JetBrains Mono"', '800 13px "JetBrains Mono"', '400 14px "IBM Plex Sans Arabic"', '700 14px "IBM Plex Sans Arabic"', '400 13px "Courier Prime"', '700 13px "Courier Prime"', '400 16px "VT323"'].map(f => document.fonts.load(f, 'A\u0628'))).catch(() => {});
  const loaded = await invoke('load');
  S.version = loaded.version; S.choices = loaded.choices;
  applyState(loaded.state);
  const avail = screen.availHeight;
  if (S.cfg.height > avail) S.cfg.height = avail;
  if (!S.cfg.welcomed) await openWizard(0, false);
  draw();
  syncWindow();
  invoke('show_window');
  if (S.cfg.updates) checkUpdate();
  shareMoved();
  setInterval(shareMoved, SHARE_EVERY_MS);
  window.__TAURI__.event.listen('changed', async e => {
    applyState(e.payload);
    if (!S.past) draw();
    else if (S.past === 'logs') {
      const p = app.querySelector('.panel'), l = document.getElementById('loglist');
      if (p && l && p.scrollTop <= 1) { await refreshLogs(); l.innerHTML = logBody(); }
    }
  });
})();
})();
