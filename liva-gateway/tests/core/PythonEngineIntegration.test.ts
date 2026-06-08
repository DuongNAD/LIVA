import { describe, it, expect } from "vitest";
import { PreemptiveVramMutex } from "@core/PreemptiveVramMutex";

describe("PreemptiveVramMutex ↔ Python gRPC Integration Mock Test", () => {
    it("should propagate AbortSignal to gRPC client call and release VRAM when preempted", async () => {
        const mutex = new PreemptiveVramMutex(8000); // 8GB
        
        // Task1 represents a running background gRPC call (low priority = 5)
        const handle1 = await mutex.acquire("BackgroundTask", 4000, 5);
        expect(handle1.signal.aborted).toBe(false);

        // Mock gRPC Client Call with AbortSignal listener
        let grpcCallCancelled = false;
        const callPythonEngineMock = async (signal: AbortSignal) => {
            return new Promise<void>((resolve, reject) => {
                const onAbort = () => {
                    grpcCallCancelled = true;
                    reject(new Error("gRPC Call Cancelled"));
                };
                if (signal.aborted) {
                    return onAbort();
                }
                signal.addEventListener("abort", onAbort);
                
                // Simulate slow connection (resolves after 2 seconds if not aborted)
                setTimeout(() => {
                    signal.removeEventListener("abort", onAbort);
                    resolve();
                }, 2000);
            });
        };

        // Trigger task execution asynchronously
        const task1Promise = callPythonEngineMock(handle1.signal)
            .catch(err => {
                // Ensure clean release in finally/catch
                handle1.release();
                return "aborted";
            });

        // Task2 is user interactive (high priority = 12) needing 5000MB
        // This will preempt Task1 since 5000MB is not available
        const handle2 = await mutex.acquire("InteractiveTask", 5000, 12);

        // Check if Task1's gRPC was cancelled and VRAM is updated
        const taskResult = await task1Promise;
        expect(taskResult).toBe("aborted");
        expect(grpcCallCancelled).toBe(true);
        expect(handle1.signal.aborted).toBe(true);

        const status = mutex.getStatus();
        expect(status.allocatedVram).toBe(5000); // Task2 holds 5000MB
        expect(status.availableVram).toBe(3000);

        handle2.release();
        expect(mutex.getStatus().allocatedVram).toBe(0);
    });
});
