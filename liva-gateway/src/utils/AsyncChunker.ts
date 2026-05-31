import { logger } from "./logger";

/**
 * AsyncChunker
 * ============
 * Processes large arrays in smaller batches (chunks) asynchronously, yielding 
 * control back to Node's Event Loop between chunks using `setImmediate`.
 * This protects the main thread from CPU-heavy operations blocking incoming 
 * I/O (Webhooks, Telegram, Zalo polling).
 *
 * It automatically detects if it is running inside a test environment 
 * (`process.env.NODE_ENV === "test"`) and bypasses `setImmediate` using 
 * `Promise.resolve()` to avoid fake-timer deadlocks/hangs under Vitest.
 */
export class AsyncChunker {
    /**
     * Runs a large array without blocking the Event Loop, yielding CPU after every `chunkSize` items.
     *
     * @param items Array of items to process
     * @param processor Async or sync function executing logic on each item
     * @param chunkSize Number of items processed before yielding (default: 50)
     */
    public static async processNonBlocking<T, R>(
        items: T[],
        processor: (item: T, index: number) => Promise<R> | R,
        chunkSize: number = 50
    ): Promise<R[]> {
        const results: R[] = [];
        for (let i = 0; i < items.length; i += chunkSize) {
            const chunk = items.slice(i, i + chunkSize);
            
            // Parallel execution within the chunk
            const chunkResults = await Promise.all(
                chunk.map((item, idx) => processor(item, i + idx))
            );
            results.push(...chunkResults);

            // Yield control back to Node's Event Loop
            if (i + chunkSize < items.length) {
                if (typeof process !== "undefined" && process.env.NODE_ENV === "test") {
                    await Promise.resolve();
                } else {
                    await new Promise<void>((resolve) => setImmediate(resolve));
                }
            }
        }
        return results;
    }
}
