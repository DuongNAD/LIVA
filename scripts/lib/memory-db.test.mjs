import assert from 'node:assert/strict'
import { createCipheriv, hkdfSync, randomBytes } from 'node:crypto'
import { mkdtempSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import test from 'node:test'
import { DatabaseSync } from 'node:sqlite'

import { conversationMemoryContains } from './memory-db.mjs'

test('conversation memory verifier chỉ đọc đúng owner và audience scope', () => {
  const dir = mkdtempSync(join(tmpdir(), 'liva-memory-db-'))
  const dbPath = join(dir, 'memory.sqlite')
  const db = new DatabaseSync(dbPath)
  db.exec(`
    CREATE TABLE vectors_meta (
      vec_id TEXT PRIMARY KEY,
      type TEXT NOT NULL,
      content TEXT NOT NULL,
      domain TEXT NOT NULL,
      category TEXT NOT NULL,
      source_event_ids TEXT NOT NULL DEFAULT '[]'
    );
    CREATE TABLE events (
      eventId TEXT PRIMARY KEY,
      consolidation_status TEXT NOT NULL,
      domain TEXT NOT NULL,
      category TEXT NOT NULL
    );
  `)
  const insert = db.prepare(
    `INSERT INTO vectors_meta (
      vec_id, type, content, domain, category, source_event_ids
    ) VALUES (?, ?, ?, ?, ?, ?)`,
  )
  insert.run(
    'local-turn',
    'conversation_turn',
    'mã dự án ORION-7',
    'memory_owner:local',
    'conversation:default',
    '["local-turn"]',
  )
  insert.run(
    'other-turn',
    'conversation_turn',
    'bí mật owner khác',
    'memory_owner:telegram:100',
    'conversation:telegram_chat:100',
    '["other-turn"]',
  )
  insert.run(
    'scope-mismatch',
    'conversation_turn',
    'bí mật sai audience',
    'memory_owner:local',
    'conversation:default',
    '["scope-mismatch"]',
  )
  insert.run(
    'consolidated-turn',
    'conversation_turn',
    'ký ức đã finalize',
    'memory_owner:local',
    'conversation:default',
    '["consolidated-turn"]',
  )
  const insertEvent = db.prepare(
    `INSERT INTO events (
      eventId, consolidation_status, domain, category
    ) VALUES (?, ?, ?, ?)`,
  )
  insertEvent.run(
    'local-turn',
    'pending',
    'memory_owner:local',
    'conversation:default',
  )
  insertEvent.run(
    'other-turn',
    'pending',
    'memory_owner:telegram:100',
    'conversation:telegram_chat:100',
  )
  insertEvent.run(
    'scope-mismatch',
    'pending',
    'memory_owner:local',
    'conversation:other',
  )
  insertEvent.run(
    'consolidated-turn',
    'consolidated',
    'memory_owner:local',
    'conversation:default',
  )
  db.close()

  try {
    assert.equal(conversationMemoryContains(dbPath, 'ORION-7'), true)
    assert.equal(conversationMemoryContains(dbPath, 'bí mật owner khác'), false)
    assert.equal(conversationMemoryContains(dbPath, 'bí mật sai audience'), false)
    assert.equal(conversationMemoryContains(dbPath, 'ký ức đã finalize'), true)
    assert.equal(conversationMemoryContains(dbPath, 'không tồn tại'), false)
  } finally {
    rmSync(dir, { recursive: true, force: true })
  }
})

test('conversation memory verifier từ chối vector không có event ledger', () => {
  const dir = mkdtempSync(join(tmpdir(), 'liva-memory-orphan-'))
  const dbPath = join(dir, 'memory.sqlite')
  const db = new DatabaseSync(dbPath)
  db.exec(`
    CREATE TABLE vectors_meta (
      vec_id TEXT PRIMARY KEY,
      type TEXT NOT NULL,
      content TEXT NOT NULL,
      domain TEXT NOT NULL,
      category TEXT NOT NULL,
      source_event_ids TEXT NOT NULL DEFAULT '[]'
    );
    CREATE TABLE events (
      eventId TEXT PRIMARY KEY,
      consolidation_status TEXT NOT NULL,
      domain TEXT NOT NULL,
      category TEXT NOT NULL
    );
    INSERT INTO vectors_meta (
      vec_id, type, content, domain, category, source_event_ids
    ) VALUES (
      'orphan-turn',
      'conversation_turn',
      'mã dự án ORION-7',
      'memory_owner:local',
      'conversation:default',
      '["orphan-turn"]'
    );
  `)
  db.close()

  try {
    assert.equal(conversationMemoryContains(dbPath, 'ORION-7'), false)
  } finally {
    rmSync(dir, { recursive: true, force: true })
  }
})

// Vì sao test này tồn tại — trả giá 16/08/2026. `vectors_meta.content` được mã
// hoá thành bao `v2:` từ 22/07/2026, nhưng phép kiểm vẫn làm
// `instr(content, marker)` thẳng trong SQL, tức tìm plaintext trong cột
// ciphertext ⇒ KHÔNG BAO GIỜ khớp. `e2e-memory.mjs` vì thế tụt 6/6 → 4/6 và đọc
// y hệt "bộ nhớ vỡ", trong khi bộ nhớ chạy đúng và kiểm mềm vẫn đạt. Bộ test cũ
// không bắt được vì mọi fixture của nó đều chèn plaintext — đúng vùng mù. Test
// này chèn ciphertext thật, nên lần sau đổi định dạng mã hoá là nó đỏ ngay.
test('conversation memory verifier đọc được nội dung đã mã hoá (bao v2)', () => {
  const passphrase = 'khoa-test-khong-bi-mat-32-byte!!'
  const dir = mkdtempSync(join(tmpdir(), 'liva-memory-db-v2-'))
  const dbPath = join(dir, 'memory.sqlite')

  // Dựng bao v2 đúng như Rust: salt/iv/tag 16 byte, khoá = HKDF-SHA256.
  const salt = randomBytes(16)
  const iv = randomBytes(16)
  const key = Buffer.from(
    hkdfSync('sha256', Buffer.from(passphrase, 'utf8'), salt, Buffer.from('liva-facts-encryption-v2', 'utf8'), 32),
  )
  const cipher = createCipheriv('aes-256-gcm', key, iv, { authTagLength: 16 })
  const ct = Buffer.concat([cipher.update('mã dự án ORION-7', 'utf8'), cipher.final()])
  const bao = `v2:${salt.toString('hex')}:${iv.toString('hex')}:${cipher.getAuthTag().toString('hex')}:${ct.toString('hex')}`
  assert.ok(bao.startsWith('v2:'), 'fixture phải là bao v2, nếu không test này vô nghĩa')

  const db = new DatabaseSync(dbPath)
  db.exec(`
    CREATE TABLE vectors_meta (
      vec_id TEXT PRIMARY KEY,
      type TEXT NOT NULL,
      content TEXT NOT NULL,
      domain TEXT NOT NULL,
      category TEXT NOT NULL,
      source_event_ids TEXT NOT NULL DEFAULT '[]'
    );
    CREATE TABLE events (
      eventId TEXT PRIMARY KEY,
      consolidation_status TEXT NOT NULL,
      domain TEXT NOT NULL,
      category TEXT NOT NULL
    );
    INSERT INTO events (eventId, consolidation_status, domain, category)
    VALUES ('enc-turn', 'consolidated', 'memory_owner:local', 'conversation:default');
  `)
  db.prepare(
    `INSERT INTO vectors_meta (vec_id, type, content, domain, category, source_event_ids)
     VALUES (?, ?, ?, ?, ?, ?)`,
  ).run('enc-turn', 'conversation_turn', bao, 'memory_owner:local', 'conversation:default', '["enc-turn"]')
  db.close()

  try {
    // Đúng khoá → thấy nội dung bên trong ciphertext.
    assert.equal(conversationMemoryContains(dbPath, 'ORION-7', 'memory_owner:local', passphrase), true)
    // Chuỗi không có trong bản rõ → vẫn false; không phải "giải mã được là true".
    assert.equal(conversationMemoryContains(dbPath, 'KHONG-CO', 'memory_owner:local', passphrase), false)
    // Sai khoá → NÉM, không lặng lẽ trả false. Fail-closed: nuốt lỗi ở đây đúng
    // là cách bộ kiểm mù thêm một lần nữa.
    assert.throws(
      () => conversationMemoryContains(dbPath, 'ORION-7', 'memory_owner:local', 'khoa-sai-hoan-toan-khac-32byte!!'),
      'sai khoá phải ném chứ không được trả false',
    )
  } finally {
    rmSync(dir, { recursive: true, force: true })
  }
})
