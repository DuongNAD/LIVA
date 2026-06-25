let peerConnection;
let localStream;
let ws;

const connectBtn = document.getElementById('connect-btn');
const statusDot = document.getElementById('status-dot');
const statusText = document.getElementById('status-text');
const voiceOrb = document.getElementById('voice-orb');
const transcript = document.getElementById('transcript');

connectBtn.addEventListener('click', toggleConnection);

async function toggleConnection() {
    if (peerConnection && peerConnection.connectionState === 'connected') {
        disconnect();
    } else {
        await connect();
    }
}

async function connect() {
    try {
        statusText.textContent = "Connecting...";
        
        // 1. Get Microphone
        localStream = await navigator.mediaDevices.getUserMedia({ audio: true });
        
        // 2. Setup WebRTC
        peerConnection = new RTCPeerConnection({
            iceServers: [{ urls: 'stun:stun.l.google.com:19302' }]
        });

        localStream.getTracks().forEach(track => {
            peerConnection.addTrack(track, localStream);
        });

        peerConnection.ontrack = (event) => {
            const remoteAudio = new Audio();
            remoteAudio.srcObject = event.streams[0];
            remoteAudio.play();
        };

        peerConnection.onconnectionstatechange = () => {
            if (peerConnection.connectionState === 'connected') {
                statusDot.classList.add('connected');
                statusText.textContent = "Connected";
                connectBtn.innerHTML = `
                    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                        <rect x="3" y="3" width="18" height="18" rx="2" ry="2"></rect>
                    </svg>
                    Stop Listening
                `;
                connectBtn.classList.add('active');
                transcript.textContent = "I'm listening...";
                simulateVoiceActivity();
            } else if (peerConnection.connectionState === 'disconnected' || peerConnection.connectionState === 'failed') {
                disconnect();
            }
        };

        // 3. Create Offer
        const offer = await peerConnection.createOffer();
        await peerConnection.setLocalDescription(offer);

        // 4. Send to Signaling Server via WebSocket
        ws = new WebSocket('ws://localhost:8080');
        
        ws.onopen = () => {
            ws.send(JSON.stringify({
                type: 'offer',
                sdp: peerConnection.localDescription.sdp
            }));
        };

        ws.onmessage = async (event) => {
            const message = JSON.parse(event.data);
            if (message.type === 'answer') {
                await peerConnection.setRemoteDescription(new RTCSessionDescription({
                    type: 'answer',
                    sdp: message.sdp
                }));
            }
        };

        ws.onerror = (err) => {
            console.error("WebSocket error:", err);
            statusText.textContent = "Signaling Server Error";
            disconnect();
        };

    } catch (err) {
        console.error('Error starting connection:', err);
        statusText.textContent = "Failed to access Mic";
        transcript.textContent = "Please allow microphone access.";
    }
}

function disconnect() {
    if (peerConnection) {
        peerConnection.close();
        peerConnection = null;
    }
    if (localStream) {
        localStream.getTracks().forEach(track => track.stop());
        localStream = null;
    }
    if (ws) {
        ws.close();
        ws = null;
    }
    
    statusDot.classList.remove('connected');
    statusText.textContent = "Disconnected";
    connectBtn.innerHTML = `
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <path d="M12 2a3 3 0 0 0-3 3v7a3 3 0 0 0 6 0V5a3 3 0 0 0-3-3Z"></path>
            <path d="M19 10v2a7 7 0 0 1-14 0v-2"></path>
            <line x1="12" y1="19" x2="12" y2="22"></line>
        </svg>
        Start Listening
    `;
    connectBtn.classList.remove('active');
    voiceOrb.classList.remove('speaking');
    transcript.textContent = "Ready when you are...";
}

// Simple simulation for voice visualizer
let interval;
function simulateVoiceActivity() {
    if(interval) clearInterval(interval);
    interval = setInterval(() => {
        if (peerConnection && peerConnection.connectionState === 'connected') {
            const isSpeaking = Math.random() > 0.6; // random toggle
            if (isSpeaking) {
                voiceOrb.classList.add('speaking');
            } else {
                voiceOrb.classList.remove('speaking');
            }
        }
    }, 500);
}
