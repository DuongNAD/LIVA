const THREE = require('three');
const { GLTFLoader } = require('three/examples/jsm/loaders/GLTFLoader.js');

const scene = new THREE.Scene();
const camera = new THREE.PerspectiveCamera(75, 400 / 500, 0.1, 1000);
camera.position.z = 5;

const renderer = new THREE.WebGLRenderer({ alpha: true, antialias: true });
// Điều chỉnh kích thước hiển thị 3D trừ đi phần khung chat
renderer.setSize(400, 430); 
document.getElementById('canvas-container').appendChild(renderer.domElement);

const ambientLight = new THREE.AmbientLight(0xffffff, 1);
scene.add(ambientLight);
const directionalLight = new THREE.DirectionalLight(0xffffff, 2);
directionalLight.position.set(1, 1, 2);
scene.add(directionalLight);

let currentModel;
let currentMaterial;
let isThinking = false;

function loadAvatarModel(modelPath) {
    if (!modelPath) {
        const geometry = new THREE.TorusKnotGeometry(1, 0.3, 100, 16);
        currentMaterial = new THREE.MeshNormalMaterial(); 
        currentModel = new THREE.Mesh(geometry, currentMaterial);
        scene.add(currentModel);
        return;
    }

    const loader = new GLTFLoader();
    loader.load(
        modelPath,
        function (gltf) {
            if (currentModel) scene.remove(currentModel);
            currentModel = gltf.scene;
            currentModel.scale.set(1.5, 1.5, 1.5);
            currentModel.position.y = -2;
            scene.add(currentModel);
        },
        undefined,
        function (error) {
            console.error("-> Lỗi tải mô hình:", error);
        }
    );
}

loadAvatarModel(''); 

let ws;

function connectWebSocket() {
    ws = new WebSocket('ws://localhost:8082');

    ws.onopen = () => {
        console.log('📡 Đã kết nối với hệ thống lõi Gateway!');
    };

    ws.onmessage = (message) => {
        const data = JSON.parse(message.data);
        
        if (data.event === 'ai_thinking_start') {
            isThinking = true;
            if (currentModel && currentModel.geometry) {
                currentModel.material = new THREE.MeshStandardMaterial({ color: 0xff0066, wireframe: true });
            }
        } else if (data.event === 'ai_thinking_end') {
            isThinking = false;
            if (currentModel && currentModel.geometry) {
                currentModel.material = currentMaterial;
            }
        } else if (data.event === 'ai_spoken_response') {
            const textToSpeak = data.payload.text;
            console.log('🔊 Liva nói:', textToSpeak);
            addMessageToChat('liva', textToSpeak);

            const utterance = new SpeechSynthesisUtterance(textToSpeak);
            utterance.lang = 'vi-VN';
            utterance.rate = 1.0;
            utterance.pitch = 1.2;

            const voices = window.speechSynthesis.getVoices();
            const viVoice = voices.find(v => v.lang === 'vi-VN' || v.name.includes('Vietnamese'));
            if (viVoice) {
                utterance.voice = viVoice;
            }

            window.speechSynthesis.speak(utterance);
        }
    };

    ws.onclose = () => {
        console.log('❌ Mất kết nối với Gateway. Đang thử kết nối lại sau 3 giây...');
        setTimeout(connectWebSocket, 3000);
    };

    ws.onerror = (err) => {
        console.error('❌ Lỗi kết nối WebSocket:', err.message);
        ws.close();
    };
}

connectWebSocket();

function animate() {
    requestAnimationFrame(animate);
    
    if (currentModel && currentModel.geometry) {
        const speed = isThinking ? 0.05 : 0.01;
        currentModel.rotation.x += speed;
        currentModel.rotation.y += speed;
    }

    renderer.render(scene, camera);
}

animate();

function addMessageToChat(role, text) {
    const history = document.getElementById('chat-history');
    if (!history) return;

    const msgDiv = document.createElement('div');
    msgDiv.classList.add('message');
    msgDiv.classList.add(role === 'user' ? 'user-message' : 'liva-message');
    msgDiv.textContent = text;
    history.appendChild(msgDiv);
    history.scrollTop = history.scrollHeight;
}

// ==========================================
// TÍCH HỢP TEXT CHAT (KEYBOARD INPUT)
// ==========================================
const chatInput = document.getElementById('chat-input');

chatInput.addEventListener('keydown', (event) => {
    if (event.key === 'Enter') {
        const text = chatInput.value.trim();
        if (text !== '') {
            console.log('⌨️ Anh Dương vừa gõ:', text);
            addMessageToChat('user', text);
            
            // Gửi dữ liệu qua WebSocket. 
            // Chúng ta dùng chung event 'user_voice_command' để Backend hiểu ngay lập tức!
            if (ws.readyState === WebSocket.OPEN) {
                ws.send(JSON.stringify({
                    event: 'user_voice_command',
                    payload: { text: text }
                }));
            }
            
            // Xóa nội dung trong ô sau khi gửi (Clear input field)
            chatInput.value = '';
        }
    }
});

// ==========================================
// TÍCH HỢP MICROPHONE (VOICE INPUT)
// ==========================================
const SpeechRecognition = window.SpeechRecognition || window.webkitSpeechRecognition;

if (SpeechRecognition) {
    const recognition = new SpeechRecognition();
    recognition.lang = 'vi-VN'; 
    recognition.continuous = false; 
    recognition.interimResults = false;

    // Đổi phím tắt thu âm thành F2 (hoặc tùy Anh) để không bị trùng với phím Space khi đang gõ chữ
    window.addEventListener('keydown', (event) => {
        // Tránh kích hoạt mic khi Anh đang gõ chữ trong ô chat
        if (document.activeElement === chatInput) return;

        if (event.code === 'Space') {
            console.log('🎤 Liva đang lắng nghe (Listening)...');
            recognition.start();
        }
    });

    recognition.onresult = (event) => {
        const transcript = event.results[0][0].transcript;
        console.log('🗣️ Anh Dương nói:', transcript);
        addMessageToChat('user', transcript);
        
        if (ws.readyState === WebSocket.OPEN) {
            ws.send(JSON.stringify({
                event: 'user_voice_command',
                payload: { text: transcript }
            }));
        }
    };

    recognition.onerror = (event) => {
        console.error('❌ Lỗi Microphone (Audio Input Error):', event.error);
    };
}