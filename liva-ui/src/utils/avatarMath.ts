/**
 * avatarMath.ts — hàm thuần dùng chung cho hoạt ảnh avatar
 * =========================================================
 * Nội suy, đường cong ease, nhịp chớp mắt và bốc thăm có trọng số.
 *
 * Tách ra từ `use3DModel.ts` ngày 06/08/2026 (mục U25 trong
 * docs/03-danh-gia/05-nang-cap-toan-dien.md). Trước đó năm hàm này tồn tại
 * **hai bản giống nhau từng byte**: một bản riêng trong `use3DModel.ts` và một
 * bản `export` trong một composable mồ côi đã bị xoá. Bộ test lại nhập từ bản
 * mồ côi, nên thứ được kiểm và thứ được chạy là hai bản khác nhau — chúng khớp
 * nhau thuần tuý do may.
 *
 * Giữ ở đây, một bản duy nhất, để tình trạng đó không lặp lại.
 */

/** Linear interpolation for smooth transitions */
export function lerp(a: number, b: number, t: number): number {
  return a + (b - a) * t;
}

/** Ease-out quadratic — fast start, slow end (natural eyelid close) */
export function easeOutQuad(t: number): number {
  return t * (2 - t);
}

/** Ease-in quadratic — slow start, fast end (natural expression fade) */
export function easeInQuad(t: number): number {
  return t * t;
}

/** Random blink interval using Poisson-like distribution (2-6s base + jitter) */
export function randomBlinkInterval(): number {
  // Average human blink rate: 15-20 blinks/min = every 3-4s
  // Add random jitter for natural variation
  return 2 + Math.random() * 4 + Math.random() * Math.random() * 3;
 // NOSONAR
}

/** Weighted random selection */
export function weightedRandom<T>(options: T[], weights: number[]): T {
  const total = weights.reduce((s, w) => s + w, 0);
  let r = Math.random() * total;
 // NOSONAR
  for (let i = 0; i < options.length; i++) {
    r -= weights[i];
    if (r <= 0) return options[i];
  }
  return options[options.length - 1];
}

// ═══════════════════════════════════════════════════════
//  Fixational Eye Micro-Saccade Math (Feature 10)
//  2-4 Hz Poisson/interval timed, amplitude ±0.85° (±0.0148 rad)
// ═══════════════════════════════════════════════════════

export const MAX_SACCADE_AMPLITUDE_DEG = 0.85; // ±0.85° ≈ ±0.0148 rad
export const SACCADE_JUMP_DURATION_S = 0.025; // 25ms (20-30ms ballistic jump)
export const SACCADE_DRIFT_HALF_LIFE_S = 0.20; // 200ms half-life fixational drift

export interface SaccadeDisplacement {
  yaw: number;   // horizontal angle in degrees (±0.85° max)
  pitch: number; // vertical angle in degrees (±0.60° max, elliptical aspect ratio)
}

/**
 * Random interval between fixational micro-saccades (2-4 Hz Poisson/interval timed).
 * Inter-saccadic interval: 250ms - 500ms, with biological refractory floor and mean ~333ms.
 */
export function randomSaccadeInterval(): number {
  // Refractory floor 0.22s + Poisson exponential tail (lambda = 8.0)
  const u = Math.random();
  const exponentialTail = -Math.log(1 - u * 0.90) / 8.0;
  const interval = 0.22 + exponentialTail;
  // Strictly clamp within 2-4 Hz band: [0.25s, 0.50s]
  return Math.min(Math.max(interval, 0.25), 0.50);
}

/**
 * Generate random 2D fixational micro-saccade displacement within ±0.85° (±0.0148 rad).
 * Employs elliptical polar distribution (horizontal dominant, 0.70 vertical scale).
 */
export function randomSaccadeDisplacement(
  maxAmplitudeDeg: number = MAX_SACCADE_AMPLITUDE_DEG
): SaccadeDisplacement {
  const angle = Math.random() * Math.PI * 2;
  const minAmp = 0.15;
  const r = minAmp + Math.random() * (maxAmplitudeDeg - minAmp);

  const yaw = r * Math.cos(angle);
  const pitch = r * Math.sin(angle) * 0.70;

  const clampedYaw = Math.max(-maxAmplitudeDeg, Math.min(maxAmplitudeDeg, yaw));
  const clampedPitch = Math.max(-maxAmplitudeDeg, Math.min(maxAmplitudeDeg, pitch));

  return {
    yaw: Math.round(clampedYaw * 10000) / 10000,
    pitch: Math.round(clampedPitch * 10000) / 10000,
  };
}

/** Smoothstep interpolation for ballistic saccade jump (zero velocity at endpoints) */
export function smoothstep(edge0: number, edge1: number, x: number): number {
  const t = Math.min(Math.max((x - edge0) / (edge1 - edge0), 0), 1);
  return t * t * (3 - 2 * t);
}
