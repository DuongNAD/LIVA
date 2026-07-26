import { DatabaseSync } from 'node:sqlite'

const LOCAL_MEMORY_DOMAIN = 'memory_owner:local'

/**
 * Deterministic E2E assertion for conversation memory without reopening the
 * untrusted raw search command. The DB path must be the isolated file used by
 * the gateway under test.
 */
export const conversationMemoryContains = (
  dbPath,
  marker,
  ownerDomain = LOCAL_MEMORY_DOMAIN,
) => {
  const db = new DatabaseSync(dbPath, { readOnly: true })
  try {
    const row = db
      .prepare(`
        SELECT 1
        FROM vectors_meta AS memory
        INNER JOIN events AS event ON event.eventId = memory.vec_id
        WHERE memory.type = 'conversation_turn'
          AND memory.domain = ?
          AND event.domain = memory.domain
          AND event.category = memory.category
          AND event.consolidation_status IN ('pending', 'consolidated')
          AND instr(memory.content, ?) > 0
          AND EXISTS (
            SELECT 1
            FROM json_each(memory.source_event_ids)
            WHERE json_each.value = event.eventId
          )
        LIMIT 1
      `)
      .get(ownerDomain, marker)
    return row !== undefined
  } finally {
    db.close()
  }
}
