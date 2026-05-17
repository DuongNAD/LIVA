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
    chunkSizeWarningLimit: 1000,
    rollupOptions: {
      // [Phase 5.1] Fail-fast: Cắt đứt mọi liên kết vô tình với Node.js API trong Frontend
      external: ['fs', 'path', 'os', 'crypto', 'child_process', 'electron'],
      input: {
        widget: resolve(__dirname, 'widget.html'),
        dashboard: resolve(__dirname, 'dashboard.html'),
      },
      output: {
        manualChunks(id) {
          if (id.includes('node_modules')) {
            if (id.includes('three') || id.includes('@pixiv')) {
              return 'vendor-three';
            }
            if (id.includes('pixi.js') || id.includes('pixi-live2d-display')) {
              return 'vendor-pixi';
            }
            if (id.includes('onnxruntime') || id.includes('@mediapipe')) {
              return 'vendor-ai';
            }
            if (id.includes('vue')) {
              return 'vendor-vue';
            }
            return 'vendor';
          }
        }
      }
    }
  },
  server: {
    host: true, // Listen on all local IPs (0.0.0.0) for Mobile LAN access
    port: 5173,
    strictPort: true
  }
})
