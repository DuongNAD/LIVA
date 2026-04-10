const { app, BrowserWindow, screen } = require('electron');

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

  // Tải trang Web của LIVA đang được Host bởi Vite vào cái Khung tàng hình này!
  win.loadURL('http://localhost:5173');
}

app.whenReady().then(createWindow);

app.on('window-all-closed', () => {
    app.quit();
});
