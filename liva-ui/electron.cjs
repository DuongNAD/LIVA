const { app, BrowserWindow, screen, ipcMain } = require('electron');

function createWindow() {
  // Lấy kích thước màn hình để tự động tính gốc tọa độ Góc Dưới Phải
  const { width, height } = screen.getPrimaryDisplay().workAreaSize;
  const widgetWidth = 450;
  const widgetHeight = 850;

  const win = new BrowserWindow({
    width: widgetWidth,
    height: widgetHeight,
    x: width - widgetWidth - 20, // Bo vào trong lề 20px
    y: height - widgetHeight, // Dưới cùng
    transparent: true,
    frame: false,
    alwaysOnTop: true,
    hasShadow: false,
    resizable: false,
    skipTaskbar: false, // Vẫn cho hiện ở thanh Taskbar để dễ tắt
    webPreferences: {
      nodeIntegration: true,
      contextIsolation: false
    }
  });

  // Bật chế độ đâm xuyên mặc định, nhưng cho phép forward event
  win.setIgnoreMouseEvents(true, { forward: true });

  // Lắng nghe lệnh Mở Khóa Click từ giao diện
  ipcMain.on('set-ignore-mouse-events', (event, ignore, options) => {
    const webContents = event.sender;
    const window = BrowserWindow.fromWebContents(webContents);
    if (window) {
      window.setIgnoreMouseEvents(ignore, options);
    }
  });

  // Tải trang Web của LIVA đang được Host bởi Vite vào cái Khung tàng hình này!
  const targetUrl = 'http://127.0.0.1:5173';
  
  const loadWithRetry = (url, retries = 5, delayMs = 2000) => {
    win.loadURL(url).catch((err) => {
      console.log(`Không thể kết nối ${url}, đang thử lại sau ${delayMs/1000}s... (còn ${retries} lần)`);
      if (retries > 0) {
        setTimeout(() => loadWithRetry(url, retries - 1, delayMs), delayMs);
      }
    });
  };

  loadWithRetry(targetUrl);
}

app.whenReady().then(createWindow);

app.on('window-all-closed', () => {
    app.quit();
});
