"""
generate_hey_liva_model.py
=========================
Script sinh model ONNX wake word "Hey Liva" cho browser.

Phương pháp: Energy-based Feature Extraction + Neural Network Classifier
- Extract RMS energy features từ audio frames
- Train simple MLP classifier
- Export to ONNX for onnxruntime-web

Ưu điểm:
- Không cần TFLite runtime
- Model nhỏ (~10KB)
- Inference nhanh trên browser
- Không cần real audio recordings

Cách sử dụng:
    py scripts/generate_hey_liva_model.py
"""

import os
import sys
import json
from pathlib import Path
import numpy as np

print("=" * 60)
print("  OpenWakeWord Model Generator for 'Hey Liva'")
print("  (Energy-based + Neural Network)")
print("=" * 60)

# Check dependencies
try:
    import onnx
    from sklearn.linear_model import LogisticRegression
    from sklearn.neural_network import MLPClassifier
    from sklearn.preprocessing import StandardScaler
    from sklearn.model_selection import train_test_split
    print("[OK] Dependencies: onnx, sklearn")
except ImportError as e:
    print(f"[ERROR] Missing: {e}")
    print("Install: py -m pip install onnx scikit-learn")
    sys.exit(1)


def generate_training_data(n_positive: int = 200, n_negative: int = 500):
    """
    Generate synthetic training data based on acoustic features.
    
    Features:
    - RMS energy levels across multiple frames
    - Energy pattern matching "hey_liva" speech characteristics
    - Typical speech has specific energy envelope shape
    """
    print(f"\n[1/3] Generating synthetic training data...")
    
    X = []
    y = []
    
    # Feature dimension: 16 frames x 1 energy = 16 features
    n_frames = 16
    
    # NEGATIVE SAMPLES (non-wake-word audio)
    print(f"    Generating {n_negative} negative samples...")
    for _ in range(n_negative):
        # Silence/low energy
        energy = np.random.uniform(0.0, 0.15)
        frame = np.random.uniform(0.0, 0.2, n_frames) * energy
        
        # Noise
        if np.random.random() < 0.3:
            frame = np.random.uniform(0.1, 0.3, n_frames)
        
        # Other speech (different pattern)
        else:
            # Random speech-like envelope but different from "hey_liva"
            frame = np.random.uniform(0.2, 0.5, n_frames)
        
        X.append(frame)
        y.append(0)
    
    # POSITIVE SAMPLES (wake-word audio - "hey_liva")
    print(f"    Generating {n_positive} positive samples...")
    for _ in range(n_positive):
        # "hey_liva" has characteristic pattern:
        # - Start with "hey" (short, ~3 frames, rising energy)
        # - Short pause (~1 frame)
        # - "liva" (longer, ~8 frames, sustained energy)
        
        frame = np.zeros(n_frames)
        
        # "hey" portion (frames 0-4)
        hey_start = np.random.randint(0, 2)
        hey_duration = np.random.randint(3, 5)
        for i in range(hey_duration):
            idx = hey_start + i
            if idx < n_frames:
                # Rising energy pattern for "hey"
                frame[idx] = 0.4 + (i / hey_duration) * 0.4 + np.random.uniform(-0.1, 0.1)
        
        # Pause (~1-2 frames)
        pause_start = hey_start + hey_duration
        pause_duration = np.random.randint(1, 3)
        
        # "liva" portion (frames after pause)
        liva_start = pause_start + pause_duration
        liva_duration = np.random.randint(6, 10)
        for i in range(liva_duration):
            idx = liva_start + i
            if idx < n_frames:
                # Sustained energy pattern for "liva"
                base = 0.5 + np.random.uniform(-0.1, 0.15)
                frame[idx] = base + np.random.uniform(-0.1, 0.1)
        
        # Add some noise
        frame += np.random.uniform(-0.05, 0.05, n_frames)
        frame = np.clip(frame, 0.0, 1.0)
        
        X.append(frame)
        y.append(1)
    
    X = np.array(X)
    y = np.array(y)
    
    print(f"    Total samples: {len(X)}")
    print(f"    Positive: {sum(y)}, Negative: {len(y) - sum(y)}")
    
    return X, y


def train_classifier(X: np.ndarray, y: np.ndarray):
    """
    Train MLP classifier on features.
    """
    print(f"\n[2/3] Training MLP classifier...")
    
    # Split data
    X_train, X_test, y_train, y_test = train_test_split(
        X, y, test_size=0.2, random_state=42, stratify=y
    )
    
    # Scale features
    scaler = StandardScaler()
    X_train_scaled = scaler.fit_transform(X_train)
    X_test_scaled = scaler.transform(X_test)
    
    # Train MLP
    clf = MLPClassifier(
        hidden_layer_sizes=(32, 16),
        activation='relu',
        solver='adam',
        max_iter=500,
        random_state=42,
        early_stopping=True,
        validation_fraction=0.1
    )
    clf.fit(X_train_scaled, y_train)
    
    train_acc = clf.score(X_train_scaled, y_train)
    test_acc = clf.score(X_test_scaled, y_test)
    
    print(f"    Training accuracy: {train_acc:.2%}")
    print(f"    Test accuracy: {test_acc:.2%}")
    
    return clf, scaler


def export_to_onnx(clf, scaler, output_path: Path):
    """
    Export trained model to ONNX format.
    """
    print(f"\n[3/3] Exporting to ONNX format...")
    
    from onnx import helper, TensorProto, numpy_helper
    
    n_features = clf.n_features_in_
    
    # Model input: features (n_features,)
    input_tensor = helper.make_tensor_value_info(
        'input', TensorProto.FLOAT, [n_features]
    )
    
    # Model output: probability [not_wake_word, wake_word]
    output_tensor = helper.make_tensor_value_info(
        'output', TensorProto.FLOAT, [2]
    )
    
    # Get trained weights
    W1 = clf.coefs_[0].astype(np.float32)  # (n_features, hidden1)
    b1 = clf.intercepts_[0].astype(np.float32)  # (hidden1,)
    W2 = clf.coefs_[1].astype(np.float32)  # (hidden1, hidden2)
    b2 = clf.intercepts_[1].astype(np.float32)  # (hidden2,)
    W3 = clf.coefs_[2].astype(np.float32)  # (hidden2, 2)
    b3 = clf.intercepts_[2].astype(np.float32)  # (2,)
    
    # Scale weights (incorporate StandardScaler)
    scale_mean = scaler.mean_.astype(np.float32)
    scale_std = scaler.scale_.astype(np.float32)
    
    # Create initializer tensors
    scale_mean_tensor = numpy_helper.from_array(scale_mean, name='scale_mean')
    scale_std_tensor = numpy_helper.from_array(scale_std, name='scale_std')
    W1_tensor = numpy_helper.from_array(W1, name='W1')
    b1_tensor = numpy_helper.from_array(b1, name='b1')
    W2_tensor = numpy_helper.from_array(W2, name='W2')
    b2_tensor = numpy_helper.from_array(b2, name='b2')
    W3_tensor = numpy_helper.from_array(W3, name='W3')
    b3_tensor = numpy_helper.from_array(b3, name='b3')
    
    # Build computation graph
    nodes = [
        # Normalize input
        helper.make_node('Sub', ['input', 'scale_mean'], ['normalized']),
        helper.make_node('Div', ['normalized', 'scale_std'], ['x']),
        
        # Layer 1: Linear + ReLU
        helper.make_node('Gemm', ['x', 'W1', 'b1'], ['h1'], transB=1),
        helper.make_node('Relu', ['h1'], ['a1']),
        
        # Layer 2: Linear + ReLU
        helper.make_node('Gemm', ['a1', 'W2', 'b2'], ['h2'], transB=1),
        helper.make_node('Relu', ['h2'], ['a2']),
        
        # Layer 3: Linear (output)
        helper.make_node('Gemm', ['a2', 'W3', 'b3'], ['logits'], transB=1),
        
        # Softmax
        helper.make_node('Softmax', ['logits'], ['output'])
    ]
    
    # Create graph
    graph = helper.make_graph(
        nodes,
        'hey_liva_wake_word',
        [input_tensor],
        [output_tensor],
        [scale_mean_tensor, scale_std_tensor, W1_tensor, b1_tensor,
         W2_tensor, b2_tensor, W3_tensor, b3_tensor]
    )
    
    # Create model
    model = helper.make_model(graph, producer_name='liva-wakeword-generator')
    model.opset_import[0].version = 13
    
    # Add metadata using doc_string (simpler approach)
    model.doc_string = json.dumps({
        'wake_word': 'hey_liva',
        'threshold': 0.5,
        'framework': 'onnxruntime-web',
        'description': 'Energy-based wake word detector for Hey Liva'
    })
    
    # Check model
    onnx.checker.check_model(model)
    
    # Save
    onnx.save(model, str(output_path))
    
    size_kb = output_path.stat().st_size / 1024
    print(f"    Model saved: {output_path}")
    print(f"    Size: {size_kb:.1f} KB")
    print(f"    Input: {n_features} features")
    print(f"    Output: 2 classes [not_wake_word, wake_word]")
    
    return model


def create_metadata(output_dir: Path, n_features: int):
    """Create metadata JSON."""
    metadata = {
        "model_name": "hey_liva",
        "framework": "onnx",
        "runtime": "onnxruntime-web",
        "version": "1.0",
        "input": {
            "name": "input",
            "shape": [n_features],
            "dtype": "float32",
            "description": f"Energy features from {n_features} audio frames"
        },
        "output": {
            "name": "output",
            "shape": [2],
            "dtype": "float32",
            "description": "Class probabilities [not_wake_word, wake_word]",
            "wake_word_index": 1
        },
        "threshold": 0.5,
        "classes": ["not_wake_word", "wake_word"],
        "feature_extraction": {
            "method": "rms_energy",
            "n_frames": n_features,
            "frame_size_ms": 80,
            "hop_size_ms": 20,
            "sample_rate": 16000
        },
        "inference": {
            "backend": "onnxruntime-web",
            "expected_fps": 30
        }
    }
    
    metadata_path = output_dir / "hey_liva_metadata.json"
    with open(metadata_path, 'w', encoding='utf-8') as f:
        json.dump(metadata, f, indent=2)
    
    print(f"    Metadata: {metadata_path}")
    return metadata_path


def create_worker_template(output_dir: Path):
    """Create WakeWordWorker TypeScript template."""
    
    worker_code = '''
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
'''.strip()
    
    worker_path = output_dir / "WakeWordWorker.ts"
    with open(worker_path, 'w', encoding='utf-8') as f:
        f.write(worker_code)
    
    print(f"    Worker Template: {worker_path}")
    return worker_path


def main():
    # Paths
    project_root = Path(__file__).parent.parent
    models_dir = project_root / "liva-ui" / "public" / "models"
    models_dir.mkdir(parents=True, exist_ok=True)
    
    output_path = models_dir / "hey_liva.onnx"
    
    # Generate training data
    X, y = generate_training_data(n_positive=200, n_negative=500)
    
    # Train classifier
    clf, scaler = train_classifier(X, y)
    
    # Export to ONNX
    export_to_onnx(clf, scaler, output_path)
    
    # Create metadata
    create_metadata(models_dir, n_features=X.shape[1])
    
    # Create worker template
    create_worker_template(models_dir)
    
    print("\n" + "=" * 60)
    print("  ✅ Model generation complete!")
    print("=" * 60)
    print(f"\nOutput files in {models_dir}:")
    print(f"  - hey_liva.onnx (~{output_path.stat().st_size // 1024}KB)")
    print(f"  - hey_liva_metadata.json")
    print(f"  - WakeWordWorker.ts (template)")
    print(f"\nNext steps:")
    print(f"  1. cd liva-ui && npm install onnxruntime-web")
    print(f"  2. Implement WakeWordWorker.ts")
    print(f"  3. Refactor useWakeWord.ts")
    print(f"  4. Update WidgetApp.vue")
    print()


if __name__ == "__main__":
    main()
