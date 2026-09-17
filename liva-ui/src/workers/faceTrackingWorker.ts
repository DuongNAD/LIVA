/**
 * faceTrackingWorker.ts — Dedicated Web Worker for MediaPipe FaceLandmarker
 * =========================================================================
 * Runs MediaPipe face landmark detection off the main UI thread to protect 60+ FPS
 * Three.js rendering and eliminate UI jank (RSK-04).
 */
import {
  FaceLandmarker,
  FilesetResolver,
  type FaceLandmarkerResult,
} from "@mediapipe/tasks-vision";
import {
  estimateHeadPose,
  extractExpressions,
  defaultFaceData,
  type FaceTrackingData,
} from "../composables/useFaceTracking";

export type WorkerInMessage =
  | { type: "init"; wasmPath?: string; modelAssetPath?: string }
  | { type: "detect"; bitmap: ImageBitmap; timestamp: number }
  | { type: "dispose" };

export type WorkerOutMessage =
  | { type: "ready" }
  | { type: "result"; data: FaceTrackingData }
  | { type: "error"; error: string };

const workerScope = self as unknown as {
  postMessage(message: WorkerOutMessage): void;
  onmessage: ((e: MessageEvent<WorkerInMessage>) => void) | null;
  close(): void;
};

let landmarker: FaceLandmarker | null = null;
let isInitializing = false;

async function initLandmarker(
  wasmPath = "/assets/wasm",
  modelAssetPath = "/assets/models/face_landmarker.task"
) {
  if (landmarker || isInitializing) return;
  isInitializing = true;
  try {
    const vision = await FilesetResolver.forVisionTasks(wasmPath);
    landmarker = await FaceLandmarker.createFromOptions(vision, {
      baseOptions: {
        modelAssetPath,
        delegate: "GPU",
      },
      runningMode: "IMAGE",
      numFaces: 1,
      outputFaceBlendshapes: true,
      outputFacialTransformationMatrixes: false,
    });
    workerScope.postMessage({ type: "ready" });
  } catch (err) {
    workerScope.postMessage({
      type: "error",
      error: err instanceof Error ? err.message : String(err),
    });
  } finally {
    isInitializing = false;
  }
}

workerScope.onmessage = async (e: MessageEvent<WorkerInMessage>) => {
  const msg = e.data;
  if (!msg) return;

  if (msg.type === "init") {
    await initLandmarker(msg.wasmPath, msg.modelAssetPath);
  } else if (msg.type === "detect") {
    const { bitmap } = msg;
    if (!landmarker) {
      if (typeof bitmap.close === "function") bitmap.close();
      return;
    }

    try {
      const result: FaceLandmarkerResult = landmarker.detect(bitmap);
      if (typeof bitmap.close === "function") bitmap.close();

      if (result.faceLandmarks && result.faceLandmarks.length > 0) {
        const landmarks = result.faceLandmarks[0];
        const head = estimateHeadPose(landmarks);
        const expressions =
          result.faceBlendshapes && result.faceBlendshapes.length > 0
            ? extractExpressions(result.faceBlendshapes[0].categories)
            : defaultFaceData().expressions;

        workerScope.postMessage({
          type: "result",
          data: {
            isDetected: true,
            head,
            expressions,
            confidence: 0.95,
          },
        });
      } else {
        workerScope.postMessage({
          type: "result",
          data: {
            ...defaultFaceData(),
            isDetected: false,
            confidence: 0,
          },
        });
      }
    } catch (err) {
      if (typeof bitmap.close === "function") bitmap.close();
      workerScope.postMessage({
        type: "error",
        error: err instanceof Error ? err.message : String(err),
      });
    }
  } else if (msg.type === "dispose") {
    if (landmarker) {
      landmarker.close();
      landmarker = null;
    }
    workerScope.close();
  }
};
