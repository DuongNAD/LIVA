import assert from 'node:assert/strict'
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
