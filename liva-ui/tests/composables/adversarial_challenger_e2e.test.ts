import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { FootPlantIK } from "../../src/composables/footPlantIK";
import { useSpeakerPlayback } from "../../src/composables/useSpeakerPlayback";
import { AVATAR_EMOTIONS, AVATAR_ACTIONS, AvatarControlTagStream } from "../../src/utils/avatarControlTags";

vi.mock("vue", () => ({
  onUnmounted: vi.fn(),
}));

vi.mock("../../src/utils/logger", () => ({
  logger: {
    warn: vi.fn(),
  },
}));

describe("Adversarial Challenger E2E Stress Suite", () => {
  // -------------------------------------------------------------------------
  // 1. FOOTPLANT IK LOCOMOTION ZERO-DRIFT & 60FPS BUDGET STRESS
  // -------------------------------------------------------------------------
  describe("FootPlantIK Adversarial Stress", () => {
    it("guarantees absolute zero horizontal drift across 10,000 multi-state locomotion cycles", () => {
      const ik = new FootPlantIK();
      const states = ["walk", "run", "idle", "jump"] as const;

      const t0 = performance.now();
      let maxAbsX = 0;
      let maxAbsZ = 0;
      let maxAbsY = 0;

      for (let i = 0; i < 10000; i++) {
        const state = states[i % states.length];
        const cycleProgress = (i % 60) / 60;
        const leftY = Math.sin(cycleProgress * Math.PI * 2) * 0.05;
        const rightY = -leftY;
        const leftX = (i * 0.01) % 10;
        const rightX = leftX + 0.2;

        const correction = ik.update({
          state,
          leftFoot: { x: leftX, y: leftY, z: 0.1 },
          rightFoot: { x: rightX, y: rightY, z: -0.1 },
          delta: 1 / 60,
        });

        maxAbsX = Math.max(maxAbsX, Math.abs(correction.x));
        maxAbsZ = Math.max(maxAbsZ, Math.abs(correction.z));
        maxAbsY = Math.max(maxAbsY, Math.abs(correction.y));
      }
      const elapsedMs = performance.now() - t0;

      // Horizontal offsets MUST remain strictly zero
      expect(maxAbsX).toBe(0);
      expect(maxAbsZ).toBe(0);
      expect(maxAbsY).toBeLessThanOrEqual(0.3);

      // 10,000 updates should complete in < 100ms
      expect(elapsedMs).toBeLessThan(100);
      const updatesPerSec = (10000 / elapsedMs) * 1000;
      expect(updatesPerSec).toBeGreaterThan(60); // Easily satisfying 60fps frame budget
    });

    it("absorbs extreme vertical terrain steps with smooth non-snapping damping", () => {
      const ik = new FootPlantIK();
      // Lock left foot on ground y = 0
      ik.update({
        state: "walk",
        leftFoot: { x: 0, y: 0, z: 0 },
        rightFoot: { x: 0.2, y: 0.1, z: 0 },
        delta: 1 / 60,
      });

      // Sudden terrain drop to y = 0.1
      const diffs: number[] = [];
      let lastY = 0;
      for (let frame = 0; frame < 30; frame++) {
        const correction = ik.update({
          state: "walk",
          leftFoot: { x: 0.05, y: 0.1, z: 0 },
          rightFoot: { x: 0.25, y: 0.15, z: 0 },
          delta: 1 / 60,
        });
        diffs.push(Math.abs(correction.y - lastY));
        lastY = correction.y;
      }

      // Max single-frame vertical velocity delta must be smooth (no teleport snap)
      const maxDelta = Math.max(...diffs);
      expect(maxDelta).toBeLessThan(0.05);
      expect(lastY).toBeCloseTo(-0.1, 2);
    });
  });

  // -------------------------------------------------------------------------
  // 2. AUDIO BARGE-IN 15MS PREEMPTION & GAIN RAMP-DOWN STRESS
  // -------------------------------------------------------------------------
  describe("Audio Barge-In Preemption Adversarial Stress", () => {
    const decodeAudioData = vi.fn();
    const sources: Array<{ stop: ReturnType<typeof vi.fn>; connect: ReturnType<typeof vi.fn> }> = [];
    const gains: Array<{
      gain: {
        setValueAtTime: ReturnType<typeof vi.fn>;
        linearRampToValueAtTime: ReturnType<typeof vi.fn>;
      };
    }> = [];

    class MockAudioContext {
      state = "running";
      currentTime = 10.0;
      destination = {};
      decodeAudioData = decodeAudioData;
      resume = vi.fn().mockResolvedValue(undefined);
      close = vi.fn().mockResolvedValue(undefined);
      createGain = vi.fn(() => {
        const g = {
          connect: vi.fn(),
          gain: {
            value: 1,
            setValueAtTime: vi.fn(),
            linearRampToValueAtTime: vi.fn(),
          },
        };
        gains.push(g);
        return g;
      });
      createBuffer = vi.fn((_c: number, len: number, rate: number) => ({
        duration: len / rate,
        copyToChannel: vi.fn(),
      }));
      createBufferSource = vi.fn(() => {
        const s = {
          connect: vi.fn(),
          start: vi.fn(),
          stop: vi.fn(),
          onended: null as (() => void) | null,
        };
        sources.push(s);
        return s;
      });
    }

    beforeEach(() => {
      sources.length = 0;
      gains.length = 0;
      vi.stubGlobal("AudioContext", MockAudioContext);
    });

    afterEach(() => {
      vi.unstubAllGlobals();
    });

    function pcmPayload(): Uint8Array {
      const p = new Uint8Array(12);
      const v = new DataView(p.buffer);
      v.setUint32(0, 1, true);
      v.setUint32(4, 16000, true);
      v.setFloat32(8, 0.25, true);
      return p;
    }

    it("handles 50 rapid consecutive barge-in interruptions without unhandled exceptions or gain corruption", async () => {
      const speaker = useSpeakerPlayback({ useMasterGain: true });

      for (let burst = 0; burst < 50; burst++) {
        await speaker.enqueueSpeakerPayload(pcmPayload());
        expect(speaker.isPlaying()).toBe(true);

        // Preempt / Barge-in immediately
        speaker.stop();
        expect(speaker.isBlocked()).toBe(true);

        const currentMasterGain = gains[gains.length - 1];
        // Verify 15ms rampdown was precisely scheduled
        expect(currentMasterGain.gain.linearRampToValueAtTime).toHaveBeenCalledWith(
          0,
          10.0 + 0.015
        );
        const currentSource = sources[sources.length - 1];
        expect(currentSource.stop).toHaveBeenCalledWith(10.0 + 0.015);

        // Unblock for next turn
        speaker.unblock();
        expect(speaker.isBlocked()).toBe(false);
      }
    });
  });

  // -------------------------------------------------------------------------
  // 3. AVATAR EMOTION & CONTROL TAGS SYMMETRY
  // -------------------------------------------------------------------------
  describe("Avatar Control Tags Symmetry Matrix", () => {
    it("verifies full alignment across all 11 standardized emotion and action tags", () => {
      const all11Tags = [...AVATAR_EMOTIONS, ...AVATAR_ACTIONS];
      expect(all11Tags).toHaveLength(11);

      for (const tag of all11Tags) {
        const stream = new AvatarControlTagStream();
        const res = stream.push(`[${tag}] Xin chào bạn!`);
        expect(res.controls).toHaveLength(1);
        expect(res.controls[0].value).toBe(tag);
        expect(res.text).toBe('Xin chào bạn!');
      }
    });
  });
});

