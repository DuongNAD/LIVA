const { app, BrowserWindow, screen, ipcMain } = require('electron');
const { spawn } = require('child_process');
const path = require('path');
const os = require('os');

const isWindows = os.platform() === 'win32';

// Quản lý các tiến trình ngầm (Background Daemons)
const backgroundProcesses = [];

function spawnBackgroundServices() {
  const rootDir = path.join(__dirname, '..');
  const fs = require('fs');
  
  // Cross-platform Virtual Environment Path
  const pythonPath = isWindows 
    ? path.join(rootDir, 'liva-ai-engine', 'venv', 'Scripts', 'python.exe')
    : path.join(rootDir, 'liva-ai-engine', 'venv', 'bin', 'python');

  const npxCmd = isWindows ? 'npx.cmd' : 'npx';

  console.log("🚀 [Electron Main] Đang khởi động dàn vệ tinh ngầm...");

  // 1. Khởi chạy AI Engine — ưu tiên Native Engine (CFFI, port 8100)
  //    Falls back to legacy engine.py (HTTP, port 8000) if native DLL not found
  const nativeDllPath = path.join(rootDir, 'liva-ai-engine', 'native_lib', 'llama.dll');
  const hasNativeEngine = fs.existsSync(nativeDllPath);

  if (hasNativeEngine) {
    console.log("⚡ [Electron Main] Native Engine detected — Zero-Overhead Mode!");
    const nativeEngine = spawn(pythonPath, ['liva_native_engine.py'], { 
      cwd: path.join(rootDir, 'liva-ai-engine'),
      detached: false,
      env: { ...process.env, LIVA_USE_NATIVE: 'true' }
    });
    backgroundProcesses.push(nativeEngine);
    logProcess(nativeEngine, 'Native Engine (IPC:8100)');

    // Also set env for Gateway to use NativeIPCClient
    process.env.LIVA_USE_NATIVE = 'true';
  } else {
    console.log("📦 [Electron Main] Native DLL not found — using legacy HTTP engine.");
    const aiEngine = spawn(pythonPath, ['engine.py'], { 
      cwd: path.join(rootDir, 'liva-ai-engine'),
      detached: false 
    });
    backgroundProcesses.push(aiEngine);
    logProcess(aiEngine, 'AI Engine (HTTP:8000)');

    process.env.LIVA_USE_NATIVE = 'false';
  }

  // 2. Khởi chạy Voice Engine (Python 8002)
  const voiceEngine = spawn(pythonPath, ['voice_engine.py'], { 
    cwd: path.join(rootDir, 'liva-ai-engine'),
    detached: false 
  });
  backgroundProcesses.push(voiceEngine);

  // 3. Khởi chạy OpenClaw Gateway (Node.js 8082)
  const gateway = spawn(npxCmd, ['tsx', 'src/Gateway.ts'], { 
    cwd: path.join(rootDir, 'openclaw-gateway'),
    detached: false,
    env: { ...process.env }
  });
  backgroundProcesses.push(gateway);

  // Rút gọn Event Logging
  logProcess(voiceEngine, 'Voice Engine');
  logProcess(gateway, 'Gateway Node');
}

// Logging helper (hoisted for use inside spawnBackgroundServices)
function logProcess(proc, name) {
  proc.stdout.on('data', (d) => console.log(`[${name}] ${d.toString().trim()}`));
  proc.stderr.on('data', (d) => console.error(`[${name} ERR] ${d.toString().trim()}`));
  proc.on('close', (code) => console.log(`[${name}] Đã đóng với mã ${code}`));
}

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

app.whenReady().then(() => {
  // Giai đoạn 1: Foundation. Liva UI âm thầm gọi Python & Gateway ngay khi lên sóng.
  spawnBackgroundServices();
  createWindow();
});

app.on('window-all-closed', () => {
  app.quit();
});

// Chặn ngắt an toàn, tiêu diệt toàn bộ vệ tinh nền khi Electron đóng (Tránh Zombification)
app.on('will-quit', () => {
  console.log("🛑 [Electron Main] Tiến hành tiêu diệt dàn vệ tinh nền...");
  backgroundProcesses.forEach(proc => {
    try {
      if (isWindows) {
        spawn('taskkill', ['/pid', proc.pid, '/f', '/t']);
      } else {
        process.kill(-proc.pid, 'SIGKILL');
      }
    } catch (e) {
      console.warn("Lỗi tắt tiến trình nền:", e);
    }
  });
});
