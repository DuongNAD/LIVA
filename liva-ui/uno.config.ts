import { defineConfig, presetUno, presetWebFonts } from 'unocss'

export default defineConfig({
  presets: [
    presetUno(),
    presetWebFonts({
      provider: 'google',
      fonts: {
        sans: 'Inter',
      },
    }),
  ],
  rules: [
    ['glass', { 
        'background': 'rgba(255, 255, 255, 0.1)', 
        'backdrop-filter': 'blur(10px)',
        '-webkit-backdrop-filter': 'blur(10px)',
        'border': '1px solid rgba(255, 255, 255, 0.2)',
        'box-shadow': '0 4px 6px rgba(0, 0, 0, 0.1)'
    }]
  ]
})
