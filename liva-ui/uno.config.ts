import { defineConfig, presetUno } from 'unocss'

export default defineConfig({
  presets: [presetUno()],
  // Ngăn xếp font CỤC BỘ, không `presetWebFonts`.
  //
  // Trước 02/08/2026 chỗ này là `presetWebFonts({ provider: 'google',
  // fonts: { sans: 'Inter' } })`, và nó hỏng theo hai cách độc lập:
  //
  // 1. **CI không hermetic.** UnoCSS đi lấy CSS font từ `fonts.googleapis.com`
  //    lúc build/test. Ngày 01/08/2026 bước "Run UI Tests" trên `e02f1c5` ĐỎ
  //    với `[unocss] Fetch web fonts timeout` → unhandled rejection → vitest
  //    thoát 1. Commit đó chỉ đụng `docs/`. Một sự cố mạng ngoài biến thành
  //    "mã hỏng", và nó nhấp nháy theo đường truyền của runner.
  // 2. **Thủng chính định vị của sản phẩm.** LIVA chạy hoàn toàn cục bộ; máy
  //    người dùng có thể offline vĩnh viễn. Một font chỉ nạp được qua mạng thì
  //    trên máy đó **không bao giờ hiện**, và LIVA âm thầm rơi về font hệ
  //    thống mà không ai biết.
  //
  // Chọn ngăn xếp cục bộ thay vì self-host `.woff2`: `font-sans` được dùng ở
  // ĐÚNG MỘT chỗ (`App.vue`), nên thêm vài trăm KB nhị phân vào repo cho một
  // class là cái giá sai. Inter vẫn đứng đầu — máy nào đã cài thì vẫn dùng nó;
  // máy không có thì rơi về font giao diện của HĐH, vốn là lựa chọn đúng cho
  // một app desktop.
  theme: {
    fontFamily: {
      sans: '"Inter","Segoe UI Variable","Segoe UI",system-ui,-apple-system,"Noto Sans",Roboto,"Helvetica Neue",Arial,sans-serif',
    },
  },
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
