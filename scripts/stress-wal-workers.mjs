#!/usr/bin/env node
/**
 * LIVA Intelligent Assistant — Multi-Worker SQLite WAL Empirical Stress Test
 * Uses true multi-threading with Node.js worker_threads.
 */

import { DatabaseSync } from 'node:sqlite'
import fs from 'node:fs'
import path from 'node:path'
import os from 'node:os'
import { Worker, isMainThread, parentPort, workerData } from 'node:worker_threads'
import { fileURLToPath } from 'node:url'

const __filename = fileURLToPath(import.meta.url)

if (isMainThread) {
  async function run() {
    console.log('🚀 Starting Multi-Worker Thread SQLite WAL Empirical Stress Test...')
    const dbPath = path.join(os.tmpdir(), `liva_wal_workers_${Date.now()}.db`)

    // Initialize WAL DB
    const db = new DatabaseSync(dbPath)
    db.exec(`
      PRAGMA journal_mode = WAL;
      PRAGMA synchronous = NORMAL;
      PRAGMA busy_timeout = 10000;
      PRAGMA temp_store = MEMORY;
      PRAGMA foreign_keys = ON;

      CREATE TABLE IF NOT EXISTS worker_bench (
        id TEXT PRIMARY KEY,
        worker_id INTEGER NOT NULL,
        val INTEGER NOT NULL,
        created_at INTEGER NOT NULL
      );
      CREATE INDEX IF NOT EXISTS idx_bench_worker ON worker_bench(worker_id);
    `)
    db.close()

    const NUM_WORKERS = 8
    const OPS_PER_WORKER = 100
    console.log(`⚡ Spawning ${NUM_WORKERS} OS worker threads, ${OPS_PER_WORKER} operations each (${NUM_WORKERS * OPS_PER_WORKER} total transactions)...`)

    const startTime = performance.now()
    const workerPromises = []

    for (let w = 0; w < NUM_WORKERS; w++) {
      const p = new Promise((resolve, reject) => {
        const worker = new Worker(__filename, {
          workerData: { workerId: w, dbPath, opsCount: OPS_PER_WORKER }
        })
        worker.on('message', resolve)
        worker.on('error', reject)
        worker.on('exit', (code) => {
          if (code !== 0) reject(new Error(`Worker ${w} stopped with exit code ${code}`))
        })
      })
      workerPromises.push(p)
    }

    const results = await Promise.all(workerPromises)
    const totalDuration = performance.now() - startTime

    let totalWrites = 0
    let totalReads = 0
    let totalBusy = 0
    let totalErrors = 0

    results.forEach(r => {
      totalWrites += r.writes
      totalReads += r.reads
      totalBusy += r.busyErrors
      totalErrors += r.otherErrors
    })

    const verifyDb = new DatabaseSync(dbPath)
    const rowCount = verifyDb.prepare('SELECT count(*) as c FROM worker_bench').get().c
    verifyDb.close()

    try {
      fs.unlinkSync(dbPath)
      if (fs.existsSync(`${dbPath}-wal`)) fs.unlinkSync(`${dbPath}-wal`)
      if (fs.existsSync(`${dbPath}-shm`)) fs.unlinkSync(`${dbPath}-shm`)
    } catch {}

    console.log('\n📊 Multi-Worker Thread Empirical Results:')
    console.log(`- Total Duration   : ${totalDuration.toFixed(2)}ms`)
    console.log(`- Worker Count     : ${NUM_WORKERS} OS threads`)
    console.log(`- Total Writes     : ${totalWrites}/${NUM_WORKERS * OPS_PER_WORKER}`)
    console.log(`- Total Reads      : ${totalReads}`)
    console.log(`- Final Row Count  : ${rowCount}/${NUM_WORKERS * OPS_PER_WORKER}`)
    console.log(`- SQLITE_BUSY Errs : ${totalBusy} (Target: 0)`)
    console.log(`- Other Errors     : ${totalErrors}`)

    const success = (totalBusy === 0 && totalErrors === 0 && rowCount === NUM_WORKERS * OPS_PER_WORKER)
    console.log(`Verdict: ${success ? '✅ PASS' : '❌ FAIL'}`)
    process.exit(success ? 0 : 1)
  }

  run().catch(e => {
    console.error('Fatal error:', e)
    process.exit(1)
  })
} else {
  // Worker Thread Code
  const { workerId, dbPath, opsCount } = workerData
  const db = new DatabaseSync(dbPath)
  db.exec('PRAGMA busy_timeout = 10000;')
  db.exec('PRAGMA synchronous = NORMAL;')

  let writes = 0
  let reads = 0
  let busyErrors = 0
  let otherErrors = 0

  const insertStmt = db.prepare('INSERT INTO worker_bench (id, worker_id, val, created_at) VALUES (?, ?, ?, ?)')
  const readStmt = db.prepare('SELECT count(*) as c FROM worker_bench WHERE worker_id = ?')

  for (let i = 0; i < opsCount; i++) {
    // Interleaved read and write
    const recId = `worker_${workerId}_seq_${i}_${Math.random().toString(36).substring(2, 6)}`
    try {
      db.exec('BEGIN IMMEDIATE;')
      insertStmt.run(recId, workerId, i, Date.now())
      db.exec('COMMIT;')
      writes++
    } catch (e) {
      try { db.exec('ROLLBACK;') } catch {}
      if (e.message && (e.message.includes('busy') || e.message.includes('locked'))) {
        busyErrors++
      } else {
        otherErrors++
      }
    }

    try {
      const res = readStmt.get(workerId)
      reads++
    } catch (e) {
      if (e.message && (e.message.includes('busy') || e.message.includes('locked'))) {
        busyErrors++
      } else {
        otherErrors++
      }
    }
  }

  db.close()
  parentPort.postMessage({ workerId, writes, reads, busyErrors, otherErrors })
}
