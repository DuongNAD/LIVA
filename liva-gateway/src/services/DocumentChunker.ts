import { logger } from "../utils/logger";

export interface Chunk {
    content: string;
    metadata: {
        section_title: string;
        doc_source: string;
        chunk_index: number;
        token_count: number;
    };
}

export interface ChunkerOptions {
    maxWords?: number;
    overlapWords?: number;
}

export class DocumentChunker {
    private static instance: DocumentChunker;

    private constructor() {}

    public static getInstance(): DocumentChunker {
        if (!DocumentChunker.instance) {
            DocumentChunker.instance = new DocumentChunker();
        }
        return DocumentChunker.instance;
    }

    /**
     * Splits a document (Markdown or plain text) into logical chunks.
     * Respects headers (#), code blocks (```), and paragraphs (\n\n).
     */
    public chunkDocument(text: string, docSource: string, options: ChunkerOptions = {}): Chunk[] {
        const maxWords = options.maxWords ?? 200;
        const overlapWords = options.overlapWords ?? 50;

        if (!text || text.trim() === "") {
            return [];
        }

        // Phase 1: Parse document into flat units
        const units = this.parseToUnits(text);

        // Phase 2: Convert units into sub-units (items) that fit within maxWords
        const items = this.splitUnitsToItems(units, maxWords);

        // Phase 3: Sliding window chunk compilation
        const chunks = this.compileChunks(items, docSource, maxWords, overlapWords);

        logger.debug(`[DocumentChunker] Chunked document "${docSource}": ${text.length} chars -> ${chunks.length} chunks.`);
        return chunks;
    }

    // Helper to count words
    private countWords(text: string): number {
        if (!text) return 0;
        return text.trim().split(/\s+/).filter(w => w.length > 0).length;
    }

    // Parse the document into a list of units (headers, code blocks, paragraphs)
    private parseToUnits(text: string): Array<{ type: "header" | "code_block" | "paragraph"; content: string; headerLevel?: number; headerText?: string }> {
        const lines = text.split(/\r?\n/);
        const units: Array<{ type: "header" | "code_block" | "paragraph"; content: string; headerLevel?: number; headerText?: string }> = [];

        let inCodeBlock = false;
        let codeBlockLines: string[] = [];
        let currentParagraphLines: string[] = [];

        const emitParagraph = () => {
            if (currentParagraphLines.length > 0) {
                units.push({
                    type: "paragraph",
                    content: currentParagraphLines.join("\n").trim()
                });
                currentParagraphLines = [];
            }
        };

        for (let i = 0; i < lines.length; i++) {
            const line = lines[i];

            if (line.trim().startsWith("```")) {
                if (inCodeBlock) {
                    // End of code block
                    codeBlockLines.push(line);
                    units.push({
                        type: "code_block",
                        content: codeBlockLines.join("\n")
                    });
                    codeBlockLines = [];
                    inCodeBlock = false;
                } else {
                    // Start of code block
                    emitParagraph();
                    codeBlockLines.push(line);
                    inCodeBlock = true;
                }
                continue;
            }

            if (inCodeBlock) {
                codeBlockLines.push(line);
                continue;
            }

            // Check if line is a header
            const headerMatch = line.match(/^(#{1,6})\s+(.+)$/);
            if (headerMatch) {
                emitParagraph();
                units.push({
                    type: "header",
                    content: line,
                    headerLevel: headerMatch[1].length,
                    headerText: headerMatch[2].trim()
                });
                continue;
            }

            // Check if line is empty (paragraph boundary)
            if (line.trim() === "") {
                emitParagraph();
            } else {
                currentParagraphLines.push(line);
            }
        }

        // Emit any trailing paragraph or unclosed code block
        if (inCodeBlock && codeBlockLines.length > 0) {
            units.push({
                type: "code_block",
                content: codeBlockLines.join("\n")
            });
        } else {
            emitParagraph();
        }

        return units;
    }

    // Split units that exceed maxWords into smaller items
    private splitUnitsToItems(
        units: Array<{ type: "header" | "code_block" | "paragraph"; content: string; headerLevel?: number; headerText?: string }>,
        maxWords: number
    ): Array<{ text: string; sectionTitle: string; wordCount: number; isHeader: boolean }> {
        const items: Array<{ text: string; sectionTitle: string; wordCount: number; isHeader: boolean }> = [];
        const activeHeaders: string[] = [];

        for (const unit of units) {
            if (unit.type === "header") {
                const lvl = unit.headerLevel || 1;
                const txt = unit.headerText || "";
                activeHeaders[lvl - 1] = txt;
                activeHeaders.length = lvl; // Truncate sub-headers
                const sectionTitle = activeHeaders.filter(Boolean).join(" > ") || "General";

                items.push({
                    text: unit.content,
                    sectionTitle,
                    wordCount: this.countWords(unit.content),
                    isHeader: true
                });
                continue;
            }

            const sectionTitle = activeHeaders.filter(Boolean).join(" > ") || "General";
            const wordCount = this.countWords(unit.content);

            if (wordCount <= maxWords) {
                items.push({
                    text: unit.content,
                    sectionTitle,
                    wordCount,
                    isHeader: false
                });
                continue;
            }

            if (unit.type === "code_block") {
                // Split large code block line by line, preserving backticks
                const lines = unit.content.split("\n");
                let langSpec = "";
                if (lines[0] && lines[0].startsWith("```")) {
                    langSpec = lines[0].substring(3).trim();
                }

                const codeLines = lines.slice(1, -1);
                let currentLinesGroup: string[] = [];
                let currentGroupWords = 0;

                for (const line of codeLines) {
                    const lineWords = this.countWords(line);
                    if (currentLinesGroup.length > 0 && currentGroupWords + lineWords > maxWords - 10) {
                        const codeText = `\`\`\`${langSpec}\n${currentLinesGroup.join("\n")}\n\`\`\``;
                        items.push({
                            text: codeText,
                            sectionTitle,
                            wordCount: this.countWords(codeText),
                            isHeader: false
                        });
                        currentLinesGroup = [];
                        currentGroupWords = 0;
                    }
                    currentLinesGroup.push(line);
                    currentGroupWords += lineWords;
                }

                if (currentLinesGroup.length > 0) {
                    const codeText = `\`\`\`${langSpec}\n${currentLinesGroup.join("\n")}\n\`\`\``;
                    items.push({
                        text: codeText,
                        sectionTitle,
                        wordCount: this.countWords(codeText),
                        isHeader: false
                    });
                }
            } else {
                // Split large paragraph into sentences
                const sentences = unit.content.split(/(?<=[.!?])\s+/);
                for (const sentence of sentences) {
                    const sentenceWords = this.countWords(sentence);
                    if (sentenceWords > maxWords) {
                        const words = sentence.split(/\s+/);
                        let subWords: string[] = [];
                        for (const w of words) {
                            if (subWords.length > 0 && subWords.length + 1 > maxWords) {
                                const subText = subWords.join(" ");
                                items.push({
                                    text: subText,
                                    sectionTitle,
                                    wordCount: this.countWords(subText),
                                    isHeader: false
                                });
                                subWords = [];
                            }
                            subWords.push(w);
                        }
                        if (subWords.length > 0) {
                            const subText = subWords.join(" ");
                            items.push({
                                text: subText,
                                sectionTitle,
                                wordCount: this.countWords(subText),
                                isHeader: false
                            });
                        }
                    } else if (sentenceWords > 0) {
                        items.push({
                            text: sentence,
                            sectionTitle,
                            wordCount: sentenceWords,
                            isHeader: false
                        });
                    }
                }
            }
        }

        return items;
    }

    // Sliding window chunk compilation
    private compileChunks(
        items: Array<{ text: string; sectionTitle: string; wordCount: number; isHeader: boolean }>,
        docSource: string,
        maxWords: number,
        overlapWords: number
    ): Chunk[] {
        const chunks: Chunk[] = [];
        let i = 0;
        let chunkIndex = 0;

        while (i < items.length) {
            let currentWords = 0;
            let j = i;
            const chunkItems: typeof items = [];

            while (j < items.length) {
                const item = items[j];
                if (currentWords > 0 && currentWords + item.wordCount > maxWords) {
                    break;
                }
                chunkItems.push(item);
                currentWords += item.wordCount;
                j++;
            }

            if (chunkItems.length === 0 && j < items.length) {
                chunkItems.push(items[j]);
                currentWords += items[j].wordCount;
                j++;
            }

            const content = chunkItems.map(item => item.text).join("\n\n");
            const sectionTitle = chunkItems[chunkItems.length - 1]?.sectionTitle || "General";

            chunks.push({
                content,
                metadata: {
                    section_title: sectionTitle,
                    doc_source: docSource,
                    chunk_index: chunkIndex++,
                    token_count: Math.ceil(currentWords * 1.3)
                }
            });

            if (j >= items.length) {
                break;
            }

            let overlapCount = 0;
            let nextI = j;
            while (nextI > i) {
                const item = items[nextI - 1];
                if (overlapCount + item.wordCount > overlapWords) {
                    break;
                }
                overlapCount += item.wordCount;
                nextI--;
            }

            if (nextI === i) {
                i = i + 1;
            } else {
                i = nextI;
            }
        }

        return chunks;
    }
}
