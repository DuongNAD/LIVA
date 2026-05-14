// WakeWordWorker.ts
// ONNX Runtime Web Worker for "Hey Liva" Wake Word Detection
// ============================================================================

import { InferenceSession, Tensor } from 'onnxruntime-web';

const CONFIG = {
  modelPath: './models/hey_liva.onnx',
  threshold: 0.5,
  wakeWordIndex: 1,
  cooldownMs: 1500,
};

let session: InferenceSession | null = null;
let lastDetectionTime = 0;
let isReady = false;

async function loadModel(): Promise<boolean> {
  try {
    console.log('[WakeWord] Loading ONNX model...');
    session = await InferenceSession.create(CONFIG.modelPath);
    isReady = true;
    console.log('[WakeWord] Model loaded successfully');
    return true;
  } catch (error) {
    console.error('[WakeWord] Failed to load model:', error);
    return false;
  }
}

async function processAudioFrame(features: Float32Array): Promise<{ detected: boolean; confidence: number } | null> {
  if (!session || !isReady) return null;
  
  // Cooldown check
  const now = Date.now();
  if (now - lastDetectionTime < CONFIG.cooldownMs) {
    return null;
  }
  
  try {
    // Create input tensor
    const inputTensor = new Tensor('float32', features, [features.length]);
    
    // Run inference
    const outputs = await session.run([inputTensor]);
    const outputTensor = outputs[0];
    const scores = outputTensor.data as Float32Array;
    
    // Get wake word probability
    const wakeWordProb = scores[CONFIG.wakeWordIndex];
    
    if (wakeWordProb > CONFIG.threshold) {
      lastDetectionTime = now;
      console.log(`[WakeWord] Wake word detected! Confidence: ${wakeWordProb.toFixed(3)}`);
      return { detected: true, confidence: wakeWordProb };
    }
    
    return { detected: false, confidence: wakeWordProb };
  } catch (error) {
    console.error('[WakeWord] Inference error:', error);
    return null;
  }
}

// Extract RMS energy features from audio buffer
function extractFeatures(audioBuffer: Float32Array, sampleRate: number = 16000): Float32Array {
  const frameSize = Math.floor(sampleRate * 0.08); // 80ms frame
  const hopSize = Math.floor(sampleRate * 0.02);   // 20ms hop
  const nFrames = 16;
  
  const features = new Float32Array(nFrames);
  
  for (let frame = 0; frame < nFrames; frame++) {
    const start = frame * hopSize;
    let sumSquares = 0;
    let count = 0;
    
    for (let i = 0; i < frameSize && start + i < audioBuffer.length; i++) {
      const sample = audioBuffer[start + i];
      sumSquares += sample * sample;
      count++;
    }
    
    const rms = count > 0 ? Math.sqrt(sumSquares / count) : 0;
    features[frame] = Math.min(1.0, rms * 2); // Scale and clamp
  }
  
  return features;
}

// Message handler
self.onmessage = async (event: MessageEvent) => {
  const { type, data } = event.data;
  
  switch (type) {
    case 'init':
      const success = await loadModel();
      self.postMessage({ type: 'ready', success });
      break;
      
    case 'audio':
      const features = extractFeatures(new Float32Array(data.audio));
      const result = await processAudioFrame(features);
      self.postMessage({ type: 'result', ...result });
      break;
      
    case 'reset':
      lastDetectionTime = 0;
      break;
  }
};

export {};