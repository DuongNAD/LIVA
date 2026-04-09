import * as fs from 'fs/promises';
import * as path from 'path';
import { QuantizedMemoryStore } from './memory/TurboQuantStore';
import { pipeline, FeatureExtractionPipeline } from '@xenova/transformers';

export interface ChatMessage {
    role: 'user' | 'assistant' | 'system';
    content: string;
    timestamp: number;
}

export class MemoryManager {
    private memoryDirectory: string;
    private shortTermFilePath: string;
    private longTermFilePath: string;
    private userProfilePath: string;
    private quantStore: QuantizedMemoryStore;
    private embedder: FeatureExtractionPipeline | null = null;

    constructor(agentId: string) {
        this.memoryDirectory = path.join(process.cwd(), 'data', 'agents', agentId);
        this.shortTermFilePath = path.join(this.memoryDirectory, 'short_term_memory.jsonl');
        // Bổ sung đường dẫn cho bộ nhớ dài hạn (Long-term Memory)
        this.longTermFilePath = path.join(this.memoryDirectory, 'long_term_memory.md');
        // File user_profile.json (lưu trữ hồ sơ cá nhân của người dùng)
        this.userProfilePath = path.join(process.cwd(), 'src', 'user_profile.json');
        
        // Khởi tạo bộ nhớ nén siêu nhẹ
        this.quantStore = new QuantizedMemoryStore(path.join(this.memoryDirectory, 'turbo_quant_memory.jsonl'));
    }

    public async initialize(): Promise<void> {
        try {
            // Load Local Embedding Model (Không dùng external API)
            console.log('[Memory] Đang nạp mô hình Nhúng (Embedding Model) cục bộ...');
            try {
                this.embedder = await pipeline('feature-extraction', 'Xenova/all-MiniLM-L6-v2');
                console.log('[Memory] Đã load xong Local Embedding Model (Xenova).');
            } catch (err: any) {
                console.error('[Memory] Không thể chạy pipeline Xenova embeddings:', err.message);
            }

            await fs.mkdir(this.memoryDirectory, { recursive: true });
            
            // Khởi tạo tệp Markdown nếu chưa tồn tại (Initialization)
            try {
                await fs.access(this.longTermFilePath);
            } catch {
                const initialContent = `# Hồ Sơ Ký Ức Dài Hạn (Long-term Context)\n\n*Hệ thống sẽ định kỳ trích xuất (extract) và ghi chú các sự thật (facts) quan trọng vào đây.*\n\n---\n\n## Thói quen & Sở thích (Habits & Preferences)\n\n## Kiến thức đã học (Learned Knowledge)\n`;
                await fs.writeFile(this.longTermFilePath, initialContent, 'utf-8');
            }
            
            console.log(`[Memory] Đã sẵn sàng không gian lưu trữ tại: ${this.memoryDirectory}`);
        } catch (error) {
            console.error('[Memory] Lỗi khởi tạo (Initialization error):', error);
        }
    }

    public async addMessage(role: 'user' | 'assistant' | 'system', content: string): Promise<void> {
        let embeddingVector: number[] = Array.from({length: 256}, () => Math.random() * 2 - 1);
        
        if (this.embedder) {
            // Tạo vector thật từ LLM thay vì dummy random
            const output = await this.embedder(content, { pooling: 'mean', normalize: true });
            embeddingVector = Array.from(output.data);
        }
        
        await this.quantStore.addMemory(role, content, embeddingVector);
        console.log(`[Memory] Đã nén và lưu tin nhắn của [${role}] bằng TurboQuant QJL`);
    }

    public async getShortTermHistory(): Promise<ChatMessage[]> {
        // [TODO]: Lấy context ngữ nghĩa dựa theo Vector Search nếu cần thiết
        // Hiện tại tạm đọc thẳng từ `quantStore` nếu muốn full context, nhưng 
        // mục đích của TurboQuant là trích xuất theo Similarity
        
        const rawHistory = await fs.readFile(path.join(this.memoryDirectory, 'turbo_quant_memory.jsonl'), 'utf-8').catch(() => '');
        const lines = rawHistory.split('\n').filter(line => line.trim() !== '');
        try {
            return lines.map(line => {
                const parsed = JSON.parse(line);
                return { role: parsed.role, content: parsed.content, timestamp: Date.now() };
            });
        } catch {
            return [];
        }
    }

    public async getHybridContext(currentQuery: string, windowSize: number = 6): Promise<ChatMessage[]> {
        // 1. Tạo vector đại diện cho câu hỏi hiện tại
        let queryEmbedding: number[] = Array.from({length: 256}, () => Math.random() * 2 - 1);
        if (this.embedder) {
            try {
                const output = await this.embedder(currentQuery, { pooling: 'mean', normalize: true });
                queryEmbedding = Array.from(output.data);
            } catch(e) {
                console.error("[Memory] Lỗi nhúng văn bản (Embedding error):", e);
            }
        }

        // 2. Tải toàn bộ cửa sổ lịch sử hiện tại
        const fullHistory = await this.getShortTermHistory();

        // Nếu lịch sử còn ngắn, tải thẳng luôn không cần RAG
        if (fullHistory.length <= windowSize) {
            return fullHistory; 
        }

        // 3. Sử dụng Sliding Window tách 5-6 tin nhắn gần nhất ráp nguyên bản (Chronological)
        const recentWindow = fullHistory.slice(-windowSize);
        const recentContents = new Set(recentWindow.map(m => m.content.trim()));

        // 4. Khứ hồi lượng tử các tin nhắn trùng lập ngữ nghĩa ẩn sâu dưới đáy file
        const semanticResults = this.quantStore.searchSimilar(queryEmbedding, 3);
        
        const recalledChat: ChatMessage[] = [];
        for (const entry of semanticResults) {
            // Loại trừ tin nhắn vừa nói nãy lặp lại, và bỏ qua system prompt
            if (entry.role !== 'system' && !recentContents.has(entry.content.trim())) {
                recalledChat.push({
                    role: 'system',
                    content: `[Ký ức cũ liên quan]: Lục lại lịch sử, tôi nhớ ${entry.role === 'user' ? 'người dùng' : 'bản thân (AI)'} từng nói: "${entry.content}"`,
                    timestamp: Date.now()
                });
            }
        }

        console.log(`[Memory] Khứ hồi ${recalledChat.length} ký ức cũ, ghép với ${recentWindow.length} tin tức thời.`);
        // 5. Kết hợp: Những tin nhắn được khứ hồi nằm trên cùng + Chuỗi hội thoại tức thời nằm ở dưới (kề prompt AI)
        return [...recalledChat, ...recentWindow];
    }

    // Phương thức mới: Cập nhật thông tin vào bộ nhớ dài hạn định dạng Markdown
    public async updateLongTermMemory(category: string, facts: string[]): Promise<void> {
        try {
            let currentContent = await fs.readFile(this.longTermFilePath, 'utf-8');
            
            // Xây dựng chuỗi văn bản danh sách (Bullet points formatting)
            const newFacts = facts.map(fact => `- ${fact}`).join('\n');
            const sectionHeader = `## ${category}`;
            
            if (currentContent.includes(sectionHeader)) {
                // Nếu mục đã tồn tại, chèn thêm (append) vào ngay dưới tiêu đề đó
                currentContent = currentContent.replace(
                    sectionHeader, 
                    `${sectionHeader}\n${newFacts}`
                );
            } else {
                // Nếu danh mục chưa tồn tại, tạo phần mới ở cuối tệp
                currentContent += `\n${sectionHeader}\n${newFacts}\n`;
            }

            await fs.writeFile(this.longTermFilePath, currentContent, 'utf-8');
            console.log(`[Memory] Đã lưu vĩnh viễn (Persisted) vào Long-term Memory ở danh mục: ${category}`);
        } catch (error) {
            console.error('[Memory] Lỗi khi cập nhật bộ nhớ dài hạn:', error);
        }
    }
    
    // Đọc toàn bộ tệp Markdown để bám làm ngữ cảnh hệ thống (System Prompt injection)
    public async getLongTermContext(): Promise<string> {
         try {
             return await fs.readFile(this.longTermFilePath, 'utf-8');
         } catch (error) {
             return '';
         }
    }

    // --- Các phương thức làm việc với user profile ---

    public async getUserProfile(): Promise<any> {
        try {
            const data = await fs.readFile(this.userProfilePath, 'utf-8');
            return JSON.parse(data);
        } catch (error) {
            console.error('[Memory] Không thể đọc user_profile.json, trả về null.', error);
            return null;
        }
    }

    public async updateUserProfile(updates: any): Promise<void> {
        try {
            const currentProfile = (await this.getUserProfile()) || {};
            const newProfile = { ...currentProfile, ...updates };
            
            await fs.writeFile(
                this.userProfilePath, 
                JSON.stringify(newProfile, null, 2), 
                'utf-8'
            );
            console.log('[Memory] Đã cập nhật user_profile.json thành công.');
        } catch (error) {
            console.error('[Memory] Lỗi khi cập nhật user_profile.json:', error);
        }
    }
}