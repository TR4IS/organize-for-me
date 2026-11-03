import { app, dialog, net, BrowserWindow } from 'electron'
import { spawn } from 'child_process'
import fs from 'fs'
import os from 'os'
import path from 'path'
import crypto from 'crypto'
import { getConfig } from './config'

// FileOrganizer's last update. It offers to install the app's successor, Organize for Me, and only after that worked,
// to remove this old version, so nobody ends up with two apps sorting the same folder. Files are never touched here.

const RELEASES_API = 'https://api.github.com/repos/TR4IS/organize-for-me/releases?per_page=30'
const HOMEPAGE = 'https://n3trunner.dev/organizeforme'
const NEW_APP = path.join(process.env.LOCALAPPDATA || '', 'Organize for Me', 'organize-for-me.exe')
const ASK_DELAY_MS = 4000

const TEXT = {
  en: {
    title: 'FileOrganizer is now Organize for Me',
    detail: 'A new version, rebuilt from scratch, is ready. It never deletes or overwrites a file, and every move can be undone.\n\n' +
      'Install now downloads and installs the new app, then offers to remove this old version. Your files stay where they are. ' +
      'Settings and history do not carry over, so you will choose your folder again.',
    install: 'Install now', later: 'Later',
    failed: 'The new version could not be downloaded. Try again later, or get it from ' + HOMEPAGE,
    notInstalled: 'The new version was not installed, so FileOrganizer stays as it is.',
    removeTitle: 'Organize for Me is installed',
    removeDetail: 'Remove the old FileOrganizer now? Your files stay where they are.',
    remove: 'Remove it', keep: 'Keep it',
  },
  ar: {
    title: 'FileOrganizer صار اسمه رتب لي',
    detail: 'فيه نسخة جديدة مبنية من الصفر. ما تحذف ولا تكتب فوق أي ملف، وكل نقلة تقدر ترجعها.\n\n' +
      'لو ضغطت ثبّت الحين، ينزل البرنامج الجديد ويتثبت، وبعدها يسألك تشيل النسخة القديمة. ملفاتك تبقى مكانها. ' +
      'الإعدادات والسجل ما تنتقل، فبتختار مجلدك مرة ثانية.',
    install: 'ثبّت الحين', later: 'بعدين',
    failed: 'ما قدرنا ننزل النسخة الجديدة. جرب بعدين، أو نزّلها من ' + HOMEPAGE,
    notInstalled: 'النسخة الجديدة ما تثبتت، وFileOrganizer يبقى مثل ما هو.',
    removeTitle: 'رتب لي تثبت',
    removeDetail: 'نشيل FileOrganizer القديم الحين؟ ملفاتك تبقى مكانها.',
    remove: 'شيله', keep: 'خله',
  },
}

interface Asset { name: string; browser_download_url: string; size: number; digest?: string }
interface Release { tag_name: string; draft: boolean; assets: Asset[] }

// The newest release of the new app that has an installer, betas included.
async function findInstaller(): Promise<Asset> {
  const response = await net.fetch(RELEASES_API, { headers: { Accept: 'application/vnd.github+json' } })
  if (!response.ok) throw new Error(`GitHub answered ${response.status}`)
  const releases = (await response.json()) as Release[]
  for (const release of releases) {
    if (release.draft || !release.tag_name.startsWith('v3')) continue
    const asset = release.assets.find(a => /^OrganizeForMe-.*-setup\.exe$/.test(a.name))
    if (asset) return asset
  }
  throw new Error('no installer found')
}

async function download(asset: Asset): Promise<string> {
  const response = await net.fetch(asset.browser_download_url)
  if (!response.ok) throw new Error(`download answered ${response.status}`)
  const data = Buffer.from(await response.arrayBuffer())
  if (data.length !== asset.size || data[0] !== 0x4d || data[1] !== 0x5a) throw new Error('download is incomplete')
  if (asset.digest?.startsWith('sha256:')) {
    const actual = crypto.createHash('sha256').update(data).digest('hex')
    if (actual !== asset.digest.slice(7)) throw new Error('download does not match GitHub\'s checksum')
  }
  const file = path.join(os.tmpdir(), asset.name)
  fs.writeFileSync(file, data)
  return file
}

function runInstaller(file: string): Promise<number> {
  return new Promise(resolve => {
    const child = spawn(file, [], { stdio: 'ignore' })
    child.on('exit', code => resolve(code ?? 1))
    child.on('error', () => resolve(1))
  })
}

// Leaves Windows with nothing pointing at this version, then hands over to its own uninstaller.
function removeOldVersion(): void {
  app.setLoginItemSettings({ openAtLogin: false })
  const uninstaller = path.join(path.dirname(process.execPath), 'Uninstall FileOrganizer.exe')
  if (fs.existsSync(uninstaller)) {
    spawn(uninstaller, ['/S'], { detached: true, stdio: 'ignore' }).unref()
  }
  app.exit(0)
}

export function offerNewApp(
  getWindow: () => BrowserWindow | null,
  pauseWatching: () => void,
  resumeWatching: () => void,
  log: (line: string) => void,
): void {
  if (!app.isPackaged) return
  setTimeout(async () => {
    const t = TEXT[getConfig().lang === 'ar' ? 'ar' : 'en']
    const parent = getWindow() ?? undefined
    const ask = await dialog.showMessageBox(parent!, {
      type: 'info', title: t.title, message: t.title, detail: t.detail,
      buttons: [t.install, t.later], defaultId: 0, cancelId: 1, noLink: true,
    })
    if (ask.response !== 0) {
      log('[*] Organize for Me offer: later')
      return
    }
    // Nothing gets sorted while the apps change hands.
    pauseWatching()
    getWindow()?.setProgressBar(2)
    try {
      log('[*] Downloading Organize for Me')
      const file = await download(await findInstaller())
      getWindow()?.setProgressBar(-1)
      log('[*] Running the Organize for Me installer')
      const code = await runInstaller(file)
      fs.rmSync(file, { force: true })
      if (code !== 0 || !fs.existsSync(NEW_APP)) {
        log(`[!] Organize for Me was not installed (exit ${code})`)
        await dialog.showMessageBox(parent!, { type: 'info', title: t.title, message: t.notInstalled, buttons: ['OK'] })
        resumeWatching()
        return
      }
      log('[*] Organize for Me installed')
      const remove = await dialog.showMessageBox(parent!, {
        type: 'question', title: t.removeTitle, message: t.removeTitle, detail: t.removeDetail,
        buttons: [t.remove, t.keep], defaultId: 0, cancelId: 1, noLink: true,
      })
      if (remove.response === 0) {
        log('[*] Removing the old FileOrganizer')
        removeOldVersion()
        return
      }
      // Kept: stay installed but stop sorting, since the new app now sorts the folder.
      log('[*] Old FileOrganizer kept, watching stays off')
    } catch (err) {
      getWindow()?.setProgressBar(-1)
      log(`[!] Could not get Organize for Me: ${err}`)
      await dialog.showMessageBox(parent!, { type: 'warning', title: t.title, message: t.failed, buttons: ['OK'] })
      resumeWatching()
    }
  }, ASK_DELAY_MS)
}
