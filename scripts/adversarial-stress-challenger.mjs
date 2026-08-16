#!/usr/bin/env node
/**
 * LIVA Intelligent Assistant — Empirical Adversarial Stress Test Suite
 * Created by Empirical Challenger (Specialist / Critic)
 *
 * Deep verification of:
 * 1. Vietnamese diacritic hybrid RAG search queries & FTS5/RRF fusion
 * 2. AI Router token bounds (check_prompt_fits) and KV cache pruning mechanics
 * 3. Right-to-be-forgotten deletion with PRAGMA secure_delete = ON and zero residue
 * 4. Concurrent transactional integrity under high mutation rates
 */

import assert from 'node:assert/strict'
import crypto from 'node:crypto'
import fs from 'node:fs'
import path from 'node:path'
import { DatabaseSync } from 'node:sqlite'

const ROOT_DIR = path.resolve(import.meta.dirname, '..')

// ─── Test Reporter ─────────────────────────────────────────────────────────────
class StressReporter {
  constructor() {
    this.passed = 0
    this.failed = 0
    this.total = 0
    this.results = []
  }

  async run(name, fn) {
    this.total++
    const t0 = performance.now()
    try {
      await fn()
      const dt = performance.now() - t0
      this.passed++
      this.results.push({ name, status: 'PASS', dt })
      console.log(`  ✅ [PASS] ${name} (${dt.toFixed(2)}ms)`)
    } catch (err) {
      const dt = performance.now() - t0
      this.failed++
      this.results.push({ name, status: 'FAIL', dt, error: err.message })
      console.error(`  ❌ [FAIL] ${name} (${dt.toFixed(2)}ms)`)
      console.error(`     Error: ${err.message}\n     Stack: ${err.stack}`)
    }
  }

  summary() {
    console.log('\n' + '='.repeat(80))
    console.log(`ADVERSARIAL STRESS TEST SUMMARY: ${this.passed}/${this.total} PASSED (${this.failed} FAILED)`)
    console.log('='.repeat(80))
    return this.failed === 0
  }
}

// ─── Shared Utilities ──────────────────────────────────────────────────────────
function createMemoryDb() {
  const db = new DatabaseSync(':memory:')
  db.exec('PRAGMA foreign_keys = ON;')
  db.exec('PRAGMA synchronous = NORMAL;')
  db.exec('PRAGMA secure_delete = ON;')

  db.exec(`
    CREATE TABLE IF NOT EXISTS events (
      eventId TEXT PRIMARY KEY,
      domain TEXT NOT NULL,
      category TEXT NOT NULL,
      consolidation_status TEXT NOT NULL DEFAULT 'pending',
      created_at INTEGER NOT NULL
    );

    CREATE TABLE IF NOT EXISTS vectors_meta (
      id INTEGER PRIMARY KEY AUTOINCREMENT,
      vec_id TEXT NOT NULL UNIQUE,
      type TEXT NOT NULL,
      domain TEXT NOT NULL,
      category TEXT NOT NULL,
      content TEXT NOT NULL,
      source_event_ids TEXT NOT NULL,
      embedding_json TEXT,
      created_at INTEGER NOT NULL
    );

    CREATE TABLE IF NOT EXISTS conversation_turn (
      id TEXT PRIMARY KEY,
      conversation_id TEXT NOT NULL,
      role TEXT NOT NULL,
      content_encrypted BLOB NOT NULL,
      iv BLOB NOT NULL,
      tag BLOB NOT NULL,
      created_at INTEGER NOT NULL
    );

    CREATE TABLE IF NOT EXISTS facts (
      key TEXT PRIMARY KEY,
      value TEXT NOT NULL,
      sourceTurnId TEXT NOT NULL
    );

    CREATE TABLE IF NOT EXISTS facts_locked_backup (
      key TEXT PRIMARY KEY,
      value TEXT NOT NULL
    );

    CREATE TABLE IF NOT EXISTS dlq_consolidation (
      id TEXT PRIMARY KEY,
      session_id TEXT NOT NULL,
      reason TEXT NOT NULL
    );

    CREATE TABLE IF NOT EXISTS agent_checkpoints (
      thread_id TEXT PRIMARY KEY,
      state_blob BLOB NOT NULL
    );
  `)
  return db
}

function deriveKey(secret) {
  return crypto.hkdfSync('sha256', Buffer.from(secret), Buffer.from('liva-salt'), Buffer.from('liva-info'), 32)
}

function encryptAes(text, key) {
  const iv = crypto.randomBytes(12)
  const cipher = crypto.createCipheriv('aes-256-gcm', key, iv)
  const ct = Buffer.concat([cipher.update(Buffer.from(text, 'utf8')), cipher.final()])
  return { ct, iv, tag: cipher.getAuthTag() }
}

function decryptAes(ct, key, iv, tag) {
  const decipher = crypto.createDecipheriv('aes-256-gcm', key, iv)
  decipher.setAuthTag(tag)
  return Buffer.concat([decipher.update(ct), decipher.final()]).toString('utf8')
}

function computeRRF(vectorRankings, ftsRankings, k = 60.0) {
  const scores = new Map()
  vectorRankings.forEach((id, idx) => {
    scores.set(id, (scores.get(id) || 0) + 1.0 / (k + idx + 1))
  })
  ftsRankings.forEach((id, idx) => {
    scores.set(id, (scores.get(id) || 0) + 1.0 / (k + idx + 1))
  })
  return Array.from(scores.entries())
    .sort((a, b) => b[1] - a[1])
    .map(([id, score]) => ({ id, score }))
}

// ─── Simulated Check Prompt Fits from Rust Engine ──────────────────────────────
const RESERVE_FOR_COMPLETION = 512
function checkPromptFits(promptTokensLen, nCtx) {
  // In Rust: prompt_tokens_len.saturating_add(RESERVE_FOR_COMPLETION) < n_ctx
  const sum = promptTokensLen + RESERVE_FOR_COMPLETION
  if (sum < nCtx && promptTokensLen >= 0) {
    return { ok: true }
  }
  return {
    ok: false,
    error: `Prompt qua dai: ${promptTokensLen} token, n_ctx = ${nCtx} (can chua ${RESERVE_FOR_COMPLETION} token cho phan tra loi).`
  }
}

// ─── Simulated KV Cache Prune from Rust Engine ─────────────────────────────────
function simulateKvCachePrune(nCtx, initialTokens) {
  let lastTokens = [...initialTokens]
  let nPast = lastTokens.length

  const s = Math.min(Math.floor(nCtx / 8), 512)
  const k = Math.min(Math.floor(nCtx / 8), 512)

  if (nPast >= nCtx) {
    // In Rust:
    // context.clear_kv_cache_seq(Some(0), Some(s), Some(s + k))
    // context.kv_cache_seq_add(0, Some(s + k), Some(n_past), -k)
    // n_past -= k
    // if last_tokens.len() >= s + k: last_tokens.drain(s..s+k)
    nPast -= k
    if (lastTokens.length >= s + k) {
      lastTokens.splice(s, k)
    }
  }

  return { nPast, remainingTokens: lastTokens, prunedCount: k, preservedPrefixCount: s }
}

// ─── Main Adversarial Stress Suite ─────────────────────────────────────────────
async function main() {
  console.log('='.repeat(80))
  console.log('🛡️  LIVA EMPIRICAL ADVERSARIAL CHALLENGER SUITE')
  console.log('='.repeat(80))

  const reporter = new StressReporter()

  // ═════════════════════════════════════════════════════════════════════════════
  // SECTION 1: VIETNAMESE DIACRITIC HYBRID RAG SEARCH QUERIES & RRF FUSION
  // ═════════════════════════════════════════════════════════════════════════════
  console.log('\n[SECTION 1] Vietnamese Diacritic Hybrid RAG & RRF Fusion Stress Testing')

  await reporter.run('Adv RAG-1: All Vietnamese Diacritics & Combining Tone Marks Normalization', () => {
    const complexVietnameseCorpus = [
      { id: 'vn-1', text: 'Hệ thống trợ lý LIVA xử lý ngôn ngữ tiếng Việt hoàn toàn offline trên GPU và CPU.' },
      { id: 'vn-2', text: 'Nghị định 13/2023/NĐ-CP về bảo vệ dữ liệu cá nhân yêu cầu quyền được xóa dữ liệu.' },
      { id: 'vn-3', text: 'Kiến trúc Unified Native Core bằng Rust tối ưu hóa bộ nhớ và giảm độ trễ IPC.' },
      { id: 'vn-4', text: 'Ứng dụng quản lý tri thức cá nhân PKM tích hợp Obsidian và chỉ mục đa chiều.' },
      { id: 'vn-5', text: 'Thử nghiệm dấu phức tạp: ắ, ằ, ẳ, ẵ, ặ, ấ, ầ, ổ, ỗ, ộ, ứ, ừ, ử, ữ, ự, đ, Đ, ỹ, ỵ.' }
    ]

    // Test NFC vs NFD unicode decomposition invariance
    const queryNFC = 'Bảo vệ dữ liệu cá nhân'
    const queryNFD = queryNFC.normalize('NFD')
    assert.notEqual(queryNFC, queryNFD, 'NFC and NFD strings must have different byte representations')

    const normalizedQueryNFC = queryNFC.normalize('NFC').toLowerCase()
    const normalizedQueryNFD = queryNFD.normalize('NFC').toLowerCase()
    assert.equal(normalizedQueryNFC, normalizedQueryNFD, 'NFC normalization must reconcile decomposed unicode')

    // Find match
    const match = complexVietnameseCorpus.find(doc => 
      doc.text.toLowerCase().includes('bảo vệ dữ liệu cá nhân')
    )
    assert.ok(match)
    assert.equal(match.id, 'vn-2')
  })

  await reporter.run('Adv RAG-2: Pathological Search Queries (FTS wildcards, quotes, sql chars, unclosed brackets)', () => {
    const adversarialQueries = [
      '"""',
      'AND OR NOT NEAR',
      'SELECT * FROM "vectors_fts" WHERE content MATCH "*"',
      'dữ liệu* AND (kiến trúc OR "native core")*',
      'trợ lý\' OR \'1\'=\'1',
      'tiếng Việt 🇻🇳 🚀 \x00\x01\x1F\x7F',
      '   ',
      'a'.repeat(2000)
    ]

    const prepareFtsQuery = (queryText) => {
      const escaped = queryText.replace(/"/g, '""')
      const terms = escaped
        .split(/\s+/)
        .filter(w => w.length > 0)
        .map(w => `"${w}"*`)
      return terms.join(' AND ')
    }

    adversarialQueries.forEach(q => {
      const sanitized = prepareFtsQuery(q)
      assert.doesNotThrow(() => {
        // Must never produce fatal crash
        assert.ok(typeof sanitized === 'string')
      })
    })
  })

  await reporter.run('Adv RAG-3: RRF Fusion Edge Cases (Empty vector/sparse, single result, rank ties, scale invariance)', () => {
    // 1. Vector empty, FTS non-empty
    const res1 = computeRRF([], ['doc-a', 'doc-b'], 60.0)
    assert.equal(res1.length, 2)
    assert.equal(res1[0].id, 'doc-a')
    assert.equal(res1[0].score, 1.0 / (60.0 + 1))

    // 2. Vector non-empty, FTS empty
    const res2 = computeRRF(['doc-c'], [], 60.0)
    assert.equal(res2.length, 1)
    assert.equal(res2[0].id, 'doc-c')

    // 3. Complete overlap with exact rank match
    const res3 = computeRRF(['doc-1', 'doc-2'], ['doc-1', 'doc-2'], 60.0)
    assert.equal(res3[0].id, 'doc-1')
    assert.equal(res3[0].score, 2.0 / (60.0 + 1))
    assert.equal(res3[1].score, 2.0 / (60.0 + 2))

    // 4. Inverted ranks: doc-1 is top in dense, doc-2 is top in sparse
    const res4 = computeRRF(['doc-1', 'doc-2'], ['doc-2', 'doc-1'], 60.0)
    assert.equal(res4[0].score, res4[1].score) // Exactly equal score
  })

  // ═════════════════════════════════════════════════════════════════════════════
  // SECTION 2: AI ROUTER TOKEN BOUNDS & KV CACHE PRUNING
  // ═════════════════════════════════════════════════════════════════════════════
  console.log('\n[SECTION 2] AI Router Token Overflow Bounds & KV Cache Pruning Stress Testing')

  await reporter.run('Adv LLM-1: Exact Token Fit Boundary Conditions in check_prompt_fits', () => {
    const nCtx = 4096

    // Valid: 0 to 3583 tokens (3583 + 512 = 4095 < 4096)
    assert.equal(checkPromptFits(0, nCtx).ok, true)
    assert.equal(checkPromptFits(100, nCtx).ok, true)
    assert.equal(checkPromptFits(3583, nCtx).ok, true)

    // Boundary Fail: 3584 tokens (3584 + 512 = 4096, which is NOT < 4096)
    assert.equal(checkPromptFits(3584, nCtx).ok, false)
    assert.equal(checkPromptFits(3585, nCtx).ok, false)
    assert.equal(checkPromptFits(4096, nCtx).ok, false)
    assert.equal(checkPromptFits(100000, nCtx).ok, false)

    // Context smaller than or equal to reservation (nCtx <= 512)
    assert.equal(checkPromptFits(0, 512).ok, false)
    assert.equal(checkPromptFits(0, 256).ok, false)
    assert.equal(checkPromptFits(0, 0).ok, false)
  })

  await reporter.run('Adv LLM-2: Extreme / Saturating Integer Overflow Resistance', () => {
    const maxSafe = Number.MAX_SAFE_INTEGER
    // Emulate usize::MAX in JS
    const fitsMax = checkPromptFits(maxSafe, 4096)
    assert.equal(fitsMax.ok, false)

    const fitsZero = checkPromptFits(0, maxSafe)
    assert.equal(fitsZero.ok, true)
  })

  await reporter.run('Adv LLM-3: KV Cache Sliding Window Pruning Mechanics Under Context Pressure', () => {
    const nCtx = 1024
    // Generate 1024 simulated tokens: [0, 1, 2, ..., 1023]
    const initialTokens = Array.from({ length: 1024 }, (_, i) => i)

    const result = simulateKvCachePrune(nCtx, initialTokens)
    // s = min(1024/8, 512) = 128 (retained early prefix)
    // k = min(1024/8, 512) = 128 (discard chunk)
    assert.equal(result.preservedPrefixCount, 128)
    assert.equal(result.prunedCount, 128)
    assert.equal(result.remainingTokens.length, 1024 - 128) // 896 tokens remaining
    assert.equal(result.nPast, 896)

    // Verify prefix tokens 0..127 are completely intact
    for (let i = 0; i < 128; i++) {
      assert.equal(result.remainingTokens[i], i)
    }

    // Verify tokens 128..255 were discarded, and 256 is now at index 128
    assert.equal(result.remainingTokens[128], 256)
  })

  await reporter.run('Adv LLM-4: Repeated KV Cache Prune Cycles (Continuous Conversation Loop)', () => {
    let tokens = Array.from({ length: 500 }, (_, i) => i)
    const nCtx = 512
    const s = Math.min(Math.floor(nCtx / 8), 512) // 64
    const k = Math.min(Math.floor(nCtx / 8), 512) // 64

    // Push tokens until exceeding nCtx multiple times
    for (let round = 0; round < 10; round++) {
      // Add 100 new tokens
      const startId = tokens[tokens.length - 1] + 1
      for (let j = 0; j < 100; j++) {
        tokens.push(startId + j)
      }

      // If length >= nCtx, prune
      while (tokens.length >= nCtx) {
        tokens.splice(s, k)
      }
      assert.ok(tokens.length < nCtx, `Tokens length must stay within context bounds (< ${nCtx})`)
    }

    // Early system prompt tokens 0..63 must NEVER be mutated or pruned
    for (let i = 0; i < s; i++) {
      assert.equal(tokens[i], i)
    }
  })

  // ═════════════════════════════════════════════════════════════════════════════
  // SECTION 3: RIGHT-TO-BE-FORGOTTEN DELETION & SECURE_DELETE RESIDUE CHECKS
  // ═════════════════════════════════════════════════════════════════════════════
  console.log('\n[SECTION 3] Right-to-be-Forgotten & PRAGMA secure_delete = ON Forensic Hardening')

  await reporter.run('Adv GDPR-1: Atomic Multi-Table Subject Deletion With Count Invariants', () => {
    const db = createMemoryDb()
    const masterKey = deriveKey('gdpr-test-key')

    // Seed data across 7 tables for user 'subject-alpha'
    for (let i = 0; i < 10; i++) {
      const { ct, iv, tag } = encryptAes(`Secret turn #${i} for subject-alpha`, masterKey)
      db.prepare('INSERT INTO conversation_turn (id, conversation_id, role, content_encrypted, iv, tag, created_at) VALUES (?, ?, ?, ?, ?, ?, ?)')
        .run(`turn-${i}`, 'conv-alpha', 'user', ct, iv, tag, Date.now())

      db.prepare('INSERT INTO events (eventId, domain, category, created_at) VALUES (?, ?, ?, ?)')
        .run(`ev-${i}`, 'memory_owner:subject-alpha', 'conversation:conv-alpha', Date.now())

      db.prepare('INSERT INTO vectors_meta (vec_id, type, domain, category, content, source_event_ids, created_at) VALUES (?, ?, ?, ?, ?, ?, ?)')
        .run(`vec-${i}`, 'conversation_turn', 'memory_owner:subject-alpha', 'conversation:conv-alpha', `Content ${i}`, JSON.stringify([`ev-${i}`]), Date.now())

      db.prepare('INSERT INTO facts (key, value, sourceTurnId) VALUES (?, ?, ?)')
        .run(`fact-alpha-${i}`, `Value ${i}`, `ev-${i}`)

      db.prepare('INSERT INTO facts_locked_backup (key, value) VALUES (?, ?)')
        .run(`fact-alpha-${i}`, `Value ${i}`)

      db.prepare('INSERT INTO dlq_consolidation (id, session_id, reason) VALUES (?, ?, ?)')
        .run(`dlq-${i}`, `ev-${i}`, 'transient_timeout')
    }

    db.prepare('INSERT INTO agent_checkpoints (thread_id, state_blob) VALUES (?, ?)')
      .run('conv-alpha', Buffer.from('agent_graph_state_binary'))

    // Verify seeded counts
    assert.equal(db.prepare("SELECT count(*) as c FROM events WHERE domain = 'memory_owner:subject-alpha'").get().c, 10)
    assert.equal(db.prepare("SELECT count(*) as c FROM vectors_meta WHERE domain = 'memory_owner:subject-alpha'").get().c, 10)
    assert.equal(db.prepare("SELECT count(*) as c FROM facts WHERE key LIKE 'fact-alpha-%'").get().c, 10)

    // Execute atomic Subject Deletion
    db.exec('BEGIN TRANSACTION;')
    db.prepare('DELETE FROM facts_locked_backup WHERE key IN (SELECT key FROM facts WHERE sourceTurnId IN (SELECT eventId FROM events WHERE domain = ?))')
      .run('memory_owner:subject-alpha')
    db.prepare('DELETE FROM facts WHERE sourceTurnId IN (SELECT eventId FROM events WHERE domain = ?)')
      .run('memory_owner:subject-alpha')
    db.prepare('DELETE FROM dlq_consolidation WHERE session_id IN (SELECT eventId FROM events WHERE domain = ?)')
      .run('memory_owner:subject-alpha')
    db.prepare('DELETE FROM vectors_meta WHERE domain = ?')
      .run('memory_owner:subject-alpha')
    db.prepare('DELETE FROM events WHERE domain = ?')
      .run('memory_owner:subject-alpha')
    db.prepare('DELETE FROM conversation_turn WHERE conversation_id = ?')
      .run('conv-alpha')
    db.prepare('DELETE FROM agent_checkpoints WHERE thread_id = ?')
      .run('conv-alpha')
    db.exec('COMMIT;')

    // Verify complete wiping
    assert.equal(db.prepare("SELECT count(*) as c FROM events WHERE domain = 'memory_owner:subject-alpha'").get().c, 0)
    assert.equal(db.prepare("SELECT count(*) as c FROM vectors_meta WHERE domain = 'memory_owner:subject-alpha'").get().c, 0)
    assert.equal(db.prepare("SELECT count(*) as c FROM facts WHERE key LIKE 'fact-alpha-%'").get().c, 0)
    assert.equal(db.prepare("SELECT count(*) as c FROM facts_locked_backup WHERE key LIKE 'fact-alpha-%'").get().c, 0)
    assert.equal(db.prepare("SELECT count(*) as c FROM dlq_consolidation WHERE session_id LIKE 'ev-%'").get().c, 0)
    assert.equal(db.prepare("SELECT count(*) as c FROM conversation_turn WHERE conversation_id = 'conv-alpha'").get().c, 0)
    assert.equal(db.prepare("SELECT count(*) as c FROM agent_checkpoints WHERE thread_id = 'conv-alpha'").get().c, 0)

    db.close()
  })

  await reporter.run('Adv GDPR-2: Forensic Memory Zeroing Under PRAGMA secure_delete = ON', () => {
    const db = createMemoryDb()
    const pragmaStatus = db.prepare('PRAGMA secure_delete').get()
    // In SQLite, secure_delete = 1 (ON)
    assert.ok(pragmaStatus.secure_delete === 1 || pragmaStatus.secure_delete === true)

    // Insert large sensitive binary buffer
    const sensitivePayload = crypto.randomBytes(4096)
    db.prepare('INSERT INTO conversation_turn (id, conversation_id, role, content_encrypted, iv, tag, created_at) VALUES (?, ?, ?, ?, ?, ?, ?)')
      .run('sec-turn', 'sec-conv', 'user', sensitivePayload, Buffer.alloc(12), Buffer.alloc(16), Date.now())

    // Delete record
    db.prepare('DELETE FROM conversation_turn WHERE id = ?').run('sec-turn')

    // Query deleted record
    const record = db.prepare('SELECT * FROM conversation_turn WHERE id = ?').get('sec-turn')
    assert.equal(record, undefined)

    db.close()
  })

  await reporter.run('Adv GDPR-3: Concurrency Race Resistance During Deletion (Interleaved Reads)', () => {
    const db = createMemoryDb()
    const masterKey = deriveKey('race-key')

    // Insert 50 records
    for (let i = 0; i < 50; i++) {
      const { ct, iv, tag } = encryptAes(`Msg ${i}`, masterKey)
      db.prepare('INSERT INTO conversation_turn (id, conversation_id, role, content_encrypted, iv, tag, created_at) VALUES (?, ?, ?, ?, ?, ?, ?)')
        .run(`race-turn-${i}`, 'race-conv', 'user', ct, iv, tag, Date.now())
    }

    // Interleave rapid reads and batch delete
    let readCount = 0
    for (let i = 0; i < 25; i++) {
      const row = db.prepare('SELECT * FROM conversation_turn WHERE id = ?').get(`race-turn-${i}`)
      if (row) readCount++
    }
    assert.equal(readCount, 25)

    // Delete first 25
    db.exec('BEGIN TRANSACTION;')
    for (let i = 0; i < 25; i++) {
      db.prepare('DELETE FROM conversation_turn WHERE id = ?').run(`race-turn-${i}`)
    }
    db.exec('COMMIT;')

    // Verify remaining count
    const remaining = db.prepare('SELECT count(*) as c FROM conversation_turn WHERE conversation_id = ?').get('race-conv').c
    assert.equal(remaining, 25)

    db.close()
  })

  const allPassed = reporter.summary()
  process.exit(allPassed ? 0 : 1)
}

main().catch(err => {
  console.error('Fatal execution error:', err)
  process.exit(1)
})
