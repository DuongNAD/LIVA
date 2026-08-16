#!/usr/bin/env node
/**
 * LIVA Intelligent Assistant — Genuine End-to-End WebSocket Gateway Test Runner
 *
 * Runs a REAL end-to-end test suite against a live spawned LIVA native core binary
 * over actual TCP WebSocket connections.
 *
 * Scope:
 * - Section 1: Protocol Framing & Correlation (event vs IPC, req_id, malformed JSON, oversized frames, unknown commands)
 * - Section 2: Authorization Boundary & Origin Checking (Origin allowlist, query principal spoofing, command allowlists)
 * - Section 3: Reachable Core Commands Contract (ping, status, llm:health_check real field shapes & types)
 * - Section 4: chat:completion Protocol & Streaming (payload validation, model absent handling, IPC streaming)
 * - Section 5: Voice Pipeline Lifecycle (voice:stt_*, voice:tts_* lifecycle & absent model handling)
 * - Section 6: Concurrency & Connection Isolation (5 parallel client connections, interleaved commands, selective close)
 * - Section 7: Architectural Boundary Audit & Feature Mapping (Explicit F1-F15 allowlist mapping and Rust test pointers)
 *
 * Usage:
 *   node scripts/e2e-test-suite.mjs                       # Spawn debug binary on port 8099 and run suite
 *   node scripts/e2e-test-suite.mjs --release             # Use release binary
 *   node scripts/e2e-test-suite.mjs --port 8099           # Custom port
 *   node scripts/e2e-test-suite.mjs --no-spawn            # Connect to already running gateway
 *   node scripts/e2e-test-suite.mjs --json                # Output machine-readable JSON
 *   node scripts/e2e-test-suite.mjs --section 1           # Run specific section only
 */

import { E2EReporter, spawnGatewayServer, DEFAULT_PORT } from './e2e/helpers.mjs'
import { runProtocolFramingTests } from './e2e/01-protocol-framing.mjs'
import { runAuthorizationOriginTests } from './e2e/02-authorization-origin.mjs'
import { runReachableCommandsTests } from './e2e/03-reachable-commands.mjs'
import { runChatCompletionTests } from './e2e/04-chat-completion.mjs'
import { runVoiceLifecycleTests } from './e2e/05-voice-lifecycle.mjs'
import { runConcurrencyTests } from './e2e/06-concurrency.mjs'
import { runBoundaryAuditReport } from './e2e/07-boundary-audit.mjs'

const argv = process.argv.slice(2)
const getArg = (name) => {
  const i = argv.indexOf(name)
  return i > -1 ? argv[i + 1] : null
}

const isJson = argv.includes('--json')
const isRelease = argv.includes('--release')
const noSpawn = argv.includes('--no-spawn')
const customPort = getArg('--port') || process.env.PORT
const port = Number(customPort || DEFAULT_PORT)
const customBin = getArg('--bin')
const targetSection = getArg('--section') || getArg('--tier')
const sectionNum = targetSection ? parseInt(targetSection, 10) : null

async function main() {
  const reporter = new E2EReporter({ json: isJson })
  reporter.startSuite()

  let serverHandle = null

  if (!noSpawn) {
    try {
      if (!isJson) {
        console.log(`[Spawn] Đang khởi chạy LIVA Gateway (${isRelease ? 'release' : 'debug'}) trên cổng ${port}...`)
      }
      serverHandle = await spawnGatewayServer({
        port,
        binPath: customBin,
        release: isRelease,
      })
      if (!isJson) {
        console.log(`[Spawn] Gateway sẵn sàng tại ws://127.0.0.1:${port}/ws (PID: ${serverHandle.process.pid})\n`)
      }
    } catch (err) {
      console.error(`\n❌ Không thể khởi động LIVA Gateway: ${err.message}`)
      process.exit(1)
    }
  } else {
    if (!isJson) {
      console.log(`[Attach] Kết nối tới Gateway đang chạy tại ws://127.0.0.1:${port}/ws (--no-spawn)\n`)
    }
  }

  // Ensure clean shutdown on exit / interrupt
  const cleanup = () => {
    if (serverHandle) {
      serverHandle.stop()
      serverHandle = null
    }
  }
  process.on('exit', cleanup)
  process.on('SIGINT', () => { cleanup(); process.exit(130) })
  process.on('SIGTERM', () => { cleanup(); process.exit(143) })

  try {
    if (!sectionNum || sectionNum === 1) await runProtocolFramingTests(reporter, port)
    if (!sectionNum || sectionNum === 2) await runAuthorizationOriginTests(reporter, port)
    if (!sectionNum || sectionNum === 3) await runReachableCommandsTests(reporter, port)
    if (!sectionNum || sectionNum === 4) await runChatCompletionTests(reporter, port)
    if (!sectionNum || sectionNum === 5) await runVoiceLifecycleTests(reporter, port)
    if (!sectionNum || sectionNum === 6) await runConcurrencyTests(reporter, port)
    if (!sectionNum || sectionNum === 7) await runBoundaryAuditReport(reporter, port)
  } catch (err) {
    console.error('\n❌ Lỗi bất thường khi chạy suite:', err)
  } finally {
    cleanup()
  }

  const success = reporter.endSuite()
  process.exit(success ? 0 : 1)
}

main().catch((err) => {
  console.error('Lỗi nghiêm trọng:', err)
  process.exit(1)
})
