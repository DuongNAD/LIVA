import * as fs from 'fs';
import * as path from 'path';
import { logger } from '../utils/logger';

/**
 * Triển khai thuật toán TurboQuant (QJL - Quantized Johnson-Lindenstrauss)
 * Thuật toán này giúp nén các Vector Embedding nặng về bit thấp hơn
 * giảm đến 6x lần bộ nhớ cho LLM KV Cache và Semantic Search (theo Paper của Google).
 */
export class TurboQuant {
    private projectionMatrix: number[][] | null = null;
    private targetDims: number;

    constructor(targetDims: number = 256) {
        this.targetDims = targetDims;
    }

    /**
     * Khởi tạo ma trận nén (Random Gaussian Matrix)
     */
    private initializeMatrix(inputDims: number) {
        if (this.projectionMatrix) return;
        this.projectionMatrix = Array.from({ length: this.targetDims }, () =>
            Array.from({ length: inputDims }, () => this.randomGaussian())
        );
    }

    /**
     * Nén một vector embedding float32 xuống dạng QJL (binary/small size)
     */
    public quantize(vector: number[]): number[] {
        if (vector.length === 0) return [];
        this.initializeMatrix(vector.length);

        const projected = this.projectionMatrix!.map(row => 
            row.reduce((sum, val, i) => sum + val * vector[i], 0)
        );

        // QJL thresholding (Sign function) để nén vector về -1 / 1 (hoặc 0/1 bits)
        return projected.map(val => (val > 0 ? 1 : -1));
    }

    /**
     * Tính toán khoảng cách (Hamming Distance hoặc Cosine sau nén)
     * Thao tác trên vector đã nén cực kỳ tối ưu cho bộ nhớ và CPU.
     */
    public compressedCosineSimilarity(q1: number[], q2: number[]): number {
        if (q1.length !== q2.length) return 0;
        let matchCount = 0;
        for (let i = 0; i < q1.length; i++) {
            if (q1[i] === q2[i]) matchCount++;
        }
        // Quy đổi Hamming Similarity về [-1, 1] range tương tự Cosine
        return (matchCount / q1.length) * 2 - 1;
    }

    // Box-Muller transform cho phân phối chuẩn
    private randomGaussian(): number {
        let u = 0, v = 0;
        while (u === 0) u = Math.random();
        while (v === 0) v = Math.random();
        return Math.sqrt(-2.0 * Math.log(u)) * Math.cos(2.0 * Math.PI * v);
    }
}

export interface MemoryEntry {
    content: string;
    role: string;
    embedding?: number[];
    compressedEmbedding?: number[];
}

export class QuantizedMemoryStore {
    private entries: MemoryEntry[] = [];
    private quantizer: TurboQuant;
    private filePath: string;

    constructor(filePath: string) {
        this.quantizer = new TurboQuant(256); // Nén xuống 256 chiều dạng nhị phân
        this.filePath = filePath;
        this.load();
    }

    public async addMemory(role: string, content: string, originalEmbedding: number[]) {
        const compressedEmbedding = this.quantizer.quantize(originalEmbedding);
        
        const entry: MemoryEntry = {
            role,
            content,
            compressedEmbedding,
        };
        this.entries.push(entry);
        this.save();
    }

    public searchSimilar(queryEmbedding: number[], topK: number = 3): MemoryEntry[] {
        const queryCompressed = this.quantizer.quantize(queryEmbedding);
        
        const results = this.entries.map(entry => {
            const score = entry.compressedEmbedding 
                ? this.quantizer.compressedCosineSimilarity(queryCompressed, entry.compressedEmbedding)
                : -1;
            return { entry, score };
        });

        results.sort((a, b) => b.score - a.score);
        return results.slice(0, topK).map(r => r.entry);
    }

    private save() {
        if (!fs.existsSync(path.dirname(this.filePath))) {
            fs.mkdirSync(path.dirname(this.filePath), { recursive: true });
        }
        const data = this.entries.map(e => JSON.stringify(e)).join('\n');
        fs.writeFileSync(this.filePath, data, 'utf-8');
    }

    private load() {
        if (fs.existsSync(this.filePath)) {
            const data = fs.readFileSync(this.filePath, 'utf-8');
            this.entries = data.split('\n').filter(line => line.trim()).map(line => {
                try {
                    return JSON.parse(line) as MemoryEntry;
                } catch {
                    return null;
                }
            }).filter(e => e !== null) as MemoryEntry[];
        }
    }
}
