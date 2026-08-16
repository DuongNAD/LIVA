/**
 * LIVA WebSocket Gateway End-to-End Test Suite — Helpers & Utilities
 *
 * NOTE: This file intentionally contains ZERO reimplementations of LIVA algorithms
 * (no fake StateGraph, no fake SecretScrubber, no fake computeRRF, no fake crypto).
 * All tests communicate with the REAL running Rust core (liva-native-core) over
 * genuine TCP WebSocket connections.
 */

import net from 'node:net'
import crypto from 'node:crypto'
import path from 'node:path'
import fs from 'node:fs'
import { spawn } from 'node:child_process'
import { EventEmitter } from 'node:events'

export const ROOT_DIR = path.resolve(import.meta.dirname, '../..')
export const DEFAULT_PORT = 8099
export const DEFAULT_ORIGIN_ALLOWED = 'http://localhost:5173'
export const DEFAULT_ORIGIN_DISALLOWED = 'http://evil.example.com'

/**
 * Connect to LIVA WebSocket server using raw TCP (RFC 6455)
 */
export function connectWebSocket({
  host = '127.0.0.1',
  port = DEFAULT_PORT,
  path: wsPath = '/ws',
  origin = DEFAULT_ORIGIN_ALLOWED,
  authorization = null,
  timeout = 10000,
} = {}) {
  return new Promise((resolve) => {
    const bus = new EventEmitter()
    const key = crypto.randomBytes(16).toString('base64')
    const sock = net.connect({ host, port })
    let handshaken = false
    let buffer = Buffer.alloc(0)
    let isClosed = false

    const fail = (reason, statusCode = null) => {
      if (isClosed) return
      isClosed = true
      try { sock.destroy() } catch { /* ignore */ }
      resolve({ ok: false, reason, statusCode, ws: null })
    }

    const timer = setTimeout(() => fail('Handshake timed out', null), timeout)
    sock.on('error', (err) => {
      clearTimeout(timer)
      fail(String(err?.message || err), null)
    })

    sock.on('close', () => {
      isClosed = true
      bus.emit('close')
    })

    sock.on('connect', () => {
      const headers = [
        `GET ${wsPath} HTTP/1.1`,
        `Host: ${host}:${port}`,
        'Upgrade: websocket',
        'Connection: Upgrade',
        `Sec-WebSocket-Key: ${key}`,
        'Sec-WebSocket-Version: 13',
      ]
      if (origin !== null && origin !== undefined) {
        headers.push(`Origin: ${origin}`)
      }
      if (authorization) {
        headers.push(`Authorization: ${authorization}`)
      }
      headers.push('', '')
      sock.write(headers.join('\r\n'))
    })

    sock.on('data', (chunk) => {
      buffer = Buffer.concat([buffer, chunk])

      if (!handshaken) {
        const headerEnd = buffer.indexOf('\r\n\r\n')
        if (headerEnd < 0) return
        const headerText = buffer.subarray(0, headerEnd).toString('latin1')
        const firstLine = headerText.split('\r\n')[0] || ''
        const statusMatch = firstLine.match(/HTTP\/1\.[01]\s+(\d{3})/)
        const statusCode = statusMatch ? parseInt(statusMatch[1], 10) : 0
        buffer = buffer.subarray(headerEnd + 4)
        clearTimeout(timer)

        if (statusCode !== 101) {
          return fail(`HTTP ${statusCode}`, statusCode)
        }
        handshaken = true

        const wsClient = {
          rawSocket: sock,
          on: bus.on.bind(bus),
          off: bus.off.bind(bus),
          once: bus.once.bind(bus),

          sendRawText(text) {
            sendFrame(0x1, Buffer.from(text, 'utf8'))
          },

          sendRawBytes(buf) {
            sock.write(buf)
          },

          sendFrame(opcode, payloadBuf) {
            sendFrame(opcode, payloadBuf)
          },

          sendEvent(eventName, payload = {}, timeoutMs = 20000) {
            return new Promise((res) => {
              const reqTimer = setTimeout(() => {
                bus.off('message_json', listener)
                res({ event: null, error: `Timed out waiting for ${eventName}_response (${timeoutMs}ms)` })
              }, timeoutMs)

              const listener = (msg) => {
                if (msg && (msg.event === `${eventName}_response` || msg.event === `${eventName}_error`)) {
                  clearTimeout(reqTimer)
                  bus.off('message_json', listener)
                  res(msg)
                }
              }

              bus.on('message_json', listener)
              sendFrame(0x1, Buffer.from(JSON.stringify({ event: eventName, payload }), 'utf8'))
            })
          },

          sendIpc(command, payload = {}, id = null, timeoutMs = 20000) {
            const reqId = id || `req-${crypto.randomBytes(8).toString('hex')}`
            return new Promise((res) => {
              const reqTimer = setTimeout(() => {
                bus.off('message_json', listener)
                res({ id: reqId, status: 'error', error: `Timed out waiting for IPC response to ${command} (${timeoutMs}ms)` })
              }, timeoutMs)

              const listener = (msg) => {
                if (msg && msg.id === reqId) {
                  clearTimeout(reqTimer)
                  bus.off('message_json', listener)
                  res(msg)
                }
              }

              bus.on('message_json', listener)
              sendFrame(0x1, Buffer.from(JSON.stringify({ id: reqId, command, payload }), 'utf8'))
            })
          },

          waitForNextJson(timeoutMs = 15000) {
            return new Promise((res) => {
              const waitTimer = setTimeout(() => {
                bus.off('message_json', listener)
                res(null)
              }, timeoutMs)

              const listener = (msg) => {
                clearTimeout(waitTimer)
                bus.off('message_json', listener)
                res(msg)
              }

              bus.on('message_json', listener)
            })
          },

          close() {
            try {
              // Send WS close frame (0x8)
              sendFrame(0x8, Buffer.alloc(0))
            } catch { /* ignore */ }
            try { sock.destroy() } catch { /* ignore */ }
          },
        }

        resolve({ ok: true, statusCode: 101, ws: wsClient })
      }

      // Parse RFC 6455 WebSocket frames
      while (buffer.length >= 2) {
        const fin = (buffer[0] & 0x80) !== 0
        const opcode = buffer[0] & 0x0f
        const hasMask = (buffer[1] & 0x80) !== 0
        let payloadLen = buffer[1] & 0x7f
        let offset = 2

        if (payloadLen === 126) {
          if (buffer.length < 4) return
          payloadLen = buffer.readUInt16BE(2)
          offset = 4
        } else if (payloadLen === 127) {
          if (buffer.length < 10) return
          payloadLen = Number(buffer.readBigUInt64BE(2))
          offset = 10
        }

        const maskLen = hasMask ? 4 : 0
        if (buffer.length < offset + maskLen + payloadLen) return

        let body = buffer.subarray(offset + maskLen, offset + maskLen + payloadLen)
        if (hasMask) {
          const mask = buffer.subarray(offset, offset + 4)
          body = Buffer.from(body.map((b, i) => b ^ mask[i % 4]))
        }
        buffer = buffer.subarray(offset + maskLen + payloadLen)

        if (opcode === 0x8) {
          // Close frame
          try { sock.destroy() } catch { /* ignore */ }
          bus.emit('close')
          return
        }
        if (opcode === 0x9) {
          // Ping -> send Pong
          sendFrame(0xa, body)
          continue
        }
        if (opcode === 0x1 && fin) {
          // Text frame
          const text = body.toString('utf8')
          bus.emit('message_text', text)
          try {
            const parsed = JSON.parse(text)
            bus.emit('message_json', parsed)
          } catch {
            bus.emit('message_raw', body)
          }
        } else if (opcode === 0x2) {
          // Binary frame
          bus.emit('message_binary', body)
        }
      }
    })

    function sendFrame(opcode, payload) {
      if (sock.destroyed) return
      const mask = crypto.randomBytes(4)
      const p = Buffer.isBuffer(payload) ? payload : Buffer.from(payload)
      const masked = Buffer.from(p.map((b, i) => b ^ mask[i % 4]))
      let header
      if (p.length < 126) {
        header = Buffer.from([0x80 | opcode, 0x80 | p.length])
      } else if (p.length < 65536) {
        header = Buffer.alloc(4)
        header[0] = 0x80 | opcode
        header[1] = 0x80 | 126
        header.writeUInt16BE(p.length, 2)
      } else {
        header = Buffer.alloc(10)
        header[0] = 0x80 | opcode
        header[1] = 0x80 | 127
        header.writeBigUInt64BE(BigInt(p.length), 2)
      }
      sock.write(Buffer.concat([header, mask, masked]))
    }
  })
}

/**
 * Check if TCP port is accepting connections
 */
export function isPortOpen(port, host = '127.0.0.1') {
  return new Promise((resolve) => {
    const s = net.connect({ host, port })
    s.on('connect', () => {
      s.destroy()
      resolve(true)
    })
    s.on('error', () => resolve(false))
  })
}

/**
 * Sleep helper
 */
export const sleep = (ms) => new Promise((r) => setTimeout(r, ms))

/**
 * Spawn LIVA native core binary and wait for WebSocket port readiness
 */
export async function spawnGatewayServer({
  port = DEFAULT_PORT,
  binPath = null,
  release = false,
  extraEnv = {},
} = {}) {
  const profile = release ? 'release' : 'debug'
  const exeName = process.platform === 'win32' ? 'liva-native-core.exe' : 'liva-native-core'
  const resolvedBin = binPath ? path.resolve(ROOT_DIR, binPath) : path.resolve(ROOT_DIR, 'target', profile, exeName)

  if (!fs.existsSync(resolvedBin)) {
    throw new Error(`Binary không tồn tại: ${resolvedBin}\nCần build trước: cargo build ${release ? '--release ' : ''}--bin liva-native-core`)
  }

  // Check port pre-condition
  if (await isPortOpen(port)) {
    throw new Error(`Cổng ${port} ĐANG CÓ tiến trình khác lắng nghe! Tắt tiến trình đó hoặc chỉ định cổng khác qua --port.`)
  }

  const child = spawn(resolvedBin, [], {
    cwd: ROOT_DIR,
    // Stdin MUST be open pipe: LIVA core monitors stdin for IPC and terminates cleanly on EOF
    stdio: ['pipe', 'pipe', 'pipe'],
    env: {
      ...process.env,
      LIVA_SERVER_PORT: String(port),
      LIVA_DB_IN_MEMORY: '1',
      LIVA_ENCRYPTION_KEY: process.env.LIVA_ENCRYPTION_KEY || '00000000000000000000000000000000',
      ...extraEnv,
    },
  })

  const stderrLogs = []
  child.stderr?.on('data', (d) => {
    const str = d.toString('utf8')
    stderrLogs.push(str)
    if (stderrLogs.length > 50) stderrLogs.shift()
  })
  child.stdout?.resume()

  let isStopping = false
  const stop = () => {
    if (isStopping) return
    isStopping = true
    try { child.stdin.end() } catch { /* ignore */ }
    try { child.kill('SIGKILL') } catch { /* ignore */ }
  }

  child.on('exit', (code) => {
    if (!isStopping && code !== 0) {
      console.error(`\n[Server] LIVA Gateway thoát sớm với mã: ${code}`)
      if (stderrLogs.length > 0) {
        console.error(`[Server Log]\n${stderrLogs.join('')}`)
      }
    }
  })

  // Poll port readiness up to 60s
  let ready = false
  for (let i = 0; i < 120 && !ready; i++) {
    ready = await isPortOpen(port)
    if (!ready) await sleep(500)
  }

  if (!ready) {
    stop()
    const logHint = stderrLogs.length > 0 ? `\nLogs:\n${stderrLogs.join('')}` : ''
    throw new Error(`LIVA Gateway không mở cổng ${port} sau 60s.${logHint}`)
  }

  return {
    port,
    binPath: resolvedBin,
    process: child,
    stop,
  }
}

/**
 * Test Reporter for E2E Suite
 */
export class E2EReporter {
  constructor({ json = false } = {}) {
    this.jsonMode = json
    this.results = {
      passed: 0,
      failed: 0,
      skipped: 0,
      total: 0,
      durationMs: 0,
      sections: [],
      failures: [],
      skips: [],
    }
    this.currentSection = null
    this.startTime = 0
  }

  startSuite() {
    this.startTime = performance.now()
    if (!this.jsonMode) {
      console.log('='.repeat(80))
      console.log('🚀 LIVA NATIVE CORE — REAL END-TO-END WEBSOCKET GATEWAY TEST SUITE')
      console.log('='.repeat(80))
      console.log('Testing genuine Rust dispatch, WebSocket protocol framing, and authorization.\n')
    }
  }

  startSection(num, title) {
    this.currentSection = {
      num,
      title,
      passed: 0,
      failed: 0,
      skipped: 0,
      total: 0,
      tests: [],
    }
    if (!this.jsonMode) {
      console.log(`\n📦 Mục ${num}: ${title}`)
      console.log('-'.repeat(80))
    }
  }

  test(name, resultOrFn) {
    const t0 = performance.now()
    this.results.total++
    if (this.currentSection) this.currentSection.total++

    try {
      if (typeof resultOrFn === 'function') {
        resultOrFn()
      } else if (resultOrFn === false) {
        throw new Error('Assertion failed (returned false)')
      }

      const dt = performance.now() - t0
      this.results.passed++
      if (this.currentSection) {
        this.currentSection.passed++
        this.currentSection.tests.push({ name, passed: true, durationMs: dt })
      }
      if (!this.jsonMode) {
        console.log(`  ✅ ${name} (${dt.toFixed(1)}ms)`)
      }
    } catch (err) {
      const dt = performance.now() - t0
      this.results.failed++
      const failInfo = { name, passed: false, durationMs: dt, error: err?.message || String(err) }
      this.results.failures.push(failInfo)
      if (this.currentSection) {
        this.currentSection.failed++
        this.currentSection.tests.push(failInfo)
      }
      if (!this.jsonMode) {
        console.log(`  ❌ ${name} — THẤT BẠI (${dt.toFixed(1)}ms)`)
        console.log(`     Chi tiết: ${err?.message || String(err)}`)
      }
    }
  }

  skip(name, reason) {
    this.results.skipped++
    if (this.currentSection) {
      this.currentSection.skipped++
      this.currentSection.tests.push({ name, skipped: true, reason })
    }
    this.results.skips.push({ name, reason })
    if (!this.jsonMode) {
      console.log(`  ⚠️  [BỎ QUA] ${name} — ${reason}`)
    }
  }

  endSection() {
    if (this.currentSection) {
      this.results.sections.push(this.currentSection)
      if (!this.jsonMode) {
        const { passed, total, skipped } = this.currentSection
        const skipStr = skipped > 0 ? `, ${skipped} bỏ qua` : ''
        console.log(`  Tổng kết mục ${this.currentSection.num}: ${passed}/${total} đạt${skipStr}`)
      }
      this.currentSection = null
    }
  }

  endSuite() {
    this.results.durationMs = performance.now() - this.startTime

    if (this.jsonMode) {
      console.log(JSON.stringify(this.results, null, 2))
    } else {
      console.log('\n' + '='.repeat(80))
      console.log('📊 TỔNG KẾT THỰC THI E2E WEBSOCKET TEST SUITE')
      console.log('='.repeat(80))
      console.log(`Tổng số test assertions  : ${this.results.total}`)
      console.log(`Đạt (Passed)             : ${this.results.passed} ✅`)
      console.log(`Thất bại (Failed)        : ${this.results.failed} ${this.results.failed > 0 ? '❌' : ''}`)
      console.log(`Bỏ qua (Skipped)         : ${this.results.skipped} ⚠️`)
      console.log(`Thời gian thực thi       : ${(this.results.durationMs / 1000).toFixed(2)}s`)
      console.log('-'.repeat(80))

      this.results.sections.forEach((s) => {
        const skipNote = s.skipped > 0 ? ` (${s.skipped} skip)` : ''
        console.log(`Mục ${s.num} (${s.title.padEnd(45)}): ${s.passed}/${s.total} ĐẠT${skipNote}`)
      })
      console.log('='.repeat(80))

      if (this.results.failed === 0) {
        console.log('🎉 TẤT CẢ PHÉP KIỂM ĐẦU-CUỐI ĐỀU ĐẠT CHÍNH XÁC!')
      } else {
        console.log(`⚠️  CÓ ${this.results.failed} PHÉP KIỂM THẤT BẠI!`)
      }
    }

    return this.results.failed === 0
  }
}
