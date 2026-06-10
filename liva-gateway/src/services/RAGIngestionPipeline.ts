import { logger } from "../utils/logger";
import { promises as fs } from "node:fs";
import * as path from "node:path";
import { Worker } from "node:worker_threads";
import { StructuredMemory } from "../memory/StructuredMemory";
import { EmbeddingService } from "./EmbeddingService";
import { DocumentChunker } from "./DocumentChunker";
import { generateULID } from "../utils/ULID";

export interface IngestResult {
    success: boolean;
    filePath: string;
    numPages?: number;
    numChunks: number;
    previewText?: string;
    processingTimeMs: number;
    error?: string;
}

export class RAGIngestionPipeline {
    private static instance: RAGIngestionPipeline;

    private constructor() {}

    public static getInstance(): RAGIngestionPipeline {
        if (!RAGIngestionPipeline.instance) {
            RAGIngestionPipeline.instance = new RAGIngestionPipeline();
        }
        return RAGIngestionPipeline.instance;
    }

    /**
     * Standardized RAG ingestion pipeline: loads, chunks, embeds, and saves document files.
     */
    public async ingestFile(filePath: string, agentId: string = "liva_core"): Promise<IngestResult> {
        const startTime = performance.now();
        const absolutePath = path.resolve(filePath);
        const fileName = path.basename(absolutePath);
        const ext = path.extname(absolutePath).toLowerCase();

        try {
            await fs.access(absolutePath);
            const sm = await StructuredMemory.create(agentId);
            const embeddingService = EmbeddingService.getInstance();
            const chunker = DocumentChunker.getInstance();

            let numPages = 0;
            let numChunks = 0;
            let previewText = "";
            const pendingChunksData: Array<{ content: string; vecId: string; fileTarget: string; metadata: Record<string, unknown> }> = [];

            if (ext === ".pdf") {
                // Parse PDF via Worker thread
                const pdfResult = await this.parsePdfViaWorker(absolutePath);
                numPages = pdfResult.numPages;
                previewText = pdfResult.previewText;

                // Process chunks page-by-page
                for (const page of pdfResult.pages) {
                    const pageChunks = chunker.chunkDocument(page.text, fileName);
                    for (let j = 0; j < pageChunks.length; j++) {
                        const chunk = pageChunks[j];
                        const vecId = `pdf_${fileName}_p${page.pageNum}_c${j}_${generateULID()}`;
                        pendingChunksData.push({
                            content: `[PDF Chunk - ${fileName} - Trang ${page.pageNum} - Part ${j}]: ${chunk.content}`,
                            vecId,
                            fileTarget: absolutePath,
                            metadata: {
                                section_title: chunk.metadata.section_title,
                                doc_source: fileName,
                                chunk_index: chunk.metadata.chunk_index,
                                page_num: page.pageNum
                            }
                        });
                    }
                }
            } else {
                // Markdown, txt, etc. - read direct
                const text = await fs.readFile(absolutePath, "utf-8");
                previewText = text.substring(0, 1000);

                const fileChunks = chunker.chunkDocument(text, fileName);
                for (let j = 0; j < fileChunks.length; j++) {
                    const chunk = fileChunks[j];
                    const vecId = `doc_${fileName}_c${j}_${generateULID()}`;
                    pendingChunksData.push({
                        content: `[Document Chunk - ${fileName} - Part ${j}]: ${chunk.content}`,
                        vecId,
                        fileTarget: absolutePath,
                        metadata: {
                            section_title: chunk.metadata.section_title,
                            doc_source: fileName,
                            chunk_index: chunk.metadata.chunk_index
                        }
                    });
                }
            }

            numChunks = pendingChunksData.length;

            if (numChunks > 0) {
                // Batch embedding in segments of 10 to avoid event loop blocking
                const batchSize = 10;
                for (let i = 0; i < pendingChunksData.length; i += batchSize) {
                    const batch = pendingChunksData.slice(i, i + batchSize);
                    const textsToEmbed = batch.map(b => b.content);
                    const vectors = await embeddingService.embedBatch(textsToEmbed);

                    const records = batch.map((item, index) => ({
                        vecId: item.vecId,
                        type: "ANCHOR",
                        content: item.content,
                        vector: vectors[index],
                        domain: "Document",
                        category: ext.substring(1).toUpperCase(),
                        fileTarget: item.fileTarget,
                        traceKeywords: [item.metadata.section_title as string]
                    }));

                    await sm.upsertVectorsBatch(records);
                    // Yield to event loop
                    await new Promise(resolve => setTimeout(resolve, 0));
                }
            }

            const processingTimeMs = Math.round(performance.now() - startTime);
            logger.info(`[RAGIngestionPipeline] Ingested "${fileName}": ${numChunks} chunks embedded in ${processingTimeMs}ms.`);

            return {
                success: true,
                filePath: absolutePath,
                numPages: ext === ".pdf" ? numPages : undefined,
                numChunks,
                previewText,
                processingTimeMs
            };

        } catch (error: unknown) {
            const errMsg = error instanceof Error ? error.message : String(error);
            logger.error(`[RAGIngestionPipeline] Ingestion failed for "${fileName}": ${errMsg}`);
            return {
                success: false,
                filePath: absolutePath,
                numChunks: 0,
                processingTimeMs: Math.round(performance.now() - startTime),
                error: errMsg
            };
        }
    }

    private parsePdfViaWorker(targetPath: string): Promise<{ numPages: number; previewText: string; pages: Array<{ pageNum: number; text: string }> }> {
        const workerCode = `
            import { parentPort, workerData } from 'node:worker_threads';
            
            async function run() {
                try {
                    const pdfjsLib = await import("pdfjs-dist/legacy/build/pdf.mjs");
                    const doc = await pdfjsLib.getDocument(workerData.targetPath).promise;
                    const numPages = doc.numPages;
                    
                    let previewText = "";
                    const pages = [];

                    for (let i = 1; i <= numPages; i++) {
                        const page = await doc.getPage(i);
                        const textContent = await page.getTextContent();
                        const pageText = textContent.items.map(item => item.str).join(" ");
                        
                        if (i <= 3) {
                            previewText += \`[Trang \${i}]\\n\${pageText}\\n\\n\`;
                        }
                        
                        if (pageText.trim().length > 50) {
                            pages.push({ pageNum: i, text: pageText });
                        }
                    }

                    parentPort.postMessage({ type: 'done', numPages, previewText, pages });
                } catch (err) {
                    parentPort.postMessage({ type: 'error', error: err.message || String(err) });
                }
            }
            run();
        `;

        return new Promise((resolve, reject) => {
            let isDone = false;
            const worker = new Worker(workerCode, {
                eval: true,
                workerData: { targetPath }
            });

            const watchdog = setTimeout(() => {
                if (!isDone) {
                    isDone = true;
                    logger.error(`[Watchdog] PDF parsing worker deadlocked. Terminating...`);
                    worker.terminate();
                    reject(new Error("PDF parsing timed out after 45 seconds."));
                }
            }, 45000);

            worker.on("message", (msg) => {
                if (msg.type === "error") {
                    isDone = true;
                    clearTimeout(watchdog);
                    reject(new Error(msg.error));
                } else if (msg.type === "done") {
                    isDone = true;
                    clearTimeout(watchdog);
                    resolve({
                        numPages: msg.numPages,
                        previewText: msg.previewText,
                        pages: msg.pages
                    });
                }
            });

            worker.on("error", (err) => {
                isDone = true;
                clearTimeout(watchdog);
                reject(err);
            });

            worker.on("exit", (code) => {
                isDone = true;
                clearTimeout(watchdog);
                if (code !== 0) {
                    reject(new Error(`Worker exited with code ${code}`));
                }
            });
        });
    }
}
