const { app, BrowserWindow, screen } = require('electron');
const path = require('path');

function createOverlayWindow () {
    // 1. Lấy kích thước không gian làm việc của màn hình (Work Area Size)
    // Nó sẽ tự động trừ đi chiều cao của thanh Taskbar để không bị lẹm hình
    const primaryDisplay = screen.getPrimaryDisplay();
    const { width, height } = primaryDisplay.workAreaSize;

    // 2. Cố định kích thước không gian 3D của Liva
    const windowWidth = 400;
    const windowHeight = 500;

    // 3. Thuật toán tính toán tọa độ góc dưới cùng bên phải (Bottom-Right Coordinates)
    const xPos = width - windowWidth;
    const yPos = height - windowHeight;

    const win = new BrowserWindow({
        width: windowWidth,
        height: windowHeight,
        x: xPos,             // Trục ngang
        y: yPos,             // Trục dọc
        transparent: true, 
        frame: false,      
        alwaysOnTop: true, 
        skipTaskbar: true, 
        webPreferences: {
            nodeIntegration: true,
            contextIsolation: false
        }
    });

    // Cho phép chuột xuyên thấu (Click-through)
    win.setIgnoreMouseEvents(true, { forward: true });

    // Tải giao diện HTML
    win.loadFile('index.html');
}

app.whenReady().then(createOverlayWindow);

app.on('window-all-closed', () => {
    if (process.platform !== 'darwin') app.quit();
});