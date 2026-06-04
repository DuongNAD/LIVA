import { describe, it, expect } from "vitest";
import { PreemptiveVramMutex } from "@core/PreemptiveVramMutex";

describe("PreemptiveVramMutex — Concurrency Stress Test (100 Requests)", () => {
    it("should process 100 concurrent requests without deadlock or corrupt VRAM count", async () => {
        const mutex = new PreemptiveVramMutex(16000); // 16GB total VRAM
        const requestsCount = 100;
        const promises: Promise<void>[] = [];
        
        let completedCount = 0;
        let negativeVramDetected = false;

        for (let i = 0; i < requestsCount; i++) {
            const id = `Task_${i}`;
            const memory = Math.floor(Math.random() * 500) + 100; // 100MB to 600MB
            const priority = Math.floor(Math.random() * 15); // Priority 0 to 14

            const p = mutex.acquire(id, memory, priority, 5000)
                .then(async (handle) => {
                    const status = mutex.getStatus();
                    if (status.availableVram < 0 || status.allocatedVram < 0) {
                        negativeVramDetected = true;
                    }
                    
                    // Simulate random task duration
                    await new Promise(resolve => setTimeout(resolve, Math.floor(Math.random() * 10) + 1));
                    
                    handle.release();
                    completedCount++;
                })
                .catch((err) => {
                    // It is acceptable if some tasks timeout under high load
                    if (!err.message.includes("VRAM Acquisition Timeout")) {
                        throw err;
                    }
                });

            promises.push(p);
        }

        await Promise.all(promises);

        const finalStatus = mutex.getStatus();
        expect(finalStatus.allocatedVram).toBe(0);
        expect(finalStatus.availableVram).toBe(16000);
        expect(finalStatus.pendingRequests).toBe(0);
        expect(negativeVramDetected).toBe(false);
        expect(completedCount).toBeGreaterThan(80); 
    });
});
