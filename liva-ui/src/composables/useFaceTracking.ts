/**
 * useFaceTracking.ts — Webcam Face Landmark Detection
 * =====================================================
 * Uses @mediapipe/tasks-vision FaceLandmarker to detect:
 * - 478 3D face landmarks
 * - Head pose (yaw, pitch, roll)
 * - Facial blendshapes (mouth, eyes, brows)
 *
 * Output drives VRM model lookAt + expressions.
 * Camera stream stays local-only (privacy-first).
 */
import { ref, type Ref } from "vue";
import { logger } from "../utils/logger";
import {
  FaceLandmarker,
  FilesetResolver,
  type FaceLandmarkerResult,
} from "@mediapipe/tasks-vision";

// ═══════════════════════════════════════════════════════
//  Types
// ═══════════════════════════════════════════════════════

export interface HeadPose {
  yaw: number;   // Left(-) / Right(+) in degrees
  pitch: number; // Down(-) / Up(+) in degrees
  roll: number;  // Tilt in degrees
}

export interface FaceExpressions {
  happy: number;      // 0-1
  sad: number;        // 0-1
  surprised: number;  // 0-1
  angry: number;      // 0-1
  blink: number;      // 0-1
  blinkLeft: number;  // 0-1
  blinkRight: number; // 0-1
  mouthOpen: number;  // 0-1 (jawOpen)
  browUpLeft: number; // 0-1
  browUpRight: number;// 0-1
}

export interface FaceTrackingData {
  isDetected: boolean;
  head: HeadPose;
  expressions: FaceExpressions;
  confidence: number;
}

export interface UseFaceTrackingReturn {
  faceData: Ref<FaceTrackingData>;
  isTracking: Ref<boolean>;
  isCameraReady: Ref<boolean>;
  startTracking: (videoEl: HTMLVideoElement) => Promise<void>;
  stopTracking: () => void;
  captureFrame: () => string | null;
}

// ═══════════════════════════════════════════════════════
//  Default (no face detected)
// ═══════════════════════════════════════════════════════

function defaultFaceData(): FaceTrackingData {
  return {
    isDetected: false,
    head: { yaw: 0, pitch: 0, roll: 0 },
    expressions: {
      happy: 0, sad: 0, surprised: 0, angry: 0,
      blink: 0, blinkLeft: 0, blinkRight: 0,
      mouthOpen: 0, browUpLeft: 0, browUpRight: 0,
    },
    confidence: 0,
  };
}

// ═══════════════════════════════════════════════════════
//  Head Pose Estimation from Landmarks
// ═══════════════════════════════════════════════════════

/**
 * Estimate head yaw/pitch/roll from key face landmarks.
 * Uses nose tip (1), left eye (33), right eye (263), chin (152), forehead (10).
 *
 * Simple geometric estimation — NOT a full PnP solve but
 * sufficiently accurate for avatar driving at ~30fps.
 */
function estimateHeadPose(landmarks: { x: number; y: number; z: number }[]): HeadPose {
  if (landmarks.length < 468) {
    return { yaw: 0, pitch: 0, roll: 0 };
  }

  const noseTip = landmarks[1];
  const leftEye = landmarks[33];
  const rightEye = landmarks[263];
  const chin = landmarks[152];
  const forehead = landmarks[10];

  // Yaw — horizontal angle from nose offset relative to eye midpoint
  const eyeMidX = (leftEye.x + rightEye.x) / 2;
  const yaw = (noseTip.x - eyeMidX) * -180; // Scale to ~±30°

  // Pitch — vertical angle from nose to eye-chin vertical midpoint
  const verticalMid = (forehead.y + chin.y) / 2;
  const pitch = (noseTip.y - verticalMid) * -180;

  // Roll — tilt angle from eye-to-eye line
  const deltaY = rightEye.y - leftEye.y;
  const deltaX = rightEye.x - leftEye.x;
  const roll = Math.atan2(deltaY, deltaX) * (180 / Math.PI);

  return {
    yaw: clamp(yaw, -45, 45),
    pitch: clamp(pitch, -35, 35),
    roll: clamp(roll, -30, 30),
  };
}

function clamp(val: number, min: number, max: number): number {
  return Math.max(min, Math.min(max, val));
}

// ═══════════════════════════════════════════════════════
//  Blendshape Extraction
// ═══════════════════════════════════════════════════════

/**
 * Map MediaPipe blendshapes → our simplified expression set.
 * MediaPipe outputs ~52 ARKit-compatible blendshapes.
 */
function extractExpressions(blendshapes: { categoryName: string; score: number }[]): FaceExpressions {
  const result: FaceExpressions = {
    happy: 0, sad: 0, surprised: 0, angry: 0,
    blink: 0, blinkLeft: 0, blinkRight: 0,
    mouthOpen: 0, browUpLeft: 0, browUpRight: 0,
  };

  if (!blendshapes || blendshapes.length === 0) return result;

  // Build lookup map for O(1) access
  const map = new Map<string, number>();
  for (const bs of blendshapes) {
    map.set(bs.categoryName, bs.score);
  }

  // Eye blink
  result.blinkLeft = map.get("eyeBlinkLeft") ?? 0;
  result.blinkRight = map.get("eyeBlinkRight") ?? 0;
  result.blink = (result.blinkLeft + result.blinkRight) / 2;

  // Mouth
  result.mouthOpen = map.get("jawOpen") ?? 0;

  // Happy = mouthSmileLeft + mouthSmileRight
  const smileL = map.get("mouthSmileLeft") ?? 0;
  const smileR = map.get("mouthSmileRight") ?? 0;
  result.happy = (smileL + smileR) / 2;

  // Sad = mouthFrownLeft + mouthFrownRight
  const frownL = map.get("mouthFrownLeft") ?? 0;
  const frownR = map.get("mouthFrownRight") ?? 0;
  result.sad = (frownL + frownR) / 2;

  // Surprised = browInnerUp + jawOpen
  const browInnerUp = map.get("browInnerUp") ?? 0;
  result.surprised = Math.min(1, (browInnerUp + result.mouthOpen) / 2);

  // Angry = browDownLeft + browDownRight
  const browDownL = map.get("browDownLeft") ?? 0;
  const browDownR = map.get("browDownRight") ?? 0;
  result.angry = (browDownL + browDownR) / 2;

  // Brow up
  result.browUpLeft = map.get("browOuterUpLeft") ?? 0;
  result.browUpRight = map.get("browOuterUpRight") ?? 0;

  return result;
}

// ═══════════════════════════════════════════════════════
//  Main Composable
// ═══════════════════════════════════════════════════════

export function useFaceTracking(): UseFaceTrackingReturn {
  const faceData = ref<FaceTrackingData>(defaultFaceData());
  const isTracking = ref(false);
  const isCameraReady = ref(false);

  let faceLandmarker: FaceLandmarker | null = null;
  let videoStream: MediaStream | null = null;
  let videoElement: HTMLVideoElement | null = null;
  let animFrameId: number | null = null;
  let lastTimestamp = -1;

  // Hidden canvas for frame capture
  let captureCanvas: HTMLCanvasElement | null = null;
  let captureCtx: CanvasRenderingContext2D | null = null;

  // ─── Init MediaPipe ───
  async function initFaceLandmarker() {
    const vision = await FilesetResolver.forVisionTasks(
      // CDN for WASM binaries (avoids bundling ~5MB into dist)
      "https://cdn.jsdelivr.net/npm/@mediapipe/tasks-vision@latest/wasm"
    );

    faceLandmarker = await FaceLandmarker.createFromOptions(vision, {
      baseOptions: {
        modelAssetPath:
          "https://storage.googleapis.com/mediapipe-models/face_landmarker/face_landmarker/float16/1/face_landmarker.task",
        delegate: "GPU", // WebGL acceleration, falls back to CPU
      },
      runningMode: "VIDEO",
      numFaces: 1,
      outputFaceBlendshapes: true,
      outputFacialTransformationMatrixes: false, // Not needed, save perf
    });
  }

  // ─── Start Tracking ───
  async function startTracking(videoEl: HTMLVideoElement) {
    if (isTracking.value) return;

    try {
      // 1. Init MediaPipe (lazy, only once)
      if (!faceLandmarker) {
        await initFaceLandmarker();
      }

      // 2. Get webcam stream
      videoStream = await navigator.mediaDevices.getUserMedia({
        video: {
          width: { ideal: 640 },
          height: { ideal: 480 },
          frameRate: { ideal: 30 },
          facingMode: "user",
        },
        audio: false,
      });

      videoEl.srcObject = videoStream;
      videoEl.muted = true;
      await videoEl.play();

      videoElement = videoEl;
      isCameraReady.value = true;
      isTracking.value = true;

      // 3. Setup capture canvas
      captureCanvas = document.createElement("canvas");
      captureCanvas.width = 320;  // Low-res for AI frame capture
      captureCanvas.height = 240;
      captureCtx = captureCanvas.getContext("2d");

      // 4. Start detection loop
      lastTimestamp = -1;
      detectLoop();

    } catch (err: unknown) {
      logger.error('[FaceTracking]', 'Camera/MediaPipe init failed:', err instanceof Error ? err.message : String(err));
      isTracking.value = false;
      isCameraReady.value = false;
    }
  }

  // ─── Detection Loop ───
  function detectLoop() {
    if (!isTracking.value || !faceLandmarker || !videoElement) {
      if (animFrameId !== null) {
        cancelAnimationFrame(animFrameId);
        animFrameId = null;
      }
      return;
    }

    animFrameId = requestAnimationFrame(detectLoop);

    // Skip if video not ready
    if (videoElement.readyState < 2) return;

    // Skip duplicate frames (same timestamp)
    const now = performance.now();
    if (now === lastTimestamp) return;
    lastTimestamp = now;

    try {
      const result: FaceLandmarkerResult = faceLandmarker.detectForVideo(
        videoElement,
        now
      );

      if (result.faceLandmarks && result.faceLandmarks.length > 0) {
        const landmarks = result.faceLandmarks[0];

        // Head pose from landmarks
        const head = estimateHeadPose(landmarks);

        // Blendshapes
        const expressions = result.faceBlendshapes && result.faceBlendshapes.length > 0
          ? extractExpressions(result.faceBlendshapes[0].categories)
          : defaultFaceData().expressions;

        faceData.value = {
          isDetected: true,
          head,
          expressions,
          confidence: 0.95,
        };
      } else {
        // No face → smoothly decay to default
        const prev = faceData.value;
        if (prev.isDetected) {
          faceData.value = {
            ...prev,
            isDetected: false,
            confidence: 0,
          };
        }
      }
    } catch {
      // Detection error — skip frame silently
    }
  }

  // ─── Stop Tracking ───
  function stopTracking() {
    isTracking.value = false;
    isCameraReady.value = false;

    if (animFrameId !== null) {
      cancelAnimationFrame(animFrameId);
      animFrameId = null;
    }

    if (videoStream) {
      videoStream.getTracks().forEach(track => track.stop());
      videoStream = null;
    }

    if (videoElement) {
      videoElement.srcObject = null;
      videoElement = null;
    }

    captureCanvas = null;
    captureCtx = null;

    faceData.value = defaultFaceData();
  }

  // ─── Frame Capture for AI Vision ───
  /**
   * Capture current webcam frame as base64 JPEG for AI processing.
   * Low-res (320x240), ~10-20KB per frame.
   * Returns null if camera not ready.
   */
  function captureFrame(): string | null {
    if (!videoElement || !captureCanvas || !captureCtx || !isCameraReady.value) {
      return null;
    }

    captureCtx.drawImage(videoElement, 0, 0, 320, 240);
    // JPEG quality 0.6 — balances size vs. clarity for AI
    return captureCanvas.toDataURL("image/jpeg", 0.6);
  }

  return {
    faceData,
    isTracking,
    isCameraReady,
    startTracking,
    stopTracking,
    captureFrame,
  };
}
