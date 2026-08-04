import { encodePcm16Wav, resampleLinear } from './wake-enrollment-core.js';

const logEl = document.getElementById('log');
const statusEl = document.getElementById('status');
const energyBar = document.getElementById('energyBar');
const confidenceDisplay = document.getElementById('confidenceDisplay');
const recordSampleButton = document.getElementById('btnRecordSample');
const enrollmentCount = document.getElementById('enrollmentCount');
const recordNegativeButton = document.getElementById('btnRecordNegative');
const negativeEnrollmentCount = document.getElementById('negativeEnrollmentCount');
const negativePrompt = document.getElementById('negativePrompt');

function log(msg, type = 'info') {
  const entry = document.createElement('div');
  entry.className = 'log-entry log-' + type;
  entry.textContent = '[' + new Date().toLocaleTimeString() + '] ' + msg;
  logEl.appendChild(entry);
  logEl.scrollTop = logEl.scrollHeight;
  console.log('[WakeWordTest] ' + msg);
}

function setStatus(text, cls) {
  statusEl.textContent = text;
  statusEl.className = 'status ' + cls;
}

function setConfidence(confidence) {
  confidenceDisplay.textContent = confidence.toFixed(1) + '%';
  const level = confidence > 50 ? 'high' : confidence > 25 ? 'medium' : 'low';
  confidenceDisplay.className = 'confidence-display confidence-' + level;
}

let isListening = false;
let mediaStream = null;
let audioContext = null;
let processor = null;
let lastDetection = 0;
let consecutiveHighEnergy = 0;
let enrollmentChunks = [];
let enrollmentSamples = 0;
let enrollmentSampleRate = 16000;
let enrollmentKind = null;
let downloadedPositiveSamples = 0;
let downloadedNegativeSamples = 0;

const ENERGY_THRESHOLD = 0.1;
const REQUIRED_DURATION = 1500;
const COOLDOWN = 2000;
const ENROLLMENT_SECONDS = 2.5;
const ENROLLMENT_TARGET = 20;
const NEGATIVE_PROMPTS = [
  'Liva',
  'Hey Diva',
  'Hey Lina',
  'Play video',
  'Này Ly à',
  'Đi về đi',
  'Hôm nay trời đẹp',
  'Mở nhạc giúp tôi',
  'Nhắn tin cho Minh Hiển',
  'Chiều nay đi bắt Pokémon không',
];

function finishEnrollmentSample() {
  const kind = enrollmentKind;
  if (!kind) return;
  const joined = new Float32Array(enrollmentSamples);
  let offset = 0;
  for (const chunk of enrollmentChunks) {
    joined.set(chunk, offset);
    offset += chunk.length;
  }
  const exactLength = Math.min(
    joined.length,
    Math.round(enrollmentSampleRate * ENROLLMENT_SECONDS)
  );
  const pcm16k = resampleLinear(joined.subarray(0, exactLength), enrollmentSampleRate, 16000);
  const wav = encodePcm16Wav(pcm16k, 16000);
  if (kind === 'positive') downloadedPositiveSamples += 1;
  else downloadedNegativeSamples += 1;
  const count = kind === 'positive' ? downloadedPositiveSamples : downloadedNegativeSamples;
  const sequence = String(count).padStart(2, '0');
  const link = document.createElement('a');
  const url = URL.createObjectURL(new Blob([wav], { type: 'audio/wav' }));
  link.href = url;
  link.download =
    kind === 'positive' ? `hey_liva_positive_${sequence}.wav` : `hey_liva_negative_${sequence}.wav`;
  link.click();
  setTimeout(() => URL.revokeObjectURL(url), 1000);

  enrollmentKind = null;
  enrollmentChunks = [];
  enrollmentSamples = 0;
  recordSampleButton.disabled = false;
  recordNegativeButton.disabled = false;
  enrollmentCount.textContent = `${downloadedPositiveSamples}/${ENROLLMENT_TARGET} mẫu đã tải`;
  negativeEnrollmentCount.textContent = `${downloadedNegativeSamples}/${ENROLLMENT_TARGET} mẫu negative đã tải`;
  negativePrompt.textContent = `“${NEGATIVE_PROMPTS[downloadedNegativeSamples % NEGATIVE_PROMPTS.length]}”`;
  setStatus(`Đã tải mẫu ${kind} ${sequence}`, 'success');
  log(`Đã xuất ${link.download}: PCM16 mono 16 kHz`, 'success');
}

async function recordEnrollmentSample(kind) {
  if (enrollmentKind) return;
  if (!isListening) await startListening();
  if (!isListening || !audioContext) return;

  enrollmentKind = kind;
  enrollmentChunks = [];
  enrollmentSamples = 0;
  enrollmentSampleRate = audioContext.sampleRate;
  recordSampleButton.disabled = true;
  recordNegativeButton.disabled = true;
  const prompt =
    kind === 'positive'
      ? 'nói “Hey Liva” ngay bây giờ'
      : `đọc ${NEGATIVE_PROMPTS[downloadedNegativeSamples % NEGATIVE_PROMPTS.length]} nhưng không nói câu gọi`;
  setStatus(`Đang ghi 2,5 giây — ${prompt}`, 'listening');
  log(`Bắt đầu ghi một mẫu enrollment ${kind} theo yêu cầu người dùng`, 'info');
}

function flashDetection(duration = 300) {
  energyBar.classList.add('detected');
  setTimeout(() => energyBar.classList.remove('detected'), duration);
}

function playDing() {
  try {
    const ctx = new AudioContext();
    const osc = ctx.createOscillator();
    const gain = ctx.createGain();

    osc.connect(gain);
    gain.connect(ctx.destination);
    osc.frequency.value = 880;
    gain.gain.setValueAtTime(0.3, ctx.currentTime);
    gain.gain.exponentialRampToValueAtTime(0.001, ctx.currentTime + 0.15);
    osc.start();
    osc.stop(ctx.currentTime + 0.15);

    setTimeout(() => {
      const osc2 = ctx.createOscillator();
      const gain2 = ctx.createGain();
      osc2.connect(gain2);
      gain2.connect(ctx.destination);
      osc2.frequency.value = 1100;
      gain2.gain.setValueAtTime(0.2, ctx.currentTime);
      gain2.gain.exponentialRampToValueAtTime(0.001, ctx.currentTime + 0.1);
      osc2.start();
      osc2.stop(ctx.currentTime + 0.1);
    }, 150);
  } catch (e) {
    log('Ding error: ' + e.message, 'warn');
  }
}

async function startListening() {
  if (isListening) return;

  if (!navigator.mediaDevices?.getUserMedia) {
    log('ERROR: getUserMedia không được hỗ trợ', 'error');
    setStatus('Lỗi: Trình duyệt không hỗ trợ microphone', 'error');
    return;
  }

  log('Yêu cầu quyền microphone...', 'info');
  try {
    mediaStream = await navigator.mediaDevices.getUserMedia({
      audio: {
        channelCount: 1,
        sampleRate: 16000,
        echoCancellation: true,
        noiseSuppression: true,
        autoGainControl: true,
      },
    });
    log('Microphone đã được cấp quyền!', 'success');

    audioContext = new AudioContext({ sampleRate: 16000 });
    const source = audioContext.createMediaStreamSource(mediaStream);
    processor = audioContext.createScriptProcessor(4096, 1, 1);

    processor.onaudioprocess = (event) => {
      if (!isListening) return;
      const inputData = event.inputBuffer.getChannelData(0);
      if (enrollmentKind) {
        const copy = new Float32Array(inputData);
        enrollmentChunks.push(copy);
        enrollmentSamples += copy.length;
        if (enrollmentSamples >= enrollmentSampleRate * ENROLLMENT_SECONDS) {
          finishEnrollmentSample();
        }
      }
      const now = Date.now();
      if (now - lastDetection < COOLDOWN) {
        consecutiveHighEnergy = 0;
        return;
      }

      let sumSquares = 0;
      for (const sample of inputData) sumSquares += sample * sample;
      const rms = Math.sqrt(sumSquares / inputData.length);
      energyBar.value = Math.min(100, rms * 500);
      setConfidence(Math.min(100, rms * 1000));

      if (rms > ENERGY_THRESHOLD) {
        consecutiveHighEnergy += 16;
        if (consecutiveHighEnergy >= REQUIRED_DURATION) {
          lastDetection = now;
          consecutiveHighEnergy = 0;
          log('🎉 VOICE ACTIVITY DETECTED! (Energy: ' + rms.toFixed(4) + ')', 'detect');
          setStatus('VOICE ACTIVITY DETECTED! Energy: ' + rms.toFixed(4), 'success');
          playDing();
          flashDetection();
        }
      } else {
        consecutiveHighEnergy = 0;
      }
    };

    source.connect(processor);
    processor.connect(audioContext.destination);
    isListening = true;
    setStatus('🔴 Đang lắng nghe câu bắt đầu bằng “Hey Liva”...', 'listening');
    log('Bắt đầu đo năng lượng microphone!', 'success');
    log('Ngưỡng energy: ' + ENERGY_THRESHOLD, 'info');
    log('Thời gian cần thiết: ' + REQUIRED_DURATION + 'ms', 'info');
    document.getElementById('btnStart').disabled = true;
    document.getElementById('btnStop').disabled = false;
  } catch (err) {
    log('ERROR: ' + err.message, 'error');
    setStatus('Lỗi microphone: ' + err.message, 'error');
  }
}

function stopListening() {
  isListening = false;
  consecutiveHighEnergy = 0;
  enrollmentKind = null;
  enrollmentChunks = [];
  enrollmentSamples = 0;
  recordSampleButton.disabled = false;
  recordNegativeButton.disabled = false;
  processor?.disconnect();
  processor = null;
  audioContext?.close();
  audioContext = null;
  mediaStream?.getTracks().forEach((track) => track.stop());
  mediaStream = null;
  setStatus('Đã dừng', 'pending');
  log('Đã dừng lắng nghe', 'warn');
  energyBar.value = 0;
  confidenceDisplay.textContent = '-';
  confidenceDisplay.className = 'confidence-display confidence-low';
  document.getElementById('btnStart').disabled = false;
  document.getElementById('btnStop').disabled = true;
}

document.getElementById('btnStart').addEventListener('click', startListening);
document.getElementById('btnStop').addEventListener('click', stopListening);
recordSampleButton.addEventListener('click', () => recordEnrollmentSample('positive'));
recordNegativeButton.addEventListener('click', () => recordEnrollmentSample('negative'));
document.getElementById('btnTest').addEventListener('click', () => {
  log('Manual test: Simulating voice activity detection', 'warn');
  setStatus('TEST: Voice activity detected!', 'success');
  playDing();
  energyBar.value = 80;
  flashDetection(500);
  setTimeout(() => {
    energyBar.value = 0;
  }, 500);
});

setStatus('Sẵn sàng - nhấn “Bắt đầu kiểm tra mic”', 'pending');
log('Ready to start microphone energy detection', 'info');
