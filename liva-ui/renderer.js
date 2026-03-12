const THREE = require('three');
const { GLTFLoader } = require('three/examples/jsm/loaders/GLTFLoader.js');

const scene = new THREE.Scene();
const camera = new THREE.PerspectiveCamera(75, 400 / 500, 0.1, 1000);
camera.position.z = 5;

const renderer = new THREE.WebGLRenderer({ alpha: true, antialias: true });
renderer.setSize(400, 500);
document.body.appendChild(renderer.domElement);

const ambientLight = new THREE.AmbientLight(0xffffff, 1);
scene.add(ambientLight);
const directionalLight = new THREE.DirectionalLight(0xffffff, 2);
directionalLight.position.set(1, 1, 2);
scene.add(directionalLight);

let currentModel;
let currentMaterial;
let isThinking = false; // Trạng thái của Liva

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

// ==========================================
// KẾT NỐI HỆ THẦN KINH (WEBSOCKET CLIENT)
// ==========================================
// Mở đường truyền kết nối tới cổng 8080 của Gateway
const ws = new WebSocket('ws://localhost:8080');

ws.onopen = () => {
    console.log('📡 Đã kết nối với hệ thống lõi Gateway!');
};

// Lắng nghe tín hiệu (Event listener)
// Lắng nghe tín hiệu (Event listener)
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
    } 
    // BỔ SUNG NHÁNH MỚI: XỬ LÝ GIỌNG NÓI (Text-to-Speech Processing)
    else if (data.event === 'ai_spoken_response') {
        const textToSpeak = data.payload.text;
        console.log('🔊 Liva nói:', textToSpeak);

        // Khởi tạo đối tượng phát âm (Utterance)
        const utterance = new SpeechSynthesisUtterance(textToSpeak);
        utterance.lang = 'vi-VN'; // Đặt ngôn ngữ tiếng Việt
        utterance.rate = 1.0;     // Tốc độ đọc bình thường
        utterance.pitch = 1.2;    // Nâng tông giọng lên một chút cho thanh thoát

        // Tùy chọn: Cố gắng tìm một giọng nữ tiếng Việt chuẩn xác nhất nếu hệ điều hành có sẵn
        const voices = window.speechSynthesis.getVoices();
        const viVoice = voices.find(v => v.lang === 'vi-VN' || v.name.includes('Vietnamese'));
        if (viVoice) {
            utterance.voice = viVoice;
        }

        // Phát âm thanh ra loa (Playback)
        window.speechSynthesis.speak(utterance);
    }
};

ws.onclose = () => {
    console.log('❌ Mất kết nối với Gateway.');
};

// ==========================================
// VÒNG LẶP HOẠT ẢNH (ANIMATION LOOP)
// ==========================================
function animate() {
    requestAnimationFrame(animate);
    
    if (currentModel && currentModel.geometry) {
        // Thuật toán: Tăng tốc độ xoay dựa trên trạng thái (State-based speed)
        const speed = isThinking ? 0.05 : 0.01;
        currentModel.rotation.x += speed;
        currentModel.rotation.y += speed;
    }

    renderer.render(scene, camera);
}

animate();

// ==========================================
// TÍCH HỢP MICROPHONE (SPEECH-TO-TEXT)
// ==========================================
// Khởi tạo bộ nhận diện giọng nói
const SpeechRecognition = window.SpeechRecognition || window.webkitSpeechRecognition;

if (SpeechRecognition) {
    const recognition = new SpeechRecognition();
    recognition.lang = 'vi-VN'; // Định cấu hình ngôn ngữ tiếng Việt
    recognition.continuous = false; // Thu âm từng câu lệnh (Command-based)
    recognition.interimResults = false;

    // Lắng nghe sự kiện nhấn phím (Keydown Event) để làm Trigger
    window.addEventListener('keydown', (event) => {
        if (event.code === 'Space') {
            console.log('🎤 Liva đang lắng nghe (Listening)...');
            recognition.start();
        }
    });

    // Khi nhận diện thành công văn bản (Result Callback)
    recognition.onresult = (event) => {
        const transcript = event.results[0][0].transcript;
        console.log('🗣️ Anh Dương nói:', transcript);
        
        // Gói dữ liệu (Payload) và gửi qua WebSocket về hệ thống Lõi
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
} else {
    console.warn('⚠️ Hệ thống không hỗ trợ Web Speech API.');
}