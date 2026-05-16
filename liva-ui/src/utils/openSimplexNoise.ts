/**
 * OpenSimplex 2D Noise — Shared Module
 * =======================================
 * Inline, zero-dependency implementation of 2D OpenSimplex noise.
 * Value noise with smooth gradients — never repeats.
 * Used by: use3DModel.ts, useVRM.ts (idle micro-sway + breathing animations)
 */
export const STRETCH_2D = (Math.sqrt(3) - 1) / 2;
export const SQUISH_2D = (1 / Math.sqrt(3) - 1) / 2;

export const GRADIENTS_2D = [
  5, 2, 2, 5, -5, 2, -2, 5,
  5, -2, 2, -5, -5, -2, -2, -5,
];

export function buildPerm(seed: number): Int16Array {
  const perm = new Int16Array(256);
  const source = new Int16Array(256);
  for (let i = 0; i < 256; i++) source[i] = i;
  seed = Math.trunc(seed * 6364136223 + 1442695040);
  for (let i = 255; i >= 0; i--) {
    seed = (seed * 25214903917 + 11) & 0xffffffffffff;
    let r = ((seed + 31) % (i + 1));
    if (r < 0) r += i + 1;
    perm[i] = source[r];
    source[r] = source[i];
  }
  return perm;
}

export const PERM = buildPerm(42);

export function extrapolate(xsb: number, ysb: number, dx: number, dy: number): number {
  const index = (PERM[(PERM[xsb & 0xff] + ysb) & 0xff] % 8) * 2;
  return GRADIENTS_2D[index] * dx + GRADIENTS_2D[index + 1] * dy;
}

export function simplex2D(x: number, y: number): number {
  const stretchOffset = (x + y) * STRETCH_2D;
  const xs = x + stretchOffset;
  const ys = y + stretchOffset;
  const xsb = Math.floor(xs);
  const ysb = Math.floor(ys);
  const squishOffset = (xsb + ysb) * SQUISH_2D;
  const dx0 = x - (xsb + squishOffset);
  const dy0 = y - (ysb + squishOffset);
  const xins = xs - xsb;
  const yins = ys - ysb;

  let value = 0;

  // Contribution (0,0)
  const attn0 = 2 - dx0 * dx0 - dy0 * dy0;
  if (attn0 > 0) {
    const attn0sq = attn0 * attn0;
    value += attn0sq * attn0sq * extrapolate(xsb, ysb, dx0, dy0);
  }

  // Contribution (1,0) or (0,1)
  if (xins + yins <= 1) {
    const dx1 = dx0 - 1 - SQUISH_2D;
    const dy1 = dy0 - SQUISH_2D;
    const attn1 = 2 - dx1 * dx1 - dy1 * dy1;
    if (attn1 > 0) {
      const attn1sq = attn1 * attn1;
      value += attn1sq * attn1sq * extrapolate(xsb + 1, ysb, dx1, dy1);
    }
    const dx2 = dx0 - SQUISH_2D;
    const dy2 = dy0 - 1 - SQUISH_2D;
    const attn2 = 2 - dx2 * dx2 - dy2 * dy2;
    if (attn2 > 0) {
      const attn2sq = attn2 * attn2;
      value += attn2sq * attn2sq * extrapolate(xsb, ysb + 1, dx2, dy2);
    }
  } else {
    const dx1 = dx0 - 1 - 2 * SQUISH_2D;
    const dy1 = dy0 - 1 - 2 * SQUISH_2D;
    const attn1 = 2 - dx1 * dx1 - dy1 * dy1;
    if (attn1 > 0) {
      const attn1sq = attn1 * attn1;
      value += attn1sq * attn1sq * extrapolate(xsb + 1, ysb + 1, dx1, dy1);
    }
    const dx2 = dx0 - SQUISH_2D;
    const dy2 = dy0 - 1 - SQUISH_2D;
    const attn2 = 2 - dx2 * dx2 - dy2 * dy2;
    if (attn2 > 0) {
      const attn2sq = attn2 * attn2;
      value += attn2sq * attn2sq * extrapolate(xsb, ysb + 1, dx2, dy2);
    }
    const dx3 = dx0 - 1 - SQUISH_2D;
    const dy3 = dy0 - SQUISH_2D;
    const attn3 = 2 - dx3 * dx3 - dy3 * dy3;
    if (attn3 > 0) {
      const attn3sq = attn3 * attn3;
      value += attn3sq * attn3sq * extrapolate(xsb + 1, ysb, dx3, dy3);
    }
  }

  return value / 6;
}
