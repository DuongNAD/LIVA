/**
 * fetch.ts — Browser-safe fetch wrapper for LIVA UI
 * ==================================================
 * Simple wrapper around native fetch with timeout support.
 * Does NOT throw on HTTP 4xx/5xx — caller must check response.ok.
 * For internal localhost endpoints only (sensory capture, etc.).
 */
export async function safeFetch(
  input: RequestInfo | URL,
  init?: RequestInit,
  timeoutMs = 5000
): Promise<Response> {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), timeoutMs);
  try {
    const res = await fetch(input, { ...init, signal: controller.signal });
    return res;
  } finally {
    clearTimeout(timer);
  }
}
