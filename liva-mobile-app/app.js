// Mobile App State
let ws = null;
let currentPath = "/";

// DOM Elements
const chatContainer = document.getElementById('chatContainer');
const cmdInput = document.getElementById('cmdInput');
const sendBtn = document.getElementById('sendBtn');
const connectionStatus = document.getElementById('connectionStatus');
const connectBtn = document.getElementById('connectBtn');

// Tabs
const navItems = document.querySelectorAll('.nav-item');
const tabContents = document.querySelectorAll('.tab-content');

// Settings
const ipInput = document.getElementById('ipInput');
const portInput = document.getElementById('portInput');
const saveSettingsBtn = document.getElementById('saveSettingsBtn');

// Navigation Logic
navItems.forEach(item => {
    item.addEventListener('click', () => {
        navItems.forEach(nav => nav.classList.remove('active'));
        item.classList.add('active');
        
        tabContents.forEach(tab => {
            tab.classList.remove('active');
            tab.classList.add('hidden');
        });
        
        const target = document.getElementById(item.getAttribute('data-target'));
        target.classList.remove('hidden');
        target.classList.add('active');
    });
});

function appendMessage(text, type) {
    const msgDiv = document.createElement('div');
    msgDiv.className = `message ${type}`;
    
    // Parse markdown code blocks naively
    if (text.includes('```')) {
        const parts = text.split('```');
        let html = '';
        for (let i = 0; i < parts.length; i++) {
            if (i % 2 !== 0) {
                // Remove lang identifier like 'bash'
                const code = parts[i].replace(/^[a-z]+\n/i, '');
                html += `<pre>${code}</pre>`;
            } else {
                html += parts[i].replace(/\n/g, '<br>');
            }
        }
        msgDiv.innerHTML = html;
    } else {
        msgDiv.innerText = text;
    }
    
    chatContainer.appendChild(msgDiv);
    chatContainer.scrollTop = chatContainer.scrollHeight;
}

function connect() {
    if (ws && ws.readyState === WebSocket.OPEN) {
        ws.close();
        return;
    }
    
    const ip = ipInput.value.trim() || '127.0.0.1';
    const port = portInput.value.trim() || '8082';
    
    appendMessage(`Đang kết nối tới ${ip}:${port}...`, 'system-msg');
    
    try {
        ws = new WebSocket(`ws://${ip}:${port}`);
        
        ws.onopen = () => {
            connectionStatus.classList.remove('disconnected');
            connectionStatus.classList.add('connected');
            appendMessage('Đã kết nối thành công!', 'system-msg');
            
            // Auto fetch root dir
            ws.send(JSON.stringify({ event: 'explorer_ls', payload: { path: '/' } }));
        };
        
        ws.onmessage = (event) => {
            try {
                const data = JSON.parse(event.data);
                
                if (data.event === 'agent_state' && data.payload.response) {
                    appendMessage(data.payload.response, 'agent-msg');
                } 
                else if (data.event === 'system_msg') {
                    appendMessage(data.payload.text, 'system-msg');
                }
                else if (data.event === 'explorer_ls_result') {
                    renderExplorer(data.payload.path, data.payload.files);
                }
                else if (data.event === 'explorer_cat_result') {
                    appendMessage(`📄 ${data.payload.path}\n\`\`\`\n${data.payload.content}\n\`\`\``, 'agent-msg');
                }
                else if (data.event === 'explorer_error') {
                    appendMessage(`❌ Lỗi Explorer: ${data.payload.error}`, 'system-msg');
                }
            } catch (e) {
                console.error("Parse WS message error:", e);
            }
        };
        
        ws.onclose = () => {
            connectionStatus.classList.remove('connected');
            connectionStatus.classList.add('disconnected');
            appendMessage('Đã mất kết nối.', 'system-msg');
        };
        
        ws.onerror = () => {
            appendMessage('Lỗi kết nối WebSocket. Kiểm tra lại IP/Port.', 'system-msg');
        };
    } catch (e) {
        appendMessage('URL WebSocket không hợp lệ.', 'system-msg');
    }
}

function sendCommand() {
    const text = cmdInput.value.trim();
    if (!text || !ws || ws.readyState !== WebSocket.OPEN) return;
    
    appendMessage(text, 'user-msg');
    cmdInput.value = '';
    
    // Check if it's a local command
    if (text.startsWith('/ls')) {
        const p = text.split(' ')[1] || currentPath;
        ws.send(JSON.stringify({ event: 'explorer_ls', payload: { path: p } }));
    } else if (text.startsWith('/cat')) {
        const p = text.split(' ')[1];
        if (p) {
            ws.send(JSON.stringify({ event: 'explorer_cat', payload: { path: p } }));
        }
    } else {
        ws.send(JSON.stringify({ event: 'user_voice_command', payload: { text } }));
    }
}

sendBtn.addEventListener('click', sendCommand);
cmdInput.addEventListener('keypress', (e) => {
    if (e.key === 'Enter') sendCommand();
});
connectBtn.addEventListener('click', connect);
saveSettingsBtn.addEventListener('click', () => {
    localStorage.setItem('liva_ip', ipInput.value);
    connect();
});

// Explorer Rendering
function renderExplorer(path, files) {
    currentPath = path;
    document.getElementById('currentPath').innerText = path;
    
    const fileList = document.getElementById('fileList');
    fileList.innerHTML = '';
    
    if (path !== '/' && path !== '') {
        const upItem = document.createElement('div');
        upItem.className = 'file-item';
        upItem.innerHTML = `<span class="file-icon">🔙</span><span class="file-name">.. (Lên 1 cấp)</span>`;
        upItem.onclick = () => {
            const parts = path.split('/');
            parts.pop();
            const parent = parts.join('/') || '/';
            ws.send(JSON.stringify({ event: 'explorer_ls', payload: { path: parent } }));
        };
        fileList.appendChild(upItem);
    }
    
    if (files.length === 0) {
        fileList.innerHTML += `<div class="empty-state">Thư mục trống</div>`;
        return;
    }
    
    files.forEach(f => {
        const item = document.createElement('div');
        item.className = 'file-item';
        const icon = f.isDirectory ? '📁' : '📄';
        const sizeStr = f.isDirectory ? '' : `<span class="file-size">${(f.size/1024).toFixed(1)}kb</span>`;
        item.innerHTML = `<span class="file-icon">${icon}</span><span class="file-name">${f.name}</span>${sizeStr}`;
        
        item.onclick = () => {
            const targetPath = currentPath === '/' ? f.name : `${currentPath}/${f.name}`;
            if (f.isDirectory) {
                ws.send(JSON.stringify({ event: 'explorer_ls', payload: { path: targetPath } }));
            } else {
                ws.send(JSON.stringify({ event: 'explorer_cat', payload: { path: targetPath } }));
                // Switch to terminal tab
                navItems[0].click();
            }
        };
        fileList.appendChild(item);
    });
}

// Load saved settings
const savedIp = localStorage.getItem('liva_ip');
if (savedIp) {
    ipInput.value = savedIp;
} else if (location.hostname && location.hostname !== 'localhost' && location.hostname !== '127.0.0.1') {
    ipInput.value = location.hostname; // Auto detect if served from same IP
}
