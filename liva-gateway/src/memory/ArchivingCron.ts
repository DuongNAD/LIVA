import { StructuredMemory } from "./StructuredMemory";
import { logger } from "../utils/logger";
import { promises as fsp } from "node:fs";
import * as path from "node:path";
import OpenAI from "openai";
import { TaskQueue, TaskPriority } from "../core/TaskQueue";

/**
 * ArchivingCron - Sleep-dependent Memory Consolidation & Cold Storage
 * ====================================================================
 * Mô phỏng cơ chế dọn dẹp ký ức lúc ngủ của não bộ (Active Forgetting).
 * Định kỳ quét các Vector (L2) cũ và ít truy cập, tóm tắt thành các 
 * Concept cốt lõi lưu ở L3 Graph, sau đó dump toàn bộ dữ liệu chi tiết 
 * ra ổ cứng (.jsonl) và xóa khỏi SQLite để giải phóng RAM/Disk.
 */
export class ArchivingCron {
    private readonly structuredMemory: StructuredMemory;
    private readonly aiClient: OpenAI;
    #cronTimer: NodeJS.Timeout | null = null;
    private isRunning = false;

    // Cấu hình
    private readonly ARCHIVE_INTERVAL_MS = 24 * 60 * 60 * 1000; // Chạy mỗi 24 giờ
    private readonly OLD_AGE_DAYS = 30; // Vector cũ hơn 30 ngày
    private readonly MAX_ACCESS_COUNT = 2; // Ít được truy cập
    private readonly DECAY_THRESHOLD = 0.5; // Hoặc bị phai mờ (decay_weight < 0.5)

    constructor(structuredMemory: StructuredMemory, aiClient: OpenAI) {
        this.structuredMemory = structuredMemory;
        this.aiClient = aiClient;
    }

    public start(): void {
        if (this.#cronTimer) return;
        
        this.#cronTimer = setInterval(() => {
            TaskQueue.wrapMemoryTask(
                () => this.runArchivingProcess(),
                `ArchivingCron-${Date.now()}`,
                TaskPriority.LOW
            ).catch(e => logger.warn(`[ArchivingCron] Failed: ${e.message}`));
        }, this.ARCHIVE_INTERVAL_MS);
        this.#cronTimer.unref();

        logger.info("[ArchivingCron] 🧊 Cold Storage Archiving loop started (checks every 24h).");
    }

    public stop(): void {
        if (this.#cronTimer) {
            clearInterval(this.#cronTimer);
            this.#cronTimer = null;
        }
    }

    public dispose(): void {
        this.stop();
        logger.info("[ArchivingCron] Disposed.");
    }

    /**
     * Trigger manual archiving
     */
    public async runArchivingProcess(): Promise<number> {
        if (this.isRunning) return 0;
        this.isRunning = true;
        let archivedCount = 0;

        try {
            const db = this.structuredMemory.getDb();
            const now = Date.now();
            const ageMs = this.OLD_AGE_DAYS * 24 * 60 * 60 * 1000;
            const thresholdTime = now - ageMs;

            // 1. Quét các Vector cũ & ít truy cập
            const oldVectors = db.prepare(`
                SELECT id, vec_id, content, type, domain, category, source_event_ids
                FROM vectors_meta
                WHERE created_at < ? 
                  AND (access_count <= ? OR decay_weight < ?)
            `).all(thresholdTime, this.MAX_ACCESS_COUNT, this.DECAY_THRESHOLD) as Array<{
                id: number; vec_id: string; content: string; type: string; domain: string; category: string; source_event_ids: string;
            }>;

            if (oldVectors.length === 0) {
                logger.debug("[ArchivingCron] No stale vectors found for archiving.");
                return 0;
            }

            logger.info(`[ArchivingCron] Found ${oldVectors.length} stale vectors. Starting consolidation...`);

            // Chuẩn bị thư mục Cold Storage
            const archiveDir = path.join(process.cwd(), "data", "cold_storage");
            await fsp.mkdir(archiveDir, { recursive: true });
            
            const date = new Date();
            const archiveFileName = `archive_${date.getFullYear()}_${(date.getMonth() + 1).toString().padStart(2, '0')}.jsonl`;
            const archiveFilePath = path.join(archiveDir, archiveFileName);

            const fileHandle = await fsp.open(archiveFilePath, 'a');

            // 2. Gom nhóm theo Domain để LLM tóm tắt tốt hơn
            const domainGroups = new Map<string, typeof oldVectors>();
            for (const v of oldVectors) {
                const arr = domainGroups.get(v.domain) || [];
                arr.push(v);
                domainGroups.set(v.domain, arr);
            }

            for (const [domain, vectors] of domainGroups) {
                // Tóm tắt Macro-Synthesis bằng LLM
                const contents = vectors.map(v => v.content).join("\n- ");
                const prompt = `Summarize the following old memories from domain '${domain}' into a highly condensed single core concept (1-2 sentences). Extract any long-term factual entity or relationship that should not be forgotten.
Memories:
- ${contents}`;

                try {
                    const response = await this.aiClient.chat.completions.create({
                        model: "router",
                        messages: [{ role: "user", content: prompt }],
                        temperature: 0.1,
                        max_tokens: 300,
                    });

                    const summary = response.choices[0]?.message?.content?.trim();
                    if (summary) {
                        // 3. Tạo L3 Node đính kèm "Sợi dây liên kết" archive_ref
                        const nodeId = `ArchiveNode_${domain}_${Date.now()}`;
                        this.structuredMemory.graph.upsertNode({
                            id: nodeId,
                            label: "ARCHIVED_CONCEPT",
                            properties: JSON.stringify({
                                domain: domain,
                                summary: summary,
                                archive_ref: archiveFileName, // Pointer đến ổ cứng
                                archived_at: Date.now()
                            })
                        });
                        logger.info(`[ArchivingCron] Created L3 ArchiveNode for domain '${domain}' -> ${archiveFileName}`);
                    }
                } catch (e: unknown) {
                    const errMsg = e instanceof Error ? e.message : String(e);
                    logger.warn(`[ArchivingCron] LLM synthesis failed for domain ${domain}: ${errMsg}`);
                }

                // 4. Dump dữ liệu ra file .jsonl và XÓA khỏi SQLite
                db.exec("BEGIN TRANSACTION;");
                try {
                    for (const v of vectors) {
                        // Lấy Event L1 tương ứng (nếu có)
                        let sourceEvents: any[] = [];
                        try {
                            const eventIds: string[] = JSON.parse(v.source_event_ids || "[]");
                            if (eventIds.length > 0) {
                                const placeholders = eventIds.map(() => '?').join(',');
                                sourceEvents = db.prepare(`SELECT * FROM events WHERE event_id IN (${placeholders})`).all(...eventIds);
                            }
                        } catch { /* ignore parse error */ }

                        const archiveData = {
                            vector: v,
                            events: sourceEvents,
                            archived_at: Date.now()
                        };

                        await fileHandle.write(JSON.stringify(archiveData) + "\n");

                        // Xóa ở L2
                        db.prepare("DELETE FROM vec_idx WHERE rowid = ?").run(BigInt(v.id));
                        db.prepare("DELETE FROM vectors_fts WHERE rowid = ?").run(BigInt(v.id));
                        db.prepare("DELETE FROM vectors_meta WHERE id = ?").run(v.id);

                        // Xóa ở L1
                        if (sourceEvents.length > 0) {
                            const eventIds = sourceEvents.map(e => e.event_id);
                            const placeholders = eventIds.map(() => '?').join(',');
                            db.prepare(`DELETE FROM events WHERE event_id IN (${placeholders})`).run(...eventIds);
                        }
                        
                        archivedCount++;
                    }
                    db.exec("COMMIT;");
                } catch (e: unknown) {
                    try { db.exec("ROLLBACK;"); } catch {}
                    const errMsg = e instanceof Error ? e.message : String(e);
                    logger.error(`[ArchivingCron] DB deletion transaction failed: ${errMsg}`);
                }
            }

            await fileHandle.close();

            // 5. Giải phóng không gian đĩa (VACUUM)
            if (archivedCount > 0) {
                logger.info(`[ArchivingCron] ✅ Archived ${archivedCount} vectors to ${archiveFileName}. Reclaiming disk space (VACUUM)...`);
                db.exec("VACUUM;");
                logger.info("[ArchivingCron] VACUUM complete.");
            }

        } catch (e: unknown) {
            const errMsg = e instanceof Error ? e.message : String(e);
            logger.error(`[ArchivingCron] Process failed: ${errMsg}`);
        } finally {
            this.isRunning = false;
        }

        return archivedCount;
    }
}
