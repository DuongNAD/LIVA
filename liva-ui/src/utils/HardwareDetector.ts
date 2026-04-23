/**
 * HardwareDetector — Auto-Profiling GPU/RAM/CPU
 * ==============================================
 * Kiểm tra phần cứng lúc khởi động để quyết định engine 2D (Live2D) hay 3D (VRM).
 * Card onboard (Intel UHD, HD Graphics, Radeon Graphics) → ép 2D
 * Card rời (NVIDIA, AMD discrete, Apple M) → 3D
 */

export type EngineMode = '2D' | '3D';
export type EnginePreference = 'auto' | '2D' | '3D';

export interface HardwareProfile {
  gpu: string;
  ram: number;
  cores: number;
  isWeakGPU: boolean;
  recommendedEngine: EngineMode;
}

/**
 * Detect GPU info from WebGL context
 */
function getGPURenderer(): string {
  try {
    const canvas = document.createElement('canvas');
    const gl = canvas.getContext('webgl') || canvas.getContext('experimental-webgl');
    if (!gl) return 'unknown';

    const debugInfo = (gl as WebGLRenderingContext).getExtension('WEBGL_debug_renderer_info');
    if (!debugInfo) return 'unknown';

    const renderer = (gl as WebGLRenderingContext).getParameter(debugInfo.UNMASKED_RENDERER_WEBGL);
    // Cleanup canvas
    const loseCtx = (gl as WebGLRenderingContext).getExtension('WEBGL_lose_context');
    if (loseCtx) loseCtx.loseContext();

    return renderer || 'unknown';
  } catch {
    return 'unknown';
  }
}

/**
 * Check if GPU is an integrated/weak GPU
 */
function isIntegratedGPU(gpuName: string): boolean {
  const lower = gpuName.toLowerCase();
  const weakPatterns = [
    'intel',
    'uhd',
    'hd graphics',
    'iris',
    'radeon graphics',   // AMD APU integrated
    'radeon vega',       // AMD APU integrated
    'microsoft basic',
    'swiftshader',       // Software renderer
    'llvmpipe',          // Mesa software
    'vmware',
    'virtualbox',
  ];
  return weakPatterns.some(p => lower.includes(p));
}

/**
 * Run full hardware profiling
 */
export function profileHardware(): HardwareProfile {
  const gpu = getGPURenderer();
  const ram = (navigator as any).deviceMemory || 4; // GB, default 4 if API unavailable
  const cores = navigator.hardwareConcurrency || 4;
  const isWeakGPU = isIntegratedGPU(gpu);

  let recommendedEngine: EngineMode = '3D';
  if (ram < 8 || cores < 6 || isWeakGPU) {
    recommendedEngine = '2D';
  }

  return { gpu, ram, cores, isWeakGPU, recommendedEngine };
}

/**
 * Determine which engine to use based on preference + hardware
 */
export function detectOptimalEngine(preference: EnginePreference = 'auto'): EngineMode {
  if (preference === '2D') return '2D';
  if (preference === '3D') return '3D';

  // Auto-detect
  const profile = profileHardware();
  return profile.recommendedEngine;
}
