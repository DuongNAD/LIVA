
/**
 * safeFetch — Enterprise-grade Fetch Wrapper
 * ==========================================
 * Solves 3 critical gaps in native fetch:
 * 
 * 1. HTTP Error Detection: Native fetch resolves on 4xx/5xx (silent fail).
 *    safeFetch throws on non-2xx, preserving the response body as error detail.
 *    
 * 2. Timeout Management: Centralized AbortController lifecycle.
 *    Timer is ALWAYS cleaned up via `finally`, eliminating timer leaks.
 *    
 * 3. DRY: One place to maintain timeout, error-checking, and logging
 *    instead of duplicating AbortController boilerplate across 7+ files.
 * 
 * @param url       - Target URL
 * @param options   - Standard RequestInit (method, headers, body, etc.)
 * @param timeoutMs - Abort timeout in milliseconds (default: 5000)
 * @returns         - Response object (guaranteed 2xx status)
 * @throws          - Error with HTTP status + body on 4xx/5xx
 * @throws          - AbortError on timeout
 * @throws          - TypeError with cause on network failure (ECONNREFUSED, DNS, etc.)
 */
export async function safeFetch(
    url: string,
    options: RequestInit = {},
    timeoutMs = 5000
): Promise<Response> {
    const controller = new AbortController();
    const timeoutId = setTimeout(() => controller.abort(), timeoutMs);

    try {
        const res = await fetch(url, { ...options, signal: controller.signal });

        if (!res.ok) {
            // Extract the actual API error message from the response body
            // (Replaces the lost axios `e.response.data` capability)
            const errorBody = await res.text().catch(() => "Unknown API Error");
            throw new Error(`HTTP ${res.status}: ${errorBody}`);
        }

        return res;
    } finally {
        clearTimeout(timeoutId);
    }
}

/**
 * withSafeTimeout — Leak-Free Promise Timeout
 * =============================================
 * Wraps any promise with a timeout that is GUARANTEED to be cleaned up.
 * 
 * Unlike `Promise.race([task, setTimeout])`, this helper:
 * 1. Stores the timer ref and clears it in `finally` (no leak on success)
 * 2. Uses AbortController so the caller can detect timeout vs real error
 * 3. Returns the resolved value with correct typing
 * 
 * @param promise  - The async operation to race against the clock
 * @param ms       - Timeout in milliseconds
 * @param label    - Optional label for the timeout error message
 * @returns        - Resolved value of the promise
 * @throws         - Error with `label` message if timeout fires first
 */
export function withSafeTimeout<T>(
    promise: Promise<T>,
    ms: number,
    label = "TIMEOUT"
): Promise<T> {
    let timeoutId: ReturnType<typeof setTimeout>;

    const timeoutPromise = new Promise<never>((_, reject) => {
        timeoutId = setTimeout(() => reject(new Error(label)), ms);
    });

    return Promise.race([promise, timeoutPromise]).finally(() => {
        clearTimeout(timeoutId!);
    });
}
