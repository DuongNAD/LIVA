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
