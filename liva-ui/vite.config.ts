import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'
import UnoCSS from 'unocss/vite'
import { resolve } from 'path'

// https://vitejs.dev/config/
export default defineConfig({
  plugins: [
    vue(),
    UnoCSS()
  ],
  base: './',
  build: {
    rollupOptions: {
      input: {
        widget: resolve(__dirname, 'widget.html'),
        dashboard: resolve(__dirname, 'dashboard.html'),
      }
    }
  },
  server: {
    host: true, // Listen on all local IPs (0.0.0.0) for Mobile LAN access
    port: 5173,
    strictPort: true
  }
})
