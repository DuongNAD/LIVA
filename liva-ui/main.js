const { app, BrowserWindow, screen } = require('electron');
const path = require('path');

function createOverlayWindow () {
    const primaryDisplay = screen.getPrimaryDisplay();
    const { width, height } = primaryDisplay.workAreaSize;

    const windowWidth = 400;
    const windowHeight = 750;

    const xPos = width - windowWidth;
    const yPos = height - windowHeight;

    const win = new BrowserWindow({
        width: windowWidth,
        height: windowHeight,
        x: xPos,             
        y: yPos,             
        transparent: true, 
        frame: false,      
        alwaysOnTop: true, 
        skipTaskbar: true, 
        webPreferences: {
            nodeIntegration: true,
            contextIsolation: false
        }
    });

    win.loadFile('index.html');
}

app.whenReady().then(createOverlayWindow);

app.on('window-all-closed', () => {
    if (process.platform !== 'darwin') app.quit();
});