#!/usr/bin/env node
/**
 * LIVA Intelligent Assistant — Empirical Adversarial Stress Test Suite
 * Milestone 3 Challenger 1: Worklet Frame & Aggregation Stress Challenger
 * Target: liva-ui/src/worklets/mic-capture.worklet.js
 */

import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const ROOT_DIR = path.resolve(__dirname, '..');
const WORKLET_PATH = path.join(ROOT_DIR, 'liva-ui', 'src', 'worklets', 'mic-capture.worklet.js');

class StressReporter {
  constructor() {
    this.passed = 0;
    this.failed = 0;
    this.total = 0;
    this.results = [];
  }

  async run(name, fn) {
    this.total++;
    const t0 = performance.now();
    try {
      await fn();
      const dt = performance.now() - t0;
      this.passed++;
      this.results.push({ name, status: 'PASS', dt });
      console.log('  ✅ [PASS] ' + name + ' (' + dt.toFixed(2) + 'ms)');
    } catch (err) {
      const dt = performance.now() - t0;
      this.failed++;
      this.results.push({ name, status: 'FAIL', dt, error: err.message });
      console.error('  ❌ [FAIL] ' + name + ' (' + dt.toFixed(2) + 'ms)');
      console.error('     Error: ' + err.message + '\n     Stack: ' + err.stack);
    }
  }

  summary() {
    console.log('\n' + '='.repeat(80));
    console.log('WORKLET EMPIRICAL STRESS TEST SUMMARY: ' + this.passed + '/' + this.total + ' PASSED (' + this.failed + ' FAILED)');
    console.log('='.repeat(80));
    return this.failed === 0;
  }
}

async function loadWorkletProcessor() {
  let registeredConstructor = null;

  class MockAudioWorkletProcessor {
    constructor() {
      this.port = {
        postMessage: () => {},
      };
    }
  }

  globalThis.AudioWorkletProcessor = MockAudioWorkletProcessor;
  globalThis.registerProcessor = (name, ctor) => {
    if (name === 'liva-mic-capture') {
      registeredConstructor = ctor;
    }
  };

  const code = fs.readFileSync(WORKLET_PATH, 'utf-8');
  const runFn = new Function('AudioWorkletProcessor', 'registerProcessor', code);
  runFn(MockAudioWorkletProcessor, globalThis.registerProcessor);

  if (!registeredConstructor) {
    throw new Error('Failed to register liva-mic-capture processor from ' + WORKLET_PATH);
  }
  return registeredConstructor;
}

async function main() {
  console.log('='.repeat(80));
  console.log('EMPIRICAL ADVERSARIAL STRESS TEST: LivaMicCaptureProcessor');
  console.log('Target: ' + WORKLET_PATH);
  console.log('='.repeat(80) + '\n');

  const Processor = await loadWorkletProcessor();
  const reporter = new StressReporter();

  // ──────────────────────────────────────────────────────────────────────────
  // Suite 1: 10,000 continuous render quanta (128 samples) with default 256 frameSize
  // ──────────────────────────────────────────────────────────────────────────
  await reporter.run(
    'Suite 1: 10,000 continuous render quanta (128 samples) -> exactly 5,000 frames bit-for-bit',
    async () => {
      const processor = new Processor();
      const emittedFrames = [];
      const transferLists = [];

      processor.port.postMessage = (frame, transfer) => {
        emittedFrames.push(new Float32Array(frame)); // copy since buffer may be detached
        transferLists.push(transfer);
      };

      const TOTAL_QUANTA = 10000;
      const QUANTUM_SIZE = 128;
      const EXPECTED_FRAMES = 5000;
      const FRAME_SIZE = 256;

      // Deterministic pseudo-random float sequence
      const totalSamples = TOTAL_QUANTA * QUANTUM_SIZE;
      const groundTruth = new Float32Array(totalSamples);
      for (let i = 0; i < totalSamples; i++) {
        groundTruth[i] = Math.fround(Math.sin(i * 0.05) * 0.8 + ((i % 257) - 128) / 1000);
      }

      const output = new Float32Array(QUANTUM_SIZE);

      const t0 = performance.now();
      for (let q = 0; q < TOTAL_QUANTA; q++) {
        const inputQuantum = groundTruth.subarray(q * QUANTUM_SIZE, (q + 1) * QUANTUM_SIZE);
        output.fill(0.5); // non-zero to test muting
        const keepAlive = processor.process([[inputQuantum]], [[output]]);
        assert.equal(keepAlive, true, 'process() must return true at quantum ' + q);
      }
      const elapsedMs = performance.now() - t0;
      console.log('     Processed 10,000 quanta (1.28M samples) in ' + elapsedMs.toFixed(2) + 'ms (' + (1.28 / (elapsedMs / 1000)).toFixed(2) + ' MSamples/sec)');

      assert.equal(emittedFrames.length, EXPECTED_FRAMES, 'Must emit exactly ' + EXPECTED_FRAMES + ' frames');

      // Verify bit-for-bit equality and transfer lists
      const gtUintView = new Uint32Array(groundTruth.buffer, groundTruth.byteOffset, groundTruth.length);

      for (let f = 0; f < EXPECTED_FRAMES; f++) {
        const frame = emittedFrames[f];
        assert.equal(frame.length, FRAME_SIZE, 'Frame ' + f + ' length must be ' + FRAME_SIZE);
        assert.equal(transferLists[f]?.length, 1, 'Frame ' + f + ' transferList must contain 1 buffer');

        const frameUintView = new Uint32Array(frame.buffer, frame.byteOffset, frame.length);
        const expectedOffset = f * FRAME_SIZE;

        for (let s = 0; s < FRAME_SIZE; s++) {
          const actualBits = frameUintView[s];
          const expectedBits = gtUintView[expectedOffset + s];
          if (actualBits !== expectedBits) {
            throw new Error(
              'Bit mismatch at frame ' + f + ', sample ' + s + ' (global sample ' + (expectedOffset + s) + '): ' +
              'expected bits 0x' + expectedBits.toString(16) + ' (' + groundTruth[expectedOffset + s] + '), ' +
              'got 0x' + actualBits.toString(16) + ' (' + frame[s] + ')'
            );
          }
        }
      }
      console.log('     Bit-for-bit integrity verified: 1,280,000 / 1,280,000 samples perfectly matched.');
    }
  );

  // ──────────────────────────────────────────────────────────────────────────
  // Suite 2: Non-standard quantum sizes & ring buffer boundary safety
  // ──────────────────────────────────────────────────────────────────────────
  await reporter.run(
    'Suite 2.1: Sub-quantum frames (quantum size = 64 samples, 2000 quanta -> 500 frames of 256)',
    async () => {
      const processor = new Processor();
      const emittedFrames = [];
      processor.port.postMessage = (frame) => emittedFrames.push(new Float32Array(frame));

      const QUANTA = 2000;
      const Q_SIZE = 64;
      const totalSamples = QUANTA * Q_SIZE;
      const groundTruth = new Float32Array(totalSamples);
      for (let i = 0; i < totalSamples; i++) {
        groundTruth[i] = Math.fround(Math.cos(i * 0.03) * 0.5);
      }

      for (let q = 0; q < QUANTA; q++) {
        const qInput = groundTruth.subarray(q * Q_SIZE, (q + 1) * Q_SIZE);
        const res = processor.process([[qInput]], [[]]);
        assert.equal(res, true);
      }

      assert.equal(emittedFrames.length, 500);
      const gtUint = new Uint32Array(groundTruth.buffer);
      for (let f = 0; f < 500; f++) {
        const fUint = new Uint32Array(emittedFrames[f].buffer);
        for (let s = 0; s < 256; s++) {
          assert.equal(fUint[s], gtUint[f * 256 + s]);
        }
      }
    }
  );

  await reporter.run(
    'Suite 2.2: 1-to-1 quantum size (quantum size = 256 samples, 1000 quanta -> 1000 frames of 256)',
    async () => {
      const processor = new Processor();
      const emittedFrames = [];
      processor.port.postMessage = (frame) => emittedFrames.push(new Float32Array(frame));

      const QUANTA = 1000;
      const Q_SIZE = 256;
      const totalSamples = QUANTA * Q_SIZE;
      const groundTruth = new Float32Array(totalSamples);
      for (let i = 0; i < totalSamples; i++) {
        groundTruth[i] = Math.fround(i * 0.001);
      }

      for (let q = 0; q < QUANTA; q++) {
        const qInput = groundTruth.subarray(q * Q_SIZE, (q + 1) * Q_SIZE);
        processor.process([[qInput]], [[]]);
      }

      assert.equal(emittedFrames.length, 1000);
      const gtUint = new Uint32Array(groundTruth.buffer);
      for (let f = 0; f < 1000; f++) {
        const fUint = new Uint32Array(emittedFrames[f].buffer);
        for (let s = 0; s < 256; s++) {
          assert.equal(fUint[s], gtUint[f * 256 + s]);
        }
      }
    }
  );

  await reporter.run(
    'Suite 2.3: Non-aligned super-quantum (quantum size = 384 samples, 2000 quanta -> 3000 frames of 256)',
    async () => {
      const processor = new Processor();
      const emittedFrames = [];
      processor.port.postMessage = (frame) => emittedFrames.push(new Float32Array(frame));

      const QUANTA = 2000;
      const Q_SIZE = 384; // 1.5 frames per quantum
      const totalSamples = QUANTA * Q_SIZE;
      const groundTruth = new Float32Array(totalSamples);
      for (let i = 0; i < totalSamples; i++) {
        groundTruth[i] = Math.fround(Math.sin(i * 0.07) * 0.6);
      }

      for (let q = 0; q < QUANTA; q++) {
        const qInput = groundTruth.subarray(q * Q_SIZE, (q + 1) * Q_SIZE);
        processor.process([[qInput]], [[]]);
      }

      assert.equal(emittedFrames.length, 3000, 'Expected 3000 frames from 768000 samples, got ' + emittedFrames.length);
      const gtUint = new Uint32Array(groundTruth.buffer);
      for (let f = 0; f < 3000; f++) {
        const fUint = new Uint32Array(emittedFrames[f].buffer);
        for (let s = 0; s < 256; s++) {
          assert.equal(fUint[s], gtUint[f * 256 + s]);
        }
      }
    }
  );

  await reporter.run(
    'Suite 2.4: Prime & irregular chunk sizes (e.g. 7, 13, 79, 100, 500 samples)',
    async () => {
      const processor = new Processor();
      const emittedFrames = [];
      processor.port.postMessage = (frame) => emittedFrames.push(new Float32Array(frame));

      const chunkSizes = [7, 13, 79, 100, 500, 1, 3, 255, 257, 128, 64];
      let totalSamplesFed = 0;
      const fedSamples = [];

      for (let round = 0; round < 100; round++) {
        for (const size of chunkSizes) {
          const chunk = new Float32Array(size);
          for (let i = 0; i < size; i++) {
            const val = Math.fround((totalSamplesFed + i) * 0.005);
            chunk[i] = val;
            fedSamples.push(val);
          }
          totalSamplesFed += size;
          processor.process([[chunk]], [[]]);
        }
      }

      const expectedFrames = Math.floor(totalSamplesFed / 256);
      assert.equal(emittedFrames.length, expectedFrames);

      const gtArray = new Float32Array(fedSamples);
      const gtUint = new Uint32Array(gtArray.buffer);
      for (let f = 0; f < expectedFrames; f++) {
        const fUint = new Uint32Array(emittedFrames[f].buffer);
        for (let s = 0; s < 256; s++) {
          assert.equal(fUint[s], gtUint[f * 256 + s]);
        }
      }
    }
  );

  // ──────────────────────────────────────────────────────────────────────────
  // Suite 3: Empty inputs, missing channels & edge cases
  // ──────────────────────────────────────────────────────────────────────────
  await reporter.run(
    'Suite 3: Empty inputs, nullish channels, empty Float32Array',
    async () => {
      const processor = new Processor();
      let emitCount = 0;
      processor.port.postMessage = () => emitCount++;

      // 1. Completely empty inputs
      assert.equal(processor.process([], []), true);
      assert.equal(processor.process([[]], [[]]), true);

      // 2. Undefined / null channel entries
      assert.equal(processor.process([[undefined]], [[undefined]]), true);
      assert.equal(processor.process([[null]], [[null]]), true);

      // 3. Zero-length Float32Array
      assert.equal(processor.process([[new Float32Array(0)]], [[new Float32Array(0)]]), true);

      assert.equal(emitCount, 0, 'No frames should be emitted for empty/missing inputs');

      // 4. Recovery check: feed 256 samples after empty inputs
      const validChunk = new Float32Array(256);
      validChunk.fill(0.42);
      assert.equal(processor.process([[validChunk]], [[]]), true);
      assert.equal(emitCount, 1, 'Should seamlessly resume emission upon valid input');
    }
  );

  // ──────────────────────────────────────────────────────────────────────────
  // Suite 4: Silent output / Acoustic feedback protection
  // ──────────────────────────────────────────────────────────────────────────
  await reporter.run(
    'Suite 4: Microphone output muting (prevent acoustic feedback to speakers)',
    async () => {
      const processor = new Processor();
      const output = new Float32Array(128);

      // Dirty output buffer before process
      output.fill(0.999);

      const input = new Float32Array(128);
      input.fill(0.777);

      const ok = processor.process([[input]], [[output]]);
      assert.equal(ok, true);

      for (let i = 0; i < output.length; i++) {
        assert.equal(output[i], 0, 'Output sample at ' + i + ' must be strictly 0 to prevent mic bleed');
      }

      // Safe handling when outputs array is empty
      assert.doesNotThrow(() => {
        processor.process([[input]], []);
      });
    }
  );

  // ──────────────────────────────────────────────────────────────────────────
  // Suite 5: Float edge cases (NaN, Infinity, -Infinity, Subnormals)
  // ──────────────────────────────────────────────────────────────────────────
  await reporter.run(
    'Suite 5: Special float values: NaN, +Infinity, -Infinity, Denormals preserve bit integrity',
    async () => {
      const processor = new Processor();
      const emitted = [];
      processor.port.postMessage = (frame) => emitted.push(new Float32Array(frame));

      const specialInput = new Float32Array(256);
      specialInput[0] = NaN;
      specialInput[1] = Infinity;
      specialInput[2] = -Infinity;
      specialInput[3] = -0.0;
      specialInput[4] = 1.401298464324817e-45; // Minimum positive subnormal float32

      for (let i = 5; i < 256; i++) {
        specialInput[i] = Math.fround(0.123);
      }

      processor.process([[specialInput]], [[]]);
      assert.equal(emitted.length, 1);
      const res = emitted[0];

      assert.ok(Number.isNaN(res[0]), 'Sample 0 should be NaN');
      assert.equal(res[1], Infinity, 'Sample 1 should be +Infinity');
      assert.equal(res[2], -Infinity, 'Sample 2 should be -Infinity');
      assert.ok(Object.is(res[3], -0.0), 'Sample 3 should be -0.0');
      assert.equal(res[4], 1.401298464324817e-45, 'Sample 4 should preserve denormal');

      // Subsequent normal block
      const normalInput = new Float32Array(256);
      normalInput.fill(0.5);
      processor.process([[normalInput]], [[]]);
      assert.equal(emitted.length, 2);
      assert.equal(emitted[1][0], 0.5, 'Next frame must be unaffected');
    }
  );

  // ──────────────────────────────────────────────────────────────────────────
  // Suite 6: TransferList Buffer Detachment & Allocation Safety
  // ──────────────────────────────────────────────────────────────────────────
  await reporter.run(
    'Suite 6: ArrayBuffer detachment safety and unique allocations',
    async () => {
      const processor = new Processor();
      const detachedBuffers = [];

      processor.port.postMessage = (frame, transferList) => {
        assert.ok(Array.isArray(transferList));
        assert.equal(transferList[0], frame.buffer);
        detachedBuffers.push(frame.buffer);
      };

      for (let i = 0; i < 10; i++) {
        const input = new Float32Array(256);
        input.fill(i + 1);
        processor.process([[input]], [[]]);
      }

      assert.equal(detachedBuffers.length, 10);
      const bufferSet = new Set(detachedBuffers);
      assert.equal(bufferSet.size, 10, 'Every emitted frame buffer must be unique (no buffer reuse)');
    }
  );

  // ──────────────────────────────────────────────────────────────────────────
  // Suite 7: Constructor options handling & custom frame sizes
  // ──────────────────────────────────────────────────────────────────────────
  await reporter.run(
    'Suite 7: Options variations (undefined, empty, custom frameSize: 128, 512, 1024)',
    async () => {
      // 1. undefined options
      const pDefaultUndef = new Processor();
      assert.equal(pDefaultUndef.frameSize, 256);

      // 2. explicit undefined
      const pExplicitUndef = new Processor(undefined);
      assert.equal(pExplicitUndef.frameSize, 256);

      // 3. empty object
      const pEmpty = new Processor({});
      assert.equal(pEmpty.frameSize, 256);

      // 4. empty processorOptions
      const pEmptyOpts = new Processor({ processorOptions: {} });
      assert.equal(pEmptyOpts.frameSize, 256);

      // 5. custom frame sizes
      for (const customSize of [128, 512, 1024]) {
        const pCustom = new Processor({ processorOptions: { frameSize: customSize } });
        assert.equal(pCustom.frameSize, customSize);

        let emitted = 0;
        pCustom.port.postMessage = (f) => {
          assert.equal(f.length, customSize);
          emitted++;
        };

        const input = new Float32Array(customSize);
        pCustom.process([[input]], [[]]);
        assert.equal(emitted, 1);
      }
    }
  );

  // ──────────────────────────────────────────────────────────────────────────
  // Execution Summary
  // ──────────────────────────────────────────────────────────────────────────
  const success = reporter.summary();
  if (!success) {
    process.exit(1);
  }
}

main().catch((err) => {
  console.error('FATAL EXCEPTION:', err);
  process.exit(1);
});
