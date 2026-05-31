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
  os: string;
  browser: string;
  resolution: string;
  webglVersion: string;
  maxTextureSize: number;
}

function detectOS() {
  const ua = navigator.userAgent;
  if (ua.indexOf("Win") !== -1) return "Windows";
  if (ua.indexOf("Mac") !== -1) return "macOS";
  if (ua.indexOf("Linux") !== -1) return "Linux";
  if (ua.indexOf("Android") !== -1) return "Android";
  if (ua.indexOf("like Mac") !== -1) return "iOS";
  return "Unknown";
}

function detectBrowser() {
  const ua = navigator.userAgent;
  if (ua.includes("Chrome") && !ua.includes("Edg") && !ua.includes("OPR")) return "Chrome";
  if (ua.includes("Edg")) return "Edge";
  if (ua.includes("Firefox")) return "Firefox";
  if (ua.includes("Safari") && !ua.includes("Chrome")) return "Safari";
  if (ua.includes("OPR")) return "Opera";
  return "Unknown";
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

function cleanGPUName(rawName: string): string {
  let name = rawName;
  // Handle ANGLE wrapper: "ANGLE (NVIDIA, NVIDIA GeForce RTX 3060 Direct3D11 vs_5_0 ps_5_0, D3D11)"
  const angleMatch = name.match(/ANGLE \((.*)\)/);
  if (angleMatch && angleMatch[1]) {
    const parts = angleMatch[1].split(',');
    if (parts.length >= 2) {
      name = parts[1].trim(); 
    } else {
      name = parts[0].trim();
    }
  }

  // Strip APIs and shader versions
  name = name.replace(/ Direct3D.*$/i, '');
  name = name.replace(/ OpenGL.*$/i, '');
  name = name.replace(/ Vulkan.*$/i, '');
  name = name.replace(/ Metal.*$/i, '');
  name = name.replace(/ vs_.*$/i, '');
  name = name.replace(/ ps_.*$/i, '');
  
  return name.trim() || rawName;
}

export function profileHardware(): HardwareProfile {
  let gpu = 'unknown';
  let webglVersion = 'N/A';
  let maxTextureSize = 0;

  try {
    const canvas = document.createElement('canvas');
    let gl: WebGLRenderingContext | WebGL2RenderingContext | null = canvas.getContext('webgl2');
    if (gl) {
      webglVersion = 'WebGL 2.0';
    } else {
      gl = (canvas.getContext('webgl') || canvas.getContext('experimental-webgl')) as WebGLRenderingContext;
      if (gl) webglVersion = 'WebGL 1.0';
    }

    if (gl) {
      maxTextureSize = gl.getParameter(gl.MAX_TEXTURE_SIZE);
      const debugInfo = gl.getExtension('WEBGL_debug_renderer_info');
      if (debugInfo) {
        gpu = cleanGPUName(gl.getParameter(debugInfo.UNMASKED_RENDERER_WEBGL) || 'unknown');
      }
      const loseCtx = gl.getExtension('WEBGL_lose_context');
      if (loseCtx) loseCtx.loseContext();
    }
  } catch {
    // Ignore context errors
  }

  const ram = (navigator as any).deviceMemory || 4; // GB, default 4 if API unavailable
  const cores = navigator.hardwareConcurrency || 4;
  const isWeakGPU = isIntegratedGPU(gpu);
  
  const os = detectOS();
  const browser = detectBrowser();
  const resolution = `${window.screen.width}x${window.screen.height}`;

  let recommendedEngine: EngineMode = '3D';
  if (ram < 8 || cores < 6 || isWeakGPU) {
    recommendedEngine = '2D';
  }

  return { gpu, ram, cores, isWeakGPU, recommendedEngine, os, browser, resolution, webglVersion, maxTextureSize };
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
