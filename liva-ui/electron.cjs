const { app, BrowserWindow, screen, Tray, Menu, ipcMain, nativeImage } = require('electron');
const { spawn } = require('child_process');
const path = require('path');
const os = require('os');
const fs = require('fs');

const isWindows = os.platform() === 'win32';
const isDev = !app.isPackaged;

// ═══════════════════════════════════════════════════════
//  Anti-Zombie Flag: Cho phép thoát thật khi user bấm Quit
// ═══════════════════════════════════════════════════════
let isQuitting = false;

// ═══════════════════════════════════════════════════════
//  Window Handles
// ═══════════════════════════════════════════════════════
let widgetWindow = null;
let dashboardWindow = null;
let tray = null;

// ═══════════════════════════════════════════════════════
//  Background Services Management
// ═══════════════════════════════════════════════════════
const backgroundProcesses = [];

function spawnBackgroundServices() {
  const rootDir = path.join(__dirname, '..');

  // Cross-platform Virtual Environment Path
  const pythonPath = isWindows
    ? path.join(rootDir, 'liva-ai-engine', 'venv', 'Scripts', 'python.exe')
    : path.join(rootDir, 'liva-ai-engine', 'venv', 'bin', 'python');

  const npxCmd = isWindows ? 'npx.cmd' : 'npx';

  console.log("🚀 [Electron Main] Đang khởi động dàn vệ tinh ngầm...");

  // 1. AI Engine — [ZERO-PYTHON PIVOT]
  // LLM Runtime đã chuyển sang C++ native (llama-server.exe) do ModelOrchestrator quản lý trực tiếp.
  // Không cần spawn Python engine.py nữa. Gateway sẽ tự gọi llama-server.exe khi bootstrap.
  const nativeDllPath = path.join(rootDir, 'liva-ai-engine', 'native_lib', 'llama.dll');
  const hasNativeEngine = fs.existsSync(nativeDllPath);

  if (hasNativeEngine) {
    console.log("⚡ [Electron Main] Native Engine detected — Zero-Overhead Mode!");
    const nativeEngine = spawn(pythonPath, ['liva_native_engine.py'], {
      cwd: path.join(rootDir, 'liva-ai-engine'),
      detached: false,
      stdio: ['pipe', 'pipe', 'ignore'],
      env: { ...process.env, LIVA_USE_NATIVE: 'true' }
    });
    backgroundProcesses.push(nativeEngine);
    nativeEngine.stdout.on('data', (d) => console.log(`[Native Engine (IPC:8100)] ${d.toString().trim()}`));
    nativeEngine.on('close', (code) => console.log(`[Native Engine (IPC:8100)] Đã đóng với mã ${code}`));
    process.env.LIVA_USE_NATIVE = 'true';
  } else {
    console.log("🚀 [Electron Main] Zero-Python Mode: LLM Engine sẽ do Gateway/ModelOrchestrator quản lý (C++ llama-server.exe).");
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
    shell: isWindows,
    env: { ...process.env }
  });
  backgroundProcesses.push(gateway);

  logProcess(voiceEngine, 'Voice Engine');
  logProcess(gateway, 'Gateway Node');
}

function logProcess(proc, name) {
  if (proc.stdout) proc.stdout.on('data', (d) => console.log(`[${name}] ${d.toString().trim()}`));
  if (proc.stderr) proc.stderr.on('data', (d) => console.error(`[${name} ERR] ${d.toString().trim()}`));
  proc.on('close', (code) => console.log(`[${name}] Đã đóng với mã ${code}`));
}

// ═══════════════════════════════════════════════════════
//  URL / File Loading (Dev vs Production)
// ═══════════════════════════════════════════════════════
function getWidgetURL() {
  if (isDev) return 'http://127.0.0.1:5173/widget.html';
  return `file://${path.join(__dirname, 'dist', 'widget.html')}`;
}

function getDashboardURL() {
  if (isDev) return 'http://127.0.0.1:5173/dashboard.html';
  return `file://${path.join(__dirname, 'dist', 'dashboard.html')}`;
}

// ═══════════════════════════════════════════════════════
//  Widget Window (Transparent Overlay — Always Visible)
// ═══════════════════════════════════════════════════════
function createWidgetWindow() {
  const { width, height } = screen.getPrimaryDisplay().workAreaSize;
  const widgetWidth = 450;
  const widgetHeight = 850;

  widgetWindow = new BrowserWindow({
    width: widgetWidth,
    height: widgetHeight,
    x: width - widgetWidth - 20,
    y: height - widgetHeight,
    transparent: true,
    frame: false,
    alwaysOnTop: true,
    hasShadow: false,
    resizable: false,
    skipTaskbar: false,
    icon: path.join(__dirname, 'public', 'icons', 'liva-icon.png'),
    webPreferences: {
      preload: path.join(__dirname, 'preload.cjs'),
      contextIsolation: true,
      nodeIntegration: false
    }
  });

  // Mặc định: click xuyên qua (Phương án 1 — Ghost Mode)
  widgetWindow.setIgnoreMouseEvents(true, { forward: true });

  const loadWithRetry = (url, retries = 50, delayMs = 2000) => {
    widgetWindow.loadURL(url).catch(() => {
      console.log(`Không thể kết nối ${url}, đang thử lại... (còn ${retries} lần)`);
      if (retries > 0) {
        setTimeout(() => loadWithRetry(url, retries - 1, delayMs), delayMs);
      }
    });
  };

  loadWithRetry(getWidgetURL());

  widgetWindow.on('closed', () => {
    widgetWindow = null;
  });
}

// ═══════════════════════════════════════════════════════
//  Dashboard Window (Framed, Hidden by Default)
// ═══════════════════════════════════════════════════════
function createDashboardWindow() {
  const { width, height } = screen.getPrimaryDisplay().workAreaSize;

  dashboardWindow = new BrowserWindow({
    width: 1280,
    height: 800,
    x: Math.round((width - 1280) / 2),
    y: Math.round((height - 800) / 2),
    show: false, // Ẩn mặc định — chỉ show khi user yêu cầu
    frame: false, // Custom titlebar
    transparent: false,
    resizable: true,
    minWidth: 900,
    minHeight: 600,
    icon: path.join(__dirname, 'public', 'icons', 'liva-icon.png'),
    webPreferences: {
      preload: path.join(__dirname, 'preload.cjs'),
      contextIsolation: true,
      nodeIntegration: false
    }
  });

  const loadWithRetry = (url, retries = 50, delayMs = 2000) => {
    dashboardWindow.loadURL(url).catch(() => {
      console.log(`Dashboard: đang chờ Vite... (còn ${retries} lần)`);
      if (retries > 0) {
        setTimeout(() => loadWithRetry(url, retries - 1, delayMs), delayMs);
      }
    });
  };

  loadWithRetry(getDashboardURL());

  // Anti-Zombie: Ẩn thay vì đóng (trừ khi đang thoát thật)
  dashboardWindow.on('close', (e) => {
    if (!isQuitting) {
      e.preventDefault();
      dashboardWindow.hide();
    }
  });

  dashboardWindow.on('closed', () => {
    dashboardWindow = null;
  });
}

// ═══════════════════════════════════════════════════════
//  System Tray
// ═══════════════════════════════════════════════════════
function createTray() {
  const iconPath = path.join(__dirname, 'public', 'icons', 'liva-icon.png');
  let trayIcon;

  if (fs.existsSync(iconPath)) {
    trayIcon = nativeImage.createFromPath(iconPath);
  } else {
    // Fallback: tạo icon trắng 16x16
    trayIcon = nativeImage.createEmpty();
  }

  tray = new Tray(trayIcon);
  tray.setToolTip('LIVA — AI Desktop Assistant');

  const contextMenu = Menu.buildFromTemplate([
    {
      label: '📋 Open Dashboard',
      click: () => {
        if (dashboardWindow) {
          dashboardWindow.show();
          dashboardWindow.focus();
        }
      }
    },
    { type: 'separator' },
    {
      label: '🔄 Restart Services',
      click: () => {
        console.log('[Tray] Restarting services...');
        // TODO: Implement service restart
      }
    },
    { type: 'separator' },
    {
      label: '❌ Quit LIVA',
      click: () => {
        isQuitting = true;
        app.quit();
      }
    }
  ]);

  tray.setContextMenu(contextMenu);

  // Double-click tray icon → show dashboard
  tray.on('double-click', () => {
    if (dashboardWindow) {
      dashboardWindow.show();
      dashboardWindow.focus();
    }
  });
}

// ═══════════════════════════════════════════════════════
//  IPC Handlers
// ═══════════════════════════════════════════════════════
function setupIPC() {
  // Dashboard controls
  ipcMain.on('open-dashboard', () => {
    if (dashboardWindow) {
      dashboardWindow.show();
      dashboardWindow.focus();
    }
  });

  ipcMain.on('close-dashboard', () => {
    if (dashboardWindow) {
      dashboardWindow.hide();
    }
  });

  ipcMain.on('minimize-window', (event) => {
    const win = BrowserWindow.fromWebContents(event.sender);
    if (win) win.minimize();
  });

  ipcMain.on('maximize-window', (event) => {
    const win = BrowserWindow.fromWebContents(event.sender);
    if (win) {
      win.isMaximized() ? win.unmaximize() : win.maximize();
    }
  });

  // Mouse passthrough (Phantom Bounding Box Fix)
  ipcMain.on('set-ignore-mouse-events', (event, ignore) => {
    const win = BrowserWindow.fromWebContents(event.sender);
    if (!win) return;

    if (process.platform === 'win32' || process.platform === 'darwin') {
      win.setIgnoreMouseEvents(ignore, { forward: true });
    } else {
      win.setIgnoreMouseEvents(ignore);
    }
  });

  // Avatar config changed → notify widget to hot-swap
  ipcMain.on('avatar-config-changed', (_event, config) => {
    if (widgetWindow && !widgetWindow.isDestroyed()) {
      widgetWindow.webContents.send('avatar-changed', config);
    }
  });

  // Config updated → broadcast to all windows
  ipcMain.on('config-updated', (_event, config) => {
    if (widgetWindow && !widgetWindow.isDestroyed()) {
      widgetWindow.webContents.send('config-updated', config);
    }
    if (dashboardWindow && !dashboardWindow.isDestroyed()) {
      dashboardWindow.webContents.send('config-updated', config);
    }
  });

  // Global Mouse Position (for LookAt — LIVA eyes follow cursor)
  ipcMain.handle('get-mouse-position', () => {
    const cursor = screen.getCursorScreenPoint();
    if (!widgetWindow || widgetWindow.isDestroyed()) return { x: 0, y: 0 };
    const bounds = widgetWindow.getBounds();
    // Normalize to -1..1 relative to widget center
    const rx = ((cursor.x - bounds.x - bounds.width / 2) / (bounds.width / 2));
    const ry = ((cursor.y - bounds.y - bounds.height / 2) / (bounds.height / 2));
    return {
      x: Math.max(-1, Math.min(1, rx)),
      y: Math.max(-1, Math.min(1, ry)),
    };
  });
}

// ═══════════════════════════════════════════════════════
//  App Lifecycle
// ═══════════════════════════════════════════════════════
app.whenReady().then(() => {
  setupIPC();
  spawnBackgroundServices();
  createWidgetWindow();
  createDashboardWindow();
  createTray();
});

app.on('window-all-closed', () => {
  if (process.platform !== 'darwin') {
    app.quit();
  }
});

// Cleanup: tiêu diệt toàn bộ vệ tinh nền khi Electron đóng
app.on('will-quit', () => {
  console.log("🛑 [Electron Main] Tiến hành tiêu diệt dàn vệ tinh nền...");
  backgroundProcesses.forEach(proc => {
    try {
      if (isWindows) {
        spawn('taskkill', ['/pid', String(proc.pid), '/f', '/t']);
      } else {
        process.kill(-proc.pid, 'SIGKILL');
      }
    } catch (e) {
      console.warn("Lỗi tắt tiến trình nền:", e);
    }
  });

  // [ANTI-ZOMBIE] Truy sát mọi llama-server.exe còn sống sót để giải phóng VRAM
  if (isWindows) {
    try {
      spawn('taskkill', ['/IM', 'llama-server.exe', '/F'], { stdio: 'ignore' });
      console.log("🧹 [Anti-Zombie] Đã truy sát llama-server.exe tàn dư.");
    } catch (e) { /* ignore */ }
  }
});
