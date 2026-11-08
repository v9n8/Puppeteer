import { createApp } from 'vue'
import { createPinia } from 'pinia'
import { VueQueryPlugin } from '@tanstack/vue-query'
import router from './router'
import { i18n } from './i18n'
import './assets/main.css'
import App from './App.vue'
import { installHomeEndKeys } from './lib/homeEndKeys'
import { isRemoteMode } from './lib/ipc/remote'
import { installQuoteAppend } from './lib/quoteAppend'

installHomeEndKeys()

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
