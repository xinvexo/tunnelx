import {defineConfig} from 'vite'
import vue from '@vitejs/plugin-vue'
import UnoCSS from 'unocss/vite'
import {fileURLToPath, URL} from 'node:url'

const host = process.env.TAURI_DEV_HOST
const devHost = host || '127.0.0.1'

function isVueUsePureAnnotationWarning(warning: { code?: string; id?: string; message?: string }) {
  return warning.code === 'INVALID_ANNOTATION'
    && warning.id?.includes('/@vueuse/core/')
    && warning.message?.includes('#__PURE__')
}

// https://vitejs.dev/config/
export default defineConfig({
  plugins: [vue(), UnoCSS()],
  resolve: {
    alias: {
      '@': fileURLToPath(new URL('./src', import.meta.url)),
    },
  },
  clearScreen: false,
  server: {
    allowedHosts: true,
    port: 1420,
    strictPort: true,
    host: devHost,
    hmr: host
      ? {
        protocol: 'ws',
        host,
        port: 1421,
      }
      : undefined,
    watch: {
      ignored: ['**/src-tauri/**'],
    },
  },
  build: {
    target: 'esnext',
    minify: 'esbuild',
    rollupOptions: {
      onwarn(warning, warn) {
        if (isVueUsePureAnnotationWarning(warning)) return
        warn(warning)
      },
      output: {
        manualChunks: {
          'vendor-vue': ['vue', 'vue-router', 'pinia', 'vue-i18n'],
          'vendor-ui': ['reka-ui', '@iconify/vue', 'motion-v'],
          'vendor-highlight': ['highlight.js'],
        },
      },
    },
    chunkSizeWarningLimit: 600,
  },
})
