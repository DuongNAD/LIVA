#!/usr/bin/env node
/**
 * scripts/stress-1000-rounds.mjs
 * =========================================================================
 * Automated Continuous Synthetic Conversation Stress Benchmark (F12)
 *
 * Runs 1,000 continuous rounds across 3 distinct execution phases:
 * - Phase A (Rounds 1–500): Normal multi-turn dialogue, WAL state persistence,
 *   periodic memory recall, and PRAGMA integrity_check.
 * - Phase B (Rounds 501–750): Rapid barge-in & interruption storms (10-150ms),
 *   verifying immediate preemption (<20ms) and SpeakerEpochGate stale frame rejection.
 * - Phase C (Rounds 751–1000): Adversarial payloads (oversized prompts, malformed frames,
 *   NaN/Inf samples, queue backpressure), asserting zero runtime panics.
 * - Continuous Windows Memory Telemetry: WorkingSetSize < 4.0 GB and net drift < 50 MB.
 * =========================================================================
 */

import { spawnSync } from 'node:child_process';
import path from 'node:path';

const ROOT = path.resolve(import.meta.dirname, '..');

console.log('=======================================================================');
console.log('    LIVA >=1,000-ROUND CONTINUOUS CONVERSATION STRESS HARNESS (F12)    ');
console.log('=======================================================================\n');

console.log('>>> Launching Native Core 1,000-Round Stress Test (Rust)...');

const cargoArgs = [
  'test',
  '-p',
  'liva-native-core',
  '--test',
  'continuous_stress_1000_rounds',
  '-j',
  '2',
  '--',
  '--test-threads',
  '1',
  '--nocapture',
];

const startTime = Date.now();
const proc = spawnSync('cargo', cargoArgs, {
  cwd: ROOT,
  stdio: 'inherit',
  env: process.env,
  shell: true,
});
const totalElapsedSec = ((Date.now() - startTime) / 1000).toFixed(2);

if (proc.status !== 0) {
  console.error('\n[FAILED] 1,000-Round Continuous Stress Test exited with code:', proc.status);
  process.exit(proc.status ?? 1);
}

console.log(`\n>>> 1,000-Round Continuous Stress Test Completed Successfully in ${totalElapsedSec}s.`);
console.log('=======================================================================');
console.log('   ALL 1,000 CONTINUOUS STRESS ROUNDS PASSED WITH ZERO DETECTED LEAKS  ');
console.log('=======================================================================\n');
process.exit(0);
