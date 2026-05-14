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

import { InferenceSession, Tensor } from 'onnxruntime-web';

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
  modelPath: './models/hey_liva.onnx',
  threshold: 0.5,
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

let session: InferenceSession | null = null;
let config: WakeWordConfig = { ...DEFAULT_CONFIG };
let lastDetectionTime = 0;
let isReady = false;
let isPaused = false;

// ============================================================================
// ONNX Model Loading
// ============================================================================

async function loadModel(modelPath: string): Promise<boolean> {
  try {
    console.log('[WakeWordWorker] Loading ONNX model:', modelPath);
    
    // Load ONNX model
    session = await InferenceSession.create(modelPath);
    
    console.log('[WakeWordWorker] Model loaded successfully');
    console.log('[WakeWordWorker] Input names:', session.inputNames);
    console.log('[WakeWordWorker] Output names:', session.outputNames);
    
    isReady = true;
    return true;
  } catch (error) {
    console.error('[WakeWordWorker] Failed to load model:', error);
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
// Inference
// ============================================================================

async function runInference(features: Float32Array): Promise<number> {
  if (!session || !isReady) return 0;
  
  try {
    // Create input tensor (1D array of features)
    const inputTensor = new Tensor('float32', features, [config.nFrames]);
    
    // Run inference - pass as object with input name
    const feeds: Record<string, Tensor> = {};
    feeds[session.inputNames[0]] = inputTensor;
    const outputs = await session.run(feeds);
    
    // Get output tensor by name
    const outputName = session.outputNames[0];
    const outputTensor = outputs[outputName];
    const scores = outputTensor.data as Float32Array;
    
    // Get wake word probability (index 1 = wake_word)
    const wakeWordProb = scores[config.wakeWordIndex];
    
    return wakeWordProb;
  } catch (error) {
    console.error('[WakeWordWorker] Inference error:', error);
    return 0;
  }
}

// ============================================================================
// Detection Logic
// ============================================================================

async function processAudioFrame(audioData: Float32Array): Promise<{ detected: boolean; confidence: number } | null> {
  // Skip if paused or not ready
  if (!isReady || isPaused) return null;
  
  // Skip if in cooldown
  const now = Date.now();
  if (now - lastDetectionTime < config.cooldownMs) return null;
  
  // Extract features
  const features = extractFeatures(audioData);
  
  // Run inference
  const confidence = await runInference(features);
  
  // Check threshold
  if (confidence > config.threshold) {
    lastDetectionTime = now;
    console.log(`[WakeWordWorker] Wake word detected! Confidence: ${confidence.toFixed(3)}`);
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
      
      const modelPath = data?.modelPath || config.modelPath;
      const success = await loadModel(modelPath);
      
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
      console.log('[WakeWordWorker] Paused');
      self.postMessage({ type: 'paused' });
      break;
    }
    
    case 'resume': {
      isPaused = false;
      console.log('[WakeWordWorker] Resumed');
      self.postMessage({ type: 'resumed' });
      break;
    }
    
    case 'reset': {
      lastDetectionTime = 0;
      console.log('[WakeWordWorker] Reset');
      self.postMessage({ type: 'reset' });
      break;
    }
    
    case 'setThreshold': {
      const newThreshold = data?.threshold;
      if (typeof newThreshold === 'number' && newThreshold > 0 && newThreshold <= 1) {
        config.threshold = newThreshold;
        console.log(`[WakeWordWorker] Threshold set to: ${newThreshold}`);
        self.postMessage({ type: 'thresholdChanged', threshold: newThreshold });
      }
      break;
    }
    
    case 'terminate': {
      // Cleanup
      if (session) {
        await session.release();
        session = null;
      }
      isReady = false;
      console.log('[WakeWordWorker] Terminated');
      self.postMessage({ type: 'terminated' });
      self.close();
      break;
    }
  }
};

// Signal that worker is loaded
self.postMessage({ type: 'loaded' });

export {};
