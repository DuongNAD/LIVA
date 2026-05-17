const { app, BrowserWindow, screen, Tray, Menu, ipcMain, nativeImage, safeStorage } = require('electron');
app.commandLine.appendSwitch('disable-gpu-shader-disk-cache');
app.commandLine.appendSwitch('disable-http-cache');
const { spawn } = require('child_process');
const path = require('path');
const os = require('os');
const fs = require('fs');

const isWindows = os.platform() === 'win32';
const isDev = !app.isPackaged;

// ═══════════════════════════════════════════════════════
//  Single Instance Lock: Chặn mở trùng lặp Electron
//  Khi start_all.bat chạy lại, instance cũ sẽ được focus
//  thay vì tạo thêm cửa sổ mới trên taskbar.
// ═══════════════════════════════════════════════════════
const gotSingleLock = app.requestSingleInstanceLock();

if (!gotSingleLock) {
  console.log('[Electron] Phát hiện instance đang chạy. Thoát bản sao này.');
  app.quit();
}

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

// Project root directory (dùng chung cho Vite và Background Services)
const rootDir = path.join(__dirname, '..');

// ═══════════════════════════════════════════════════════
//  DevSecOps: Secure Credential Vault (electron.safeStorage)
//  NOTE: Changed to use AES-256-GCM with LIVA_ENCRYPTION_KEY
//  This allows both Electron AND Gateway to encrypt/decrypt
//  using the same EncryptionEngine format.
// ═══════════════════════════════════════════════════════
const SENSITIVE_KEYS = ['ZALO_OA_ACCESS_TOKEN', 'ZALO_USER_ID', 'AI_API_KEY', 'TAVILY_API_KEY', 'EMAIL_PASS', 'EMAIL_HOST', 'EMAIL_USER'];
const VAULT_PATH = path.join(app.getPath('userData'), 'liva_vault.json');

// ═══════════════════════════════════════════════════════
//  Vite Dev Server Management (Auto-start)
// ═══════════════════════════════════════════════════════
let viteServer = null;
let restartTimer = null;

async function waitForVite(port, maxRetries = 60, intervalMs = 1000) {
  const net = require('net');
  return new Promise((resolve, reject) => {
    let retries = 0;
    const check = () => {
      const socket = new net.Socket();
      socket.setTimeout(1000);
      socket.on('connect', () => {
        socket.destroy();
        console.log(`[Vite] Server ready on port ${port}`);
        resolve();
      });
      socket.on('timeout', () => {
        socket.destroy();
        retry();
      });
      socket.on('error', () => {
        socket.destroy();
        retry();
      });
      socket.connect(port, '127.0.0.1');
    };
    const retry = () => {
      retries++;
      if (retries > maxRetries) {
        reject(new Error(`Vite server không khởi động được sau ${maxRetries} giây`));
      } else {
        try {
          process.stdout.write(`\r[Vite] Đang chờ... (${retries}/${maxRetries}) `);
        } catch (e) { /* ignore EPIPE */ }
        setTimeout(check, intervalMs);
      }
    };
    check();
  });
}

async function killPort(port) {
  return new Promise((resolve) => {
    const { exec } = require('child_process');
    if (isWindows) {
      exec(`netstat -ano | findstr :${port}`, (err, stdout) => {
        if (stdout) {
          const lines = stdout.trim().split('\n');
          for (const line of lines) {
            const parts = line.trim().split(/\s+/);
            const pid = parts[parts.length - 1];
            if (pid && !isNaN(pid)) {
              exec(`taskkill /F /PID ${pid}`, () => {});
            }
          }
        }
        setTimeout(resolve, 1000);
      });
    } else {
      exec(`lsof -ti:${port} | xargs kill -9 2>/dev/null; true`, () => resolve());
    }
  });
}

/**
 * Check if a port is already in use (someone else started Vite).
 * Returns true if port is listening, false otherwise.
 */
function isPortInUse(port) {
  const net = require('net');
  return new Promise((resolve) => {
    const socket = new net.Socket();
    socket.setTimeout(1000);
    socket.on('connect', () => {
      socket.destroy();
      resolve(true);
    });
    socket.on('timeout', () => {
      socket.destroy();
      resolve(false);
    });
    socket.on('error', () => {
      socket.destroy();
      resolve(false);
    });
    socket.connect(port, '127.0.0.1');
  });
}

async function spawnViteServer() {
  // [FIX] Check if Vite is already running (e.g. started by start_all.ps1 Service 5).
  // If port 5173 is already listening, reuse it — do NOT kill and respawn.
  const alreadyRunning = await isPortInUse(5173);
  if (alreadyRunning) {
    console.log('[Vite] Port 5173 đã có Vite đang chạy (start_all.ps1). Dùng lại, không spawn mới.');
    return;
  }

  // Port is free — spawn our own Vite server
  await killPort(5173);

  return new Promise((resolve, reject) => {
    const uiDir = path.join(rootDir, 'liva-ui');
    const viteEnv = { ...process.env, FORCE_COLOR: '1' };

    console.log('[Vite] Đang khởi động dev server...');

    // Dùng npx để chạy vite (đỡ phải lo path)
    viteServer = spawn('npx', ['vite', '--host', '--strictPort'], {
      cwd: uiDir,
      detached: false,
      stdio: ['pipe', 'pipe', 'pipe'],
      shell: isWindows,
      env: viteEnv
    });

    let resolved = false;

    const safeWrite = (stream, prefix, data) => {
      try {
        stream.write(`${prefix}${data}`);
      } catch (e) {
        // EPIPE error - ignore (parent stream closed)
      }
    };

    viteServer.stdout.on('data', (data) => {
      const output = data.toString();
      safeWrite(process.stdout, '[Vite] ', output);
      // Resolve khi thấy dấu hiệu server ready
      if ((output.includes('Local:') || output.includes('ready in')) && !resolved) {
        resolved = true;
        resolve();
      }
    });

    viteServer.stderr.on('data', (data) => {
      safeWrite(process.stderr, '[Vite ERR] ', data.toString());
    });

    viteServer.on('close', (code) => {
      if (code !== 0 && code !== null) {
        console.log(`[Vite] Server đã đóng với mã ${code}`);
      }
    });

    viteServer.on('error', (err) => {
      console.error('[Vite] Lỗi khởi động:', err.message);
      reject(err);
    });

    // Timeout fallback: resolve sau 30s
    setTimeout(() => { if (!resolved) resolve(); }, 30000);
  });
}

/**
 * AES-256-GCM encryption compatible with EncryptionEngine (openclaw-gateway)
 * Format: iv:authTag:ciphertext (all hex)
 */
function encryptAes256Gcm(text) {
  const crypto = require('crypto');
  const key = process.env.LIVA_ENCRYPTION_KEY;
  if (!key || Buffer.byteLength(key, 'utf8') !== 32) {
    console.warn('[Vault] LIVA_ENCRYPTION_KEY not set or invalid, using safeStorage fallback');
    return null;
  }
  
  try {
    const iv = crypto.randomBytes(16);
    const cipher = crypto.createCipheriv('aes-256-gcm', Buffer.from(key), iv);
    let encrypted = cipher.update(text, 'utf8', 'hex');
    encrypted += cipher.final('hex');
    const authTag = cipher.getAuthTag().toString('hex');
    return `${iv.toString('hex')}:${authTag}:${encrypted}`;
  } catch (e) {
    console.warn('[Vault] Encryption failed:', e.message);
    return null;
  }
}

function decryptAes256Gcm(encryptedText) {
  const crypto = require('crypto');
  const key = process.env.LIVA_ENCRYPTION_KEY;
  if (!key || Buffer.byteLength(key, 'utf8') !== 32) {
    return null;
  }
  
  try {
    const parts = encryptedText.split(':');
    if (parts.length !== 3) return null;
    
    const iv = Buffer.from(parts[0], 'hex');
    const authTag = Buffer.from(parts[1], 'hex');
    const encrypted = parts[2];
    
    const decipher = crypto.createDecipheriv('aes-256-gcm', Buffer.from(key), iv);
    decipher.setAuthTag(authTag);
    let decrypted = decipher.update(encrypted, 'hex', 'utf8');
    decrypted += decipher.final('utf8');
    return decrypted;
  } catch (e) {
    return null;
  }
}

function manageSecureVault(gatewayDir) {
  const envPath = path.join(gatewayDir, '.env');
  let encryptionKey = process.env.LIVA_ENCRYPTION_KEY;
  let hasEncryptionKey = false;
  
  let vaultData = {};
  if (fs.existsSync(VAULT_PATH)) {
    try { vaultData = JSON.parse(fs.readFileSync(VAULT_PATH, 'utf8')); } catch(e) {}
  }

  if (fs.existsSync(envPath)) {
    const envLines = fs.readFileSync(envPath, 'utf8').split('\n');
    
    // FIRST PASS: Find LIVA_ENCRYPTION_KEY since dotenv is not loaded
    if (!encryptionKey) {
      for (const line of envLines) {
        const match = line.match(/^LIVA_ENCRYPTION_KEY=(.*)$/);
        if (match) {
          let val = match[1].trim();
          if (val.startsWith('"') && val.endsWith('"')) val = val.slice(1, -1);
          if (val.startsWith("'") && val.endsWith("'")) val = val.slice(1, -1);
          encryptionKey = val;
          process.env.LIVA_ENCRYPTION_KEY = val;
          break;
        }
      }
    }

    let migrated = false;
    hasEncryptionKey = encryptionKey && Buffer.byteLength(encryptionKey, 'utf8') === 32;
    const newEnvLines = [];
    
    for (const line of envLines) {
      const match = line.match(/^([^=]+)=(.*)$/);
      if (match) {
        const key = match[1].trim();
        let val = match[2].trim();
        if (val.startsWith('"') && val.endsWith('"')) val = val.slice(1, -1);
        if (val.startsWith("'") && val.endsWith("'")) val = val.slice(1, -1);
        if (key === 'EMAIL_PASS') val = val.replace(/\s+/g, '');
        
        // Skip if already looks migrated
        if (val.includes('đã được chuyển') || val.includes('liva_vault.json')) {
          newEnvLines.push(line);
          continue;
        }
        
        if (SENSITIVE_KEYS.includes(key) && val.length > 0) {
          if (hasEncryptionKey) {
            // Use AES-256-GCM (compatible with EncryptionEngine)
            const encrypted = encryptAes256Gcm(val);
            if (encrypted) {
              vaultData[key] = encrypted;
              migrated = true;
              console.log(`[DevSecOps] Đã mã hóa và di chuyển ${key} vào Vault an toàn.`);
              newEnvLines.push(`# ${key} đã được chuyển vào liva_vault.json (Mã hóa bởi EncryptionEngine)`);
              continue;
            }
          } else if (safeStorage && safeStorage.isEncryptionAvailable()) {
            // Fallback to safeStorage (old behavior)
            vaultData[key] = safeStorage.encryptString(val).toString('hex');
            migrated = true;
            console.log(`[DevSecOps] Đã mã hóa và di chuyển ${key} vào Vault (safeStorage fallback).`);
            newEnvLines.push(`# ${key} đã được chuyển vào liva_vault.json (Mã hóa bởi electron.safeStorage)`);
            continue;
          }
        }
      }
      newEnvLines.push(line);
    }
    
    if (migrated) {
      fs.writeFileSync(VAULT_PATH, JSON.stringify(vaultData, null, 2));
      fs.writeFileSync(envPath, newEnvLines.join('\n'));
    }
  }

  // Load decrypted values for this process
  const decryptedEnv = {};
  if (hasEncryptionKey) {
    for (const [key, encryptedValue] of Object.entries(vaultData)) {
      if (typeof encryptedValue !== 'string') continue;
      const decrypted = decryptAes256Gcm(encryptedValue);
      if (decrypted) {
        decryptedEnv[key] = decrypted;
      }
    }
  } else if (safeStorage && safeStorage.isEncryptionAvailable()) {
    // Fallback to safeStorage decryption
    for (const [key, hexValue] of Object.entries(vaultData)) {
      if (typeof hexValue !== 'string' || !hexValue.match(/^[0-9a-f]+$/i)) continue;
      try {
        decryptedEnv[key] = safeStorage.decryptString(Buffer.from(hexValue, 'hex'));
      } catch(e) {}
    }
  }
  return decryptedEnv;
}

function spawnBackgroundServices() {
  // Cross-platform Virtual Environment Path
  const pythonPath = isWindows
    ? path.join(rootDir, 'liva-ai-engine', 'venv', 'Scripts', 'python.exe')
    : path.join(rootDir, 'liva-ai-engine', 'venv', 'bin', 'python');

  // Resolve local tsx binary from monorepo root (avoid npx global spawn)
  const tsxBin = isWindows
    ? path.join(rootDir, 'node_modules', '.bin', 'tsx.cmd')
    : path.join(rootDir, 'node_modules', '.bin', 'tsx');

  console.log("🚀 [Electron Main] Đang khởi động dàn vệ tinh ngầm...");

  // DevSecOps: Khởi tạo Vault & Tự động migrate API Keys khỏi .env (Chống rò rỉ mã hóa cứng)
  const gatewayDir = path.join(rootDir, 'openclaw-gateway');
  const secureEnv = manageSecureVault(gatewayDir);
  const combinedEnv = { ...process.env, ...secureEnv };

  // [ARCH] Disabled auto-start Backend Services. Managed by start_all.bat for debugging.
  // 1. AI Engine — [ZERO-PYTHON PIVOT]
  // LLM Runtime đã chuyển sang C++ native (llama-server.exe) do ModelOrchestrator quản lý trực tiếp.
  // Không cần spawn Python engine.py nữa. Gateway sẽ tự gọi llama-server.exe khi bootstrap.
  const nativeDllPath = path.join(rootDir, 'liva-ai-engine', 'native_lib', 'llama.dll');
  const hasNativeEngine = fs.existsSync(nativeDllPath);

  if (hasNativeEngine) {
    console.log("⚡ [Electron Main] Native Engine detected — Zero-Overhead Mode!");
    /*
    const nativeEngine = spawn(pythonPath, ['liva_native_engine.py'], {
      cwd: path.join(rootDir, 'liva-ai-engine'),
      detached: false,
      stdio: ['pipe', 'pipe', 'ignore'],
      env: { ...combinedEnv, LIVA_USE_NATIVE: 'true' }
    });
    backgroundProcesses.push(nativeEngine);
    nativeEngine.stdout.on('data', (d) => console.log(`[Native Engine (IPC:8100)] ${d.toString().trim()}`));
    nativeEngine.on('close', (code) => console.log(`[Native Engine (IPC:8100)] Đã đóng với mã ${code}`));
    */
    process.env.LIVA_USE_NATIVE = 'true';
  } else {
    console.log("🚀 [Electron Main] Zero-Python Mode: LLM Engine sẽ do Gateway/ModelOrchestrator quản lý (C++ llama-server.exe).");
    process.env.LIVA_USE_NATIVE = 'false';
  }

  // 2. Khởi chạy Voice Engine (Python 8002)
  /*
  const voiceEngine = spawn(pythonPath, ['voice_engine.py'], {
    cwd: path.join(rootDir, 'liva-ai-engine'),
    detached: false
  });
  backgroundProcesses.push(voiceEngine);
  logProcess(voiceEngine, 'Voice Engine');
  */

  // 3. Khởi chạy OpenClaw Gateway (Node.js 8082) — dùng local tsx binary, không qua npx
  /*
  // [ARCH] Disabled auto-start Gateway. Managed by start_all.bat for debugging.
  const gateway = spawn(tsxBin, ['src/Gateway.ts'], {
    cwd: gatewayDir,
    detached: false,
    shell: isWindows,
    env: combinedEnv
  });
  backgroundProcesses.push(gateway);
  logProcess(gateway, 'Gateway Node');
  */
}

function logProcess(proc, name) {
  const safeLog = (prefix, msg) => { try { console.log(`${prefix}${msg}`); } catch (e) { /* ignore EPIPE */ } };
  if (proc.stdout) proc.stdout.on('data', (d) => safeLog(`[${name}] `, d.toString().trim()));
  if (proc.stderr) proc.stderr.on('data', (d) => safeLog(`[${name} ERR] `, d.toString().trim()));
  proc.on('close', (code) => safeLog(`[${name}] `, `Đã đóng với mã ${code}`));
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
  widgetWindow = new BrowserWindow({
    width: width,
    height: height,
    x: 0,
    y: 0,
    show: true,
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
    show: true,
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
  dashboardWindow.once('ready-to-show', () => {
    if (dashboardWindow && !dashboardWindow.isDestroyed()) {
      dashboardWindow.show();
    }
  });

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

  if (isDev) {
    const watchTargets = [
      path.join(rootDir, 'liva-ui', 'electron.cjs'),
      path.join(rootDir, 'liva-ui', 'preload.cjs'),
      path.join(rootDir, 'liva-ui', 'src'),
      path.join(rootDir, 'liva-ui', 'dashboard.html'),
      path.join(rootDir, 'liva-ui', 'widget.html'),
    ];
    const chokidar = require('chokidar');
    const watcher = chokidar.watch(watchTargets, { ignoreInitial: true });
    watcher.on('all', (_event, changedPath) => {
      console.log('[Electron] Dev change detected:', changedPath);
      scheduleElectronReload(changedPath);
    });
  }

  ipcMain.on('close-dashboard', () => {
    if (dashboardWindow) {
      dashboardWindow.hide();
    }
  });

  ipcMain.on('minimize-window', (event) => {
    const win = BrowserWindow.fromWebContents(event.sender);
    if (win) win.minimize();
  });

// ═══════════════════════════════════════════════════════
//  API & Env File Management
// ═══════════════════════════════════════════════════════
ipcMain.handle('get-env-config', async () => {
  try {
    const envPath = path.join(rootDir, 'openclaw-gateway', '.env');
    if (fs.existsSync(envPath)) return fs.readFileSync(envPath, 'utf8');
  } catch (e) { console.error('Lỗi đọc .env:', e); }
  return '';
});

ipcMain.handle('get-vault-config', async () => {
  try {
    if (fs.existsSync(VAULT_PATH)) {
      const vaultData = JSON.parse(fs.readFileSync(VAULT_PATH, 'utf8'));
      const decryptedData = {};
      const hasEncryptionKey = process.env.LIVA_ENCRYPTION_KEY && Buffer.byteLength(process.env.LIVA_ENCRYPTION_KEY, 'utf8') === 32;

      for (const [key, val] of Object.entries(vaultData)) {
        if (typeof val !== 'string') continue;
        if (hasEncryptionKey && val.includes(':')) {
          const dec = decryptAes256Gcm(val);
          if (dec) decryptedData[key] = dec;
        } else if (safeStorage && safeStorage.isEncryptionAvailable()) {
          try {
            decryptedData[key] = safeStorage.decryptString(Buffer.from(val, 'hex'));
          } catch(e) {}
        }
      }
      return decryptedData;
    }
  } catch (e) { console.error('Lỗi đọc vault:', e); }
  return {};
});

ipcMain.handle('save-env-config', async (event, content) => {
  try {
    const envPath = path.join(rootDir, 'openclaw-gateway', '.env');
    fs.writeFileSync(envPath, content, 'utf8');
    return { success: true };
  } catch (e) {
    console.error('Lỗi lưu .env:', e);
    return { success: false, error: String(e) };
  }
});

// ═══════════════════════════════════════════════════════
//  Window Controls & Frame Actions
  ipcMain.on('maximize-window', (event) => {
    const win = BrowserWindow.fromWebContents(event.sender);
    if (win) {
      win.isMaximized() ? win.unmaximize() : win.maximize();
    }
  });

  ipcMain.on('move-window', (event, dx, dy) => {
    const win = BrowserWindow.fromWebContents(event.sender);
    if (win) {
      const [x, y] = win.getPosition();
      win.setPosition(Math.round(x + dx), Math.round(y + dy));
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

  // Import avatar model file into public/models (Dashboard upload)
  ipcMain.handle('import-avatar-model', async (_event, payload) => {
    const { filename, dataBase64, type } = payload || {};
    if (!filename || !dataBase64) {
      return { success: false, error: 'Thiếu filename hoặc dữ liệu file.' };
    }
    const ext = path.extname(filename).toLowerCase();
    const allowed3d = ['.vrm', '.fbx'];
    const allowed2d = ['.json'];
    const is3d = type === '3d' || allowed3d.includes(ext);
    const is2d = type === '2d' || allowed2d.includes(ext);
    if (!is3d && !is2d) {
      return { success: false, error: 'Định dạng không hỗ trợ (.vrm, .fbx, .json).' };
    }
    const subDir = is3d ? 'vrm' : 'live2d';
    const destDir = path.join(__dirname, 'public', 'models', subDir);
    fs.mkdirSync(destDir, { recursive: true });
    const destPath = path.join(destDir, path.basename(filename));
    if (fs.existsSync(destPath)) {
      return { success: false, error: 'Model này đã tồn tại!' };
    }
    const buffer = Buffer.from(dataBase64, 'base64');
    if (buffer.length > 50 * 1024 * 1024) {
      return { success: false, error: 'File vượt quá 50 MB.' };
    }
    fs.writeFileSync(destPath, buffer);
    const relPath = path.basename(filename).replace(/\\/g, '/');
    return { success: true, filename: relPath, type: is3d ? '3d' : '2d', format: ext === '.vrm' ? 'vrm' : ext === '.fbx' ? 'fbx' : 'live2d' };
  });

  const importAvatarFolderLogic = (folderPath, filename) => {
    const srcFolder = path.resolve(folderPath);
    const folderName = path.basename(srcFolder);
    const preferredExts = ['.fbx', '.vrm', '.json'];

    const searchMainModel = (rootDir) => {
      const entries = fs.readdirSync(rootDir, { withFileTypes: true });
      const found = [];
      for (const entry of entries) {
        const full = path.join(rootDir, entry.name);
        if (entry.isDirectory()) {
          const nested = searchMainModel(full);
          if (nested) return nested;
        } else {
          const ext = path.extname(entry.name).toLowerCase();
          if (preferredExts.includes(ext)) found.push(full);
        }
      }
      found.sort((a, b) => preferredExts.indexOf(path.extname(a).toLowerCase()) - preferredExts.indexOf(path.extname(b).toLowerCase()));
      return found[0] || null;
    };

    const mainModelPath = searchMainModel(srcFolder);
    const targetName = path.basename(mainModelPath || filename);
    const ext = path.extname(targetName).toLowerCase();
    const is3d = ext === '.fbx' || ext === '.vrm';
    const destRoot = path.join(__dirname, 'public', 'models', is3d ? 'vrm' : 'live2d');
    const destDir = path.join(destRoot, folderName);
    
    if (fs.existsSync(destDir)) {
      const existingModel = searchMainModel(destDir);
      if (existingModel) {
        return { success: false, error: 'Folder model này đã tồn tại!' };
      }
      // If it's a ghost folder (no model inside), we allow overwriting.
    }
    
    fs.mkdirSync(destDir, { recursive: true });

    const copyRecursive = (src, dst) => {
      const stat = fs.statSync(src);
      if (stat.isDirectory()) {
        fs.mkdirSync(dst, { recursive: true });
        for (const entry of fs.readdirSync(src)) {
          copyRecursive(path.join(src, entry), path.join(dst, entry));
        }
      } else {
        fs.mkdirSync(path.dirname(dst), { recursive: true });
        fs.copyFileSync(src, dst);
      }
    };

    copyRecursive(srcFolder, destDir);
    const relDir = path.relative(path.join(__dirname, 'public', 'models'), destDir).replace(/\\/g, '/');
    const relModel = path.join(relDir, path.relative(srcFolder, mainModelPath || path.join(srcFolder, filename))).replace(/\\/g, '/');
    return { success: true, folderPath: relDir, filename: relModel, type: is3d ? '3d' : '2d', format: ext === '.vrm' ? 'vrm' : ext === '.fbx' ? 'fbx' : 'live2d', mainModel: relModel };
  };

  ipcMain.handle('import-avatar-model-folder', async (_event, payload) => {
    const { folderPath, filename } = payload || {};
    if (!folderPath || !filename) {
      return { success: false, error: 'Thiếu folderPath hoặc filename.' };
    }
    try {
      return importAvatarFolderLogic(folderPath, filename);
    } catch(e) {
      return { success: false, error: e.message };
    }
  });

  ipcMain.handle('select-import-avatar-folder', async () => {
    const { dialog } = require('electron');
    const result = await dialog.showOpenDialog(dashboardWindow, {
      title: 'Chọn thư mục Model 3D (chứa .fbx hoặc .vrm)',
      properties: ['openDirectory']
    });
    
    if (result.canceled || result.filePaths.length === 0) {
      return { success: false, error: 'canceled' };
    }
    
    const folderPath = result.filePaths[0];
    const filename = path.basename(folderPath);
    try {
      return importAvatarFolderLogic(folderPath, filename);
    } catch(e) {
      return { success: false, error: e.message };
    }
  });

  // Delete avatar model
  ipcMain.handle('delete-avatar-model', async (_event, payload) => {
    const { filename } = payload || {};
    if (!filename) return { success: false, error: 'Thiếu filename' };

    try {
      const safeFilename = path.normalize(filename).replace(/^(\.\.[\/\\])+/, '');
      const is3d = safeFilename.endsWith('.vrm') || safeFilename.endsWith('.fbx');
      const subDir = is3d ? 'vrm' : 'live2d';
      const baseRoot = path.join(__dirname, 'public', 'models', subDir);
      const targetPath = path.join(baseRoot, safeFilename);

      const parentDir = path.dirname(targetPath);
      
      if (parentDir !== baseRoot) {
        // Model is inside a subfolder (imported via folder)
        if (!fs.existsSync(parentDir) && !fs.existsSync(targetPath)) {
          return { success: false, error: 'Model không tồn tại' };
        }
        try {
          fs.rmSync(parentDir, { recursive: true, force: true, maxRetries: 5, retryDelay: 300 });
        } catch (err) {
          // If Windows still throws ENOTEMPTY after retries, the main files are likely gone.
          if (err.code !== 'ENOTEMPTY' && err.code !== 'EBUSY' && err.code !== 'EPERM') {
            throw err;
          }
        }
      } else {
        // Model is just a single file in the root
        if (!fs.existsSync(targetPath)) {
          return { success: false, error: 'Model không tồn tại' };
        }
        try {
          fs.rmSync(targetPath, { force: true, maxRetries: 5, retryDelay: 300 });
        } catch (err) {
          if (err.code !== 'EBUSY' && err.code !== 'EPERM') {
            throw err;
          }
        }
      }
      return { success: true };
    } catch (e) {
      return { success: false, error: e.message };
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

// Second Instance Guard: focus existing widget khi bản sao bị chặn
app.on('second-instance', () => {
  if (widgetWindow) {
    if (widgetWindow.isMinimized()) widgetWindow.restore();
    widgetWindow.focus();
  }
});

app.whenReady().then(async () => {
  setupIPC();

  // [AUTO-VITE] Khởi động Vite dev server TRƯỚC khi tạo windows
  if (isDev) {
    try {
      await spawnViteServer();
      await waitForVite(5173);
      console.log('[Electron] Vite ready! Đang tạo cửa sổ...');
    } catch (err) {
      console.error('[Electron] Lỗi khởi động Vite:', err.message);
      console.error('[Electron] Không thể tiếp tục. Vui lòng chạy "npm run dev" trong terminal riêng.');
    }
  }

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
function scheduleElectronReload(reason) {
  if (!isDev) return;
  if (restartTimer) clearTimeout(restartTimer);
  restartTimer = setTimeout(() => {
    console.log(`[Electron] Restart requested: ${reason}`);
    if (widgetWindow && !widgetWindow.isDestroyed()) widgetWindow.close();
    if (dashboardWindow && !dashboardWindow.isDestroyed()) dashboardWindow.close();
    app.relaunch();
    app.exit(0);
  }, 250);
}

app.on('will-quit', () => {
  // [AUTO-VITE] Kill Vite dev server
  if (viteServer && !viteServer.killed) {
    console.log('[Electron] Đang tắt Vite dev server...');
    if (isWindows) {
      spawn('taskkill', ['/pid', String(viteServer.pid), '/f', '/t'], { stdio: 'ignore' });
    } else {
      viteServer.kill('SIGTERM');
    }
  }

  // [ARCH] Disabled cleanup hooks. Gateway and AI Engine are now independently managed by start_all.bat
  /*
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
    } catch (e) { }
  }
  */
});
