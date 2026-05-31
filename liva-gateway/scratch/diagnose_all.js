import { performance } from 'node:perf_hooks';
import * as net from 'node:net';

// Helper to check if a TCP port is open and listening
function checkTcpPort(host, port, timeout = 2000) {
    return new Promise((resolve) => {
        const start = performance.now();
        const socket = new net.Socket();
        
        socket.setTimeout(timeout);
        
        socket.once('connect', () => {
            const latency = performance.now() - start;
            socket.destroy();
            resolve({ open: true, latency, error: null });
        });
        
        socket.once('timeout', () => {
            socket.destroy();
            resolve({ open: false, latency: timeout, error: 'Connection timeout' });
        });
        
        socket.once('error', (err) => {
            socket.destroy();
            resolve({ open: false, latency: performance.now() - start, error: err.message });
        });
        
        socket.connect(port, host);
    });
}

// Helper to ping an HTTP endpoint
async function checkHttpEndpoint(url, options = {}, timeout = 5000) {
    const start = performance.now();
    const controller = new AbortController();
    const id = setTimeout(() => controller.abort(), timeout);
    
    try {
        const response = await fetch(url, {
            ...options,
            signal: controller.signal
        });
        const latency = performance.now() - start;
        clearTimeout(id);
        
        let data = null;
        try {
            data = await response.json();
        } catch {
            try {
                data = await response.text();
                // truncate text if too long
                if (typeof data === 'string' && data.length > 200) {
                    data = data.substring(0, 200) + '...';
                }
            } catch {
                data = '[Unreadable body]';
            }
        }
        
        return {
            ok: response.ok,
            status: response.status,
            latency,
            data,
            error: null
        };
    } catch (err) {
        clearTimeout(id);
        return {
            ok: false,
            status: 0,
            latency: performance.now() - start,
            data: null,
            error: err.name === 'AbortError' ? 'Timeout' : err.message
        };
    }
}

async function runDiagnostics() {
    console.log(JSON.stringify({ type: 'start', message: 'Starting LIVA System Diagnostics...' }));
    
    const results = {};

    // 1. Check Whisper STT (Port 8101)
    console.log(JSON.stringify({ type: 'progress', step: 'Whisper STT (Port 8101)' }));
    const whisperHealth = await checkHttpEndpoint('http://localhost:8101/health');
    const whisperRoot = await checkHttpEndpoint('http://localhost:8101/');
    results.whisper = {
        port: 8101,
        active: whisperHealth.ok || whisperRoot.ok,
        health: whisperHealth,
        root: whisperRoot,
        latency: whisperHealth.ok ? whisperHealth.latency : whisperRoot.latency
    };

    // 2. Check Native AI Engine gRPC (Port 8100)
    console.log(JSON.stringify({ type: 'progress', step: 'Native AI Engine gRPC (Port 8100)' }));
    const grpcCheck = await checkTcpPort('127.0.0.1', 8100);
    results.grpcEngine = {
        port: 8100,
        active: grpcCheck.open,
        latency: grpcCheck.latency,
        error: grpcCheck.error
    };

    // 3. Check Voice Engine (Port 8002)
    console.log(JSON.stringify({ type: 'progress', step: 'Voice Engine (Port 8002)' }));
    const voicePing = await checkTcpPort('127.0.0.1', 8002);
    let ttsCheck = { ok: false, latency: 0, error: 'Voice engine port closed' };
    
    if (voicePing.open) {
        ttsCheck = await checkHttpEndpoint('http://localhost:8002/tts', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ text: 'Xin chào Liva' })
        }, 8000);
    }
    
    results.voice = {
        port: 8002,
        active: voicePing.open,
        portLatency: voicePing.latency,
        tts: {
            ok: ttsCheck.ok && ttsCheck.data?.status === 'ok',
            status: ttsCheck.status,
            latency: ttsCheck.latency,
            responseStatus: ttsCheck.data?.status || 'unknown',
            hasAudio: !!ttsCheck.data?.audio,
            audioLength: ttsCheck.data?.audio ? ttsCheck.data.audio.length : 0,
            error: ttsCheck.error || (ttsCheck.data?.status !== 'ok' ? `TTS service status: ${ttsCheck.data?.status}` : null)
        }
    };

    // 4. Check LIVA Gateway (Port 8082)
    console.log(JSON.stringify({ type: 'progress', step: 'LIVA Gateway (Port 8082)' }));
    const gatewayCheck = await checkTcpPort('127.0.0.1', 8082);
    results.gateway = {
        port: 8082,
        active: gatewayCheck.open,
        latency: gatewayCheck.latency,
        error: gatewayCheck.error
    };

    // 5. Check UI Dev Server (Port 5173)
    console.log(JSON.stringify({ type: 'progress', step: 'UI Dev Server (Port 5173)' }));
    const uiCheck = await checkHttpEndpoint('http://localhost:5173/');
    results.ui = {
        port: 5173,
        active: uiCheck.ok,
        latency: uiCheck.latency,
        status: uiCheck.status,
        error: uiCheck.error
    };

    // 6. Check Local LLM Engine (Port 8000)
    console.log(JSON.stringify({ type: 'progress', step: 'Local LLM Engine (Port 8000)' }));
    const llmCheck = await checkHttpEndpoint('http://localhost:8000/v1/models', {}, 2000);
    results.localLlm = {
        port: 8000,
        active: llmCheck.ok,
        latency: llmCheck.latency,
        status: llmCheck.status,
        error: llmCheck.error
    };

    console.log(JSON.stringify({ type: 'result', data: results }, null, 2));
}

runDiagnostics().catch(err => {
    console.error(JSON.stringify({ type: 'error', error: err.message }));
    process.exit(1);
});
