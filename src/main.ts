import { createApp } from 'vue'
import { createPinia } from 'pinia'
import { VueQueryPlugin } from '@tanstack/vue-query'
import router from './router'
import { i18n, setLocale } from './i18n'
import './assets/main.css'
import App from './App.vue'
import { installHomeEndKeys } from './lib/homeEndKeys'
import { installFontPrefs } from './lib/appFonts'
import { isRemoteMode } from './lib/ipc/remote'
import { installQuoteAppend } from './lib/quoteAppend'
import { installAppearanceSync } from './lib/appearanceSync'
import { installAnimationSync } from './lib/animationSync'

installHomeEndKeys()
installFontPrefs()
// Loading spinners / pulses of every Session and window turn on one beat.
installAnimationSync()

function mountApp() {
  const app = createApp(App)
  app.use(createPinia())
  app.use(router)
  app.use(i18n)
  app.use(VueQueryPlugin)
  app.mount('#app')
  // A quote from a file window may target a run no screen shows: it then goes
  // to the stored draft, so this can't wait for RunView to mount.
  void installQuoteAppend()
  // Theme / language changed in any window (Settings may be its own window).
  // Here, after a remote-desktop boot, so its listen goes on the remote bus.
  installAppearanceSync(setLocale)
  return app
}

// The remote-desktop window (lib/remoteDesktopWindow.ts) drives another Puppeteer;
// its boot lives in its own chunk so the normal window never loads it.
if (isRemoteMode()) {
  import('./remoteDesktopBoot')
    .then((m) => m.bootRemoteDesktop(mountApp))
    .catch((e) => console.error('[remoteDesktop] boot failed', e))
} else {
  mountApp()
}
