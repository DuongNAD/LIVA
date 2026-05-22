/**
 * WakeWordWorker.ts — ONNX Runtime Web Worker for "Hey Liva" Wake Word Detection
 * =================================================================================
 * 
 * Architecture:
 * - Runs in Web Worker thread to avoid blocking main thread
 * - Loads ONNX model (~5KB) via onnxruntime-web
 * - Extracts RMS energy features from audio frames
 * - Runs continuous inference at ~30fps
 * - Posts message to main thread when wake word is detected
 * 
 * Features:
 * - Zero Backend CPU/GPU when idle
 * - 100% local processing (privacy-first)
 * - Self-wake prevention (pause/resume)
 * - Memory cleanup on termination
 */

import weights from './hey_liva_weights.json';

function log(level: "info" | "warn" | "error", ...args: unknown[]) {
  self.postMessage({ type: '__log', level, args });
}

// ============================================================================
// Configuration
// ============================================================================

interface WakeWordConfig {
  modelPath: string;
  threshold: number;
  wakeWordIndex: number;
  cooldownMs: number;
  frameSizeMs: number;
  hopSizeMs: number;
  nFrames: number;
  sampleRate: number;
}

const DEFAULT_CONFIG: WakeWordConfig = {
  modelPath: '/models/hey_liva.onnx',
  threshold: 0.2,
  wakeWordIndex: 1,
  cooldownMs: 1500,
  frameSizeMs: 80,
  hopSizeMs: 20,
  nFrames: 16,
  sampleRate: 16000,
};

// ============================================================================
// State
// ============================================================================

let config: WakeWordConfig = { ...DEFAULT_CONFIG };
let lastDetectionTime = 0;
let isReady = false;
let isPaused = false;

// ============================================================================
// ONNX Model Loading (Replaced by Native JS implementation)
// ============================================================================

async function loadModel(): Promise<boolean> {
  try {
    log('info', '[WakeWordWorker] Initializing Native JS Neural Network Engine...');
    
    // We bypass WASM and load the weights directly from JSON
    // This fixes the Emscripten 8524768 memory crash and Vite cache issues.
    
    log('info', '[WakeWordWorker] Native Model loaded successfully');
    isReady = true;
    return true;
  } catch (error) {
    const errorMessage = error instanceof Error ? error.message : String(error);
    log('error', '[WakeWordWorker] Failed to initialize native model:', errorMessage);
    return false;
  }
}

// ============================================================================
// Feature Extraction (RMS Energy-based)
// ============================================================================

/**
 * Extract RMS energy features from audio buffer.
 * 
 * Features:
 * - Divide audio into frames
 * - Compute RMS energy for each frame
 * - Normalize to [0, 1] range
 */
function extractFeatures(audioBuffer: Float32Array): Float32Array {
  const frameSize = Math.floor(config.sampleRate * (config.frameSizeMs / 1000));
  const hopSize = Math.floor(config.sampleRate * (config.hopSizeMs / 1000));
  const features = new Float32Array(config.nFrames);
  
  for (let frame = 0; frame < config.nFrames; frame++) {
    const start = frame * hopSize;
    let sumSquares = 0;
    let count = 0;
    
    for (let i = 0; i < frameSize && start + i < audioBuffer.length; i++) {
      const sample = audioBuffer[start + i];
      sumSquares += sample * sample;
      count++;
    }
    
    // RMS energy
    const rms = count > 0 ? Math.sqrt(sumSquares / count) : 0;
    
    // Scale and clamp to [0, 1]
    // Typical speech: 0.1-0.5 RMS
    // Wake word: 0.3-0.8 RMS
    features[frame] = Math.min(1.0, rms * 3);
  }
  
  return features;
}

// ============================================================================
// Neural Network Inference (Native JS)
// ============================================================================

function relu(x: number): number {
  return x > 0 ? x : 0;
}

function sigmoid(x: number): number {
  return 1 / (1 + Math.exp(-x));
}

async function runInference(features: Float32Array): Promise<number> {
  if (!isReady) return 0;
  
  // 1. Normalize input
  const x = new Float32Array(16);
  for (let i = 0; i < 16; i++) {
    x[i] = (features[i] - weights.scale_mean[i]) / weights.scale_std[i];
  }
  
  // 2. Layer 1: Gemm + Relu (W1 is [16, 32], transB=1 -> W1 is treated as 32x16. 
  // We compute h1 = x * W1^T + b1 -> h1[j] = sum_i(x[i] * W1[j][i]) + b1[j]
  const h1 = new Float32Array(32);
  for (let j = 0; j < 32; j++) {
    let sum = weights.b1[j];
    for (let i = 0; i < 16; i++) {
      sum += x[i] * weights.W1[i][j]; // W1 in JSON is [16][32]
    }
    h1[j] = relu(sum);
  }
  
  // 3. Layer 2: Gemm + Relu
  const h2 = new Float32Array(16);
  for (let j = 0; j < 16; j++) {
    let sum = weights.b2[j];
    for (let i = 0; i < 32; i++) {
      sum += h1[i] * weights.W2[i][j]; // W2 in JSON is [32][16]
    }
    h2[j] = relu(sum);
  }
  
  // 4. Layer 3: Gemm + Sigmoid (Fixes Softmax bug)
  let logit = weights.b3[0];
  for (let i = 0; i < 16; i++) {
    logit += h2[i] * weights.W3[i][0]; // W3 in JSON is [16][1]
  }
  
  // Use Sigmoid instead of Softmax to actually get a probability
  const probability = sigmoid(logit);
  
  return probability;
}

// ============================================================================
// Detection Logic
// ============================================================================

const REQUIRED_SAMPLES = 6080; // 15 * 320 + 1280
const slidingWindow = new Float32Array(8192);
let windowLength = 0;

let maxConfidenceInSecond = 0;
let lastDebugLogTime = 0;

async function processAudioFrame(audioData: Float32Array): Promise<{ detected: boolean; confidence: number } | null> {
  // Skip if paused or not ready
  if (!isReady || isPaused) return null;
  
  // Skip if in cooldown
  const now = Date.now();
  if (now - lastDetectionTime < config.cooldownMs) return null;

  // Append new audio to sliding window
  const newLength = windowLength + audioData.length;
  if (newLength <= slidingWindow.length) {
    slidingWindow.set(audioData, windowLength);
    windowLength = newLength;
  } else {
    const shift = newLength - slidingWindow.length;
    slidingWindow.copyWithin(0, shift);
    slidingWindow.set(audioData, slidingWindow.length - audioData.length);
    windowLength = slidingWindow.length;
  }
  
  // Need at least 6080 samples to extract 16 frames
  if (windowLength < REQUIRED_SAMPLES) return null;
  
  // Extract features from the most recent 6080 samples
  const processingBuffer = slidingWindow.subarray(windowLength - REQUIRED_SAMPLES, windowLength);
  const features = extractFeatures(processingBuffer);
  
  // Run inference
  const confidence = await runInference(features);
  
  if (confidence > maxConfidenceInSecond) {
    maxConfidenceInSecond = confidence;
  }
  
  if (now - lastDebugLogTime > 1000) {
    log('info', `[WakeWordWorker Debug] Max confidence in last second: ${maxConfidenceInSecond.toFixed(4)}`);
    maxConfidenceInSecond = 0;
    lastDebugLogTime = now;
  }
  
  // Check threshold
  if (confidence > config.threshold) {
    lastDetectionTime = now;
    log('info', `[WakeWordWorker] Wake word detected! Confidence: ${confidence.toFixed(3)}`);
    return { detected: true, confidence };
  }
  
  return { detected: false, confidence };
}

// ============================================================================
// Message Handler
// ============================================================================

self.onmessage = async (event: MessageEvent) => {
  const { type, data } = event.data;
  
  switch (type) {
    case 'init': {
      // Initialize with custom config if provided
      if (data?.config) {
        config = { ...DEFAULT_CONFIG, ...data.config };
      }
      const success = await loadModel();
      
      self.postMessage({ type: 'ready', success });
      break;
    }
    
    case 'audio': {
      // Process incoming audio data
      // Audio data is expected to be Float32Array (PCM 16kHz mono)
      const audioData = new Float32Array(data.audio);
      const result = await processAudioFrame(audioData);
      
      if (result) {
        self.postMessage({ 
          type: 'detection', 
          ...result 
        });
      }
      break;
    }
    
    case 'features': {
      // Process pre-extracted features (for efficiency)
      const features = new Float32Array(data.features);
      const confidence = await runInference(features);
      
      const now = Date.now();
      if (confidence > config.threshold && now - lastDetectionTime >= config.cooldownMs) {
        lastDetectionTime = now;
        self.postMessage({ 
          type: 'detection', 
          detected: true, 
          confidence 
        });
      }
      break;
    }
    
    case 'pause': {
      isPaused = true;
      log('info', '[WakeWordWorker] Paused');
      self.postMessage({ type: 'paused' });
      break;
    }
    
    case 'resume': {
      isPaused = false;
      log('info', '[WakeWordWorker] Resumed');
      self.postMessage({ type: 'resumed' });
      break;
    }
    
    case 'reset': {
      lastDetectionTime = 0;
      log('info', '[WakeWordWorker] Reset');
      self.postMessage({ type: 'reset' });
      break;
    }
    
    case 'setThreshold': {
      const newThreshold = data?.threshold;
      if (typeof newThreshold === 'number' && newThreshold > 0 && newThreshold <= 1) {
        config.threshold = newThreshold;
        log('info', `[WakeWordWorker] Threshold set to: ${newThreshold}`);
        self.postMessage({ type: 'thresholdChanged', threshold: newThreshold });
      }
      break;
    }
    
    case 'terminate': {
      // Cleanup
      isReady = false;
      log('info', '[WakeWordWorker] Terminated');
      self.postMessage({ type: 'terminated' });
      self.close();
      break;
    }
  }
};

// Signal that worker is loaded
self.postMessage({ type: 'loaded' });

export {};
