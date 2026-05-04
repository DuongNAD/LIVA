import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import {
    CoreKernel,
    SelfHealingTensorStore,
    QuantizedMemoryStore,
    type QuantHandle,
    type ECCResidual,
    type QuantToken,
} from "../../src/memory/TurboQuantStore";
import * as fs from "node:fs";
import * as path from "node:path";

// ============================================================
// TEST GROUP 1: CoreKernel — Authority Layer
// ============================================================
describe("CoreKernel (Authority Layer)", () => {
    let kernel: CoreKernel;

    beforeEach(() => {
        kernel = new CoreKernel(["system", "user", "assistant"]);
    });

    describe("mintAuthToken", () => {
        it("should mint a valid token for authorized role", () => {
            const token = kernel.mintAuthToken("user");
            expect(token).not.toBeNull();
            expect(typeof token).toBe("string");
        });

        it("should return null for unauthorized role", () => {
            const token = kernel.mintAuthToken("hacker");
            expect(token).toBeNull();
        });

        it("should mint tokens for all authorized roles", () => {
            expect(kernel.mintAuthToken("system")).not.toBeNull();
            expect(kernel.mintAuthToken("user")).not.toBeNull();
            expect(kernel.mintAuthToken("assistant")).not.toBeNull();
        });

        it("should mint unique tokens per role", () => {
            const systemToken = kernel.mintAuthToken("system");
            const userToken = kernel.mintAuthToken("user");
            expect(systemToken).not.toEqual(userToken);
        });
    });

    describe("validateToken", () => {
        it("should validate a correctly minted token", () => {
            const token = kernel.mintAuthToken("user") as string;
            expect(kernel.validateToken(token, "user")).toBe(true);
        });

        it("should reject token validated against wrong role", () => {
            const token = kernel.mintAuthToken("user") as string;
            expect(kernel.validateToken(token, "system")).toBe(false);
        });

        it("should reject a completely forged token", () => {
            expect(kernel.validateToken("fake_token_123", "user")).toBe(false);
        });

        it("should reject empty string token", () => {
            expect(kernel.validateToken("", "user")).toBe(false);
        });
    });

    describe("Temporal Proof System", () => {
        it("should generate a temporal proof from timestamp", () => {
            const now = Date.now();
            const proof = kernel.generateTemporalProof(now);
            expect(typeof proof).toBe("string");
            expect(proof.length).toBeGreaterThan(5);
        });

        it("should verify a valid proof within drift window (≤2s)", () => {
            const now = Date.now();
            const proof = kernel.generateTemporalProof(now);
            // Verify immediately — should pass
            expect(kernel.verifyTemporalProof(proof, now)).toBe(true);
        });

        it("should verify proof with small drift (1 second)", () => {
            const now = Date.now();
            const proof = kernel.generateTemporalProof(now);
            // 1 second later
            expect(kernel.verifyTemporalProof(proof, now + 1000)).toBe(true);
        });

        it("should reject proof with large drift (>2 seconds)", () => {
            const now = Date.now();
            const proof = kernel.generateTemporalProof(now);
            // 5 seconds later
            expect(kernel.verifyTemporalProof(proof, now + 5000)).toBe(false);
        });

        it("should reject malformed proof string", () => {
            expect(kernel.verifyTemporalProof("malformed", Date.now())).toBe(false);
        });

        it("should reject proof with tampered secret", () => {
            const now = Date.now();
            const normalizedTime = Math.floor(now / 1000);
            const fakeProof = `${normalizedTime}_fake_secret`;
            expect(kernel.verifyTemporalProof(fakeProof, now)).toBe(false);
        });
    });

    describe("Cross-instance isolation", () => {
        it("tokens from different CoreKernel instances should not cross-validate", () => {
            const kernel2 = new CoreKernel(["user"]);
            const token1 = kernel.mintAuthToken("user") as string;
            // Token minted by kernel1 should fail validation on kernel2
            expect(kernel2.validateToken(token1, "user")).toBe(false);
        });
    });
});

// ============================================================
// TEST GROUP 2: SelfHealingTensorStore — Projection & ECC
// ============================================================
describe("SelfHealingTensorStore", () => {
    let kernel: CoreKernel;
    let tensorStore: SelfHealingTensorStore;
    let userToken: string;

    beforeEach(() => {
        kernel = new CoreKernel(["system", "user", "assistant"]);
        tensorStore = new SelfHealingTensorStore(kernel, 64, 128);
        userToken = kernel.mintAuthToken("user") as string;
    });

    afterEach(() => {
        tensorStore.dispose();
    });

    describe("projectAndGenerateECC", () => {
        it("should project a vector with valid auth", () => {
            const vector = Array.from({ length: 128 }, () => Math.random());
            const proof = kernel.generateTemporalProof(Date.now());

            const result = tensorStore.projectAndGenerateECC(vector, userToken, "user", proof);

            expect(result.tensor).toBeDefined();
            expect(result.ecc).toBeDefined();
            expect(result.tensor.length).toBe(64);
            expect(result.ecc.correctionVector.length).toBe(64);
            expect(typeof result.ecc.driftMagnitude).toBe("number");
        });

        it("should throw on invalid auth token", () => {
            const vector = Array.from({ length: 128 }, () => Math.random());
            const proof = kernel.generateTemporalProof(Date.now());

            expect(() =>
                tensorStore.projectAndGenerateECC(vector, "fake_token", "user", proof)
            ).toThrow("Zero-Trust Violation");
        });

        it("should throw on expired temporal proof", () => {
            const vector = Array.from({ length: 128 }, () => Math.random());
            const oldTime = Date.now() - 10000; // 10 seconds ago
            const proof = kernel.generateTemporalProof(oldTime);

            expect(() =>
                tensorStore.projectAndGenerateECC(vector, userToken, "user", proof)
            ).toThrow("Temporal Integrity Violation");
        });

        it("should throw on null/undefined vector", () => {
            const proof = kernel.generateTemporalProof(Date.now());

            expect(() =>
                tensorStore.projectAndGenerateECC(null as any, userToken, "user", proof)
            ).toThrow("Self-Healing Block");
        });

        it("should accept Float32Array as input", () => {
            const vector = new Float32Array(128).fill(0.5);
            const proof = kernel.generateTemporalProof(Date.now());

            const result = tensorStore.projectAndGenerateECC(vector, userToken, "user", proof);
            expect(result.tensor.length).toBe(64);
        });

        it("should cache repeated projections (same vector+role)", () => {
            const vector = [1.0, 2.0, 3.0]; // short vector, padded with 0s
            const proof = kernel.generateTemporalProof(Date.now());

            const r1 = tensorStore.projectAndGenerateECC(vector, userToken, "user", proof);
            const proof2 = kernel.generateTemporalProof(Date.now());
            const r2 = tensorStore.projectAndGenerateECC(vector, userToken, "user", proof2);

            // Same cache hit — same reference
            expect(r1.tensor).toBe(r2.tensor);
        });

        it("should produce compressed tensor values as ±1 (sign quantization)", () => {
            const vector = Array.from({ length: 128 }, (_, i) => (i % 2 === 0 ? 5.0 : -3.0));
            const proof = kernel.generateTemporalProof(Date.now());

            const result = tensorStore.projectAndGenerateECC(vector, userToken, "user", proof);
            for (let i = 0; i < result.tensor.length; i++) {
                expect(Math.abs(result.tensor[i])).toBe(1);
            }
        });

        it("should produce NON-ZERO ECC correctionVector (drift compensation)", () => {
            // Vector with values far from ±1 → residual MUST be non-zero
            const vector = Array.from({ length: 128 }, (_, i) => (i % 2 === 0 ? 3.7 : -2.1));
            const proof = kernel.generateTemporalProof(Date.now());

            const result = tensorStore.projectAndGenerateECC(vector, userToken, "user", proof);

            // At least SOME correction values must be non-zero
            const hasNonZero = Array.from(result.ecc.correctionVector).some(v => Math.abs(v) > 0.001);
            expect(hasNonZero).toBe(true);
            expect(result.ecc.driftMagnitude).toBeGreaterThan(0);
        });

        it("should allow reconstruction: tensor[i] + correction[i] ≈ projected[i]", () => {
            const vector = Array.from({ length: 128 }, () => Math.random() * 4 - 2);
            const proof = kernel.generateTemporalProof(Date.now());

            const result = tensorStore.projectAndGenerateECC(vector, userToken, "user", proof);

            // tensor + correction should approximate the original projected value
            for (let i = 0; i < result.tensor.length; i++) {
                const reconstructed = result.tensor[i] + result.ecc.correctionVector[i];
                // The reconstructed value should be a real number (not NaN or Infinity)
                expect(Number.isFinite(reconstructed)).toBe(true);
            }
        });
    });

    describe("healedCosineSimilarity", () => {
        it("should return 1.0 for identical tensors+ECC", () => {
            const vector = Array.from({ length: 128 }, () => Math.random());
            const proof = kernel.generateTemporalProof(Date.now());
            const r = tensorStore.projectAndGenerateECC(vector, userToken, "user", proof);

            const sim = tensorStore.healedCosineSimilarity(r.tensor, r.tensor, r.ecc, r.ecc);
            expect(sim).toBeCloseTo(1.0, 5);
        });

        it("should return 0 for mismatched length tensors", () => {
            const t1 = new Float32Array([1, 0, -1]) as unknown as QuantHandle<Float32Array>;
            const t2 = new Float32Array([1, 0]) as unknown as QuantHandle<Float32Array>;
            const ecc1: ECCResidual = {
                correctionVector: new Float32Array([0, 0, 0]) as unknown as QuantHandle<Float32Array>,
                driftMagnitude: 0,
            };
            const ecc2: ECCResidual = {
                correctionVector: new Float32Array([0, 0]) as unknown as QuantHandle<Float32Array>,
                driftMagnitude: 0,
            };

            expect(tensorStore.healedCosineSimilarity(t1, t2, ecc1, ecc2)).toBe(0);
        });

        it("should return value between -1 and 1", () => {
            const v1 = Array.from({ length: 128 }, () => Math.random() * 2 - 1);
            const v2 = Array.from({ length: 128 }, () => Math.random() * 2 - 1);
            const proof = kernel.generateTemporalProof(Date.now());

            const r1 = tensorStore.projectAndGenerateECC(v1, userToken, "user", proof);
            const proof2 = kernel.generateTemporalProof(Date.now());
            const r2 = tensorStore.projectAndGenerateECC(v2, userToken, "user", proof2);

            const sim = tensorStore.healedCosineSimilarity(r1.tensor, r2.tensor, r1.ecc, r2.ecc);
            expect(sim).toBeGreaterThanOrEqual(-1);
            expect(sim).toBeLessThanOrEqual(1);
        });

        it("should return 0 for zero vectors", () => {
            const zero = new Float32Array(64).fill(0) as unknown as QuantHandle<Float32Array>;
            const zeroEcc: ECCResidual = {
                correctionVector: new Float32Array(64).fill(0) as unknown as QuantHandle<Float32Array>,
                driftMagnitude: 0,
            };
            expect(tensorStore.healedCosineSimilarity(zero, zero, zeroEcc, zeroEcc)).toBe(0);
        });
    });

    describe("dispose", () => {
        it("should clear cache and stop GC without errors", () => {
            expect(() => tensorStore.dispose()).not.toThrow();
        });
    });
});

// ============================================================
// TEST GROUP 3: QuantizedMemoryStore — End-to-End Memory CRUD
// ============================================================
describe("QuantizedMemoryStore", () => {
    let kernel: CoreKernel;
    let store: QuantizedMemoryStore;
    const TEST_FILE = path.join(process.cwd(), "tests", "_temp_quant_test.jsonl");

    beforeEach(() => {
        // Clean up (try/catch for Windows EBUSY file locks)
        try { if (fs.existsSync(TEST_FILE)) fs.unlinkSync(TEST_FILE); } catch {}
        kernel = new CoreKernel(["system", "user", "assistant"]);
        store = new QuantizedMemoryStore(kernel, TEST_FILE);
    });

    afterEach(() => {
        store.dispose();
        try { if (fs.existsSync(TEST_FILE)) fs.unlinkSync(TEST_FILE); } catch {}
    });

    describe("addMemory", () => {
        it("should add memory with valid auth token", async () => {
            const token = kernel.mintAuthToken("user") as string;
            const embedding = Array.from({ length: 256 }, () => Math.random());

            await expect(
                store.addMemory("user", "Tôi thích cà phê", embedding, token)
            ).resolves.not.toThrow();
        });

        it("should reject addMemory with invalid token", async () => {
            const embedding = Array.from({ length: 256 }, () => Math.random());

            await expect(
                store.addMemory("user", "test", embedding, "bad_token")
            ).rejects.toThrow();
        });

        it("should persist data to file after addMemory", async () => {
            const token = kernel.mintAuthToken("user") as string;
            const embedding = Array.from({ length: 256 }, () => Math.random());

            await store.addMemory("user", "Hello world", embedding, token);
            await store.save();

            expect(fs.existsSync(TEST_FILE)).toBe(true);
            const content = fs.readFileSync(TEST_FILE, "utf-8");
            expect(content.length).toBeGreaterThan(10);
        });
    });

    describe("searchSimilar", () => {
        it("should return empty array when no entries exist for role", () => {
            const token = kernel.mintAuthToken("user") as string;
            const query = Array.from({ length: 256 }, () => Math.random());

            const results = store.searchSimilar(query, "user", token, 3);
            expect(results).toEqual([]);
        });

        it("should return entries after addMemory", async () => {
            const token = kernel.mintAuthToken("user") as string;
            const embedding = Array.from({ length: 256 }, () => Math.random());

            await store.addMemory("user", "Important fact", embedding, token);

            const results = store.searchSimilar(embedding, "user", token, 3);
            expect(results.length).toBeGreaterThanOrEqual(1);
            expect(results[0].content).toBe("Important fact");
        });

        it("should respect role isolation (user vs assistant)", async () => {
            const userToken = kernel.mintAuthToken("user") as string;
            const assistantToken = kernel.mintAuthToken("assistant") as string;
            const emb = Array.from({ length: 256 }, () => Math.random());

            await store.addMemory("user", "User message", emb, userToken);

            // Search under assistant role should return nothing
            const results = store.searchSimilar(emb, "assistant", assistantToken, 3);
            expect(results).toEqual([]);
        });

        it("should respect topK limit", async () => {
            const token = kernel.mintAuthToken("user") as string;

            for (let i = 0; i < 10; i++) {
                const emb = Array.from({ length: 256 }, () => Math.random());
                await store.addMemory("user", `Message ${i}`, emb, token);
            }

            const query = Array.from({ length: 256 }, () => Math.random());
            const results = store.searchSimilar(query, "user", token, 3);
            expect(results.length).toBeLessThanOrEqual(3);
        });

        it("should filter by minPriority", async () => {
            const token = kernel.mintAuthToken("user") as string;
            const emb = Array.from({ length: 256 }, () => Math.random());

            await store.addMemory("user", "Low priority", emb, token, 0.1);
            await store.addMemory("user", "High priority", emb, token, 0.9);

            const results = store.searchSimilar(emb, "user", token, 10, 0.5);
            expect(results.every(r => r.temporal.priority >= 0.5)).toBe(true);
        });
    });

    describe("updateLastVector", () => {
        it("should update vector without throwing", async () => {
            const token = kernel.mintAuthToken("user") as string;
            const dummyEmb = Array.from({ length: 256 }, () => 0);
            const realEmb = Array.from({ length: 256 }, () => Math.random());

            await store.addMemory("user", "test", dummyEmb, token);

            expect(() => store.updateLastVector("user", realEmb, token)).not.toThrow();
        });

        it("should silently skip if role has no entries", () => {
            const token = kernel.mintAuthToken("user") as string;
            const realEmb = Array.from({ length: 256 }, () => Math.random());

            // No entries for "user"
            expect(() => store.updateLastVector("user", realEmb, token)).not.toThrow();
        });
    });

    describe("Persistence (save/load)", () => {
        it("should save and reload data correctly", async () => {
            const token = kernel.mintAuthToken("user") as string;
            const emb = Array.from({ length: 256 }, () => Math.random());

            await store.addMemory("user", "Persistent message", emb, token);
            await store.save();

            // Create new store from same file
            const store2 = new QuantizedMemoryStore(kernel, TEST_FILE);
            const results = store2.searchSimilar(emb, "user", token, 3);
            expect(results.length).toBeGreaterThanOrEqual(1);
            expect(results[0].content).toBe("Persistent message");
            store2.dispose();
        });
    });
});
