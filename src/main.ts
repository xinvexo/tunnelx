import { createApp, h } from 'vue'
import { createPinia } from 'pinia'
import { addCollection } from '@iconify/vue'
import App from './App.vue'
import { router } from './router'
import { i18n } from './i18n'
import { offlineIconCollections } from './icons.generated'
import { installPlainTextInputBehavior } from './utils/textInputBehavior'
import '@unocss/reset/tailwind.css'
import 'virtual:uno.css'
import 'highlight.js/styles/atom-one-dark.css'
import './styles/global.css'

// 注册离线图标集：打包后的 CSP（connect-src 'self'）会拦掉 @iconify/vue 对
// api.iconify.design 的运行时拉取，不注册的话生产构建里所有图标都是空白。
for (const collection of offlineIconCollections) {
    addCollection(collection)
}

installPlainTextInputBehavior()

const app = createApp({ render: () => h(App) })

// 把组件内未捕获的错误打到控制台，方便定位偶发空白页等问题。
app.config.errorHandler = (err, _instance, info) => {
    console.error('[tunnelx] vue error:', err, info)
}

app.use(createPinia())
app.use(router)
app.use(i18n)
app.mount('#app')
