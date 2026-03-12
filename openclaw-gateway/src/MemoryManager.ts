import * as fs from 'fs/promises';
import * as path from 'path';

export interface ChatMessage {
    role: 'user' | 'assistant' | 'system';
    content: string;
    timestamp: number;
}

export class MemoryManager {
    private memoryDirectory: string;
    private shortTermFilePath: string;
    private longTermFilePath: string;

    constructor(agentId: string) {
        this.memoryDirectory = path.join(process.cwd(), 'data', 'agents', agentId);
        this.shortTermFilePath = path.join(this.memoryDirectory, 'short_term_memory.jsonl');
        // Bổ sung đường dẫn cho bộ nhớ dài hạn (Long-term Memory)
        this.longTermFilePath = path.join(this.memoryDirectory, 'long_term_memory.md');
    }

    public async initialize(): Promise<void> {
        try {
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
        const message: ChatMessage = {
            role: role,
            content: content,
            timestamp: Date.now()
        };

        const jsonLine = JSON.stringify(message) + '\n';

        try {
            await fs.appendFile(this.shortTermFilePath, jsonLine, 'utf-8');
            console.log(`[Memory] Đã lưu tin nhắn của [${role}] vào Short-term Memory`);
        } catch (error) {
            console.error('[Memory] Lỗi khi ghi tin nhắn (Write error):', error);
        }
    }

    public async getShortTermHistory(): Promise<ChatMessage[]> {
        try {
            const data = await fs.readFile(this.shortTermFilePath, 'utf-8');
            const lines = data.split('\n').filter(line => line.trim() !== '');
            return lines.map(line => JSON.parse(line));
        } catch (error) {
            return [];
        }
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
    
    // Đọc toàn bộ tệp Markdown để bơm làm ngữ cảnh hệ thống (System Prompt injection)
    public async getLongTermContext(): Promise<string> {
         try {
             return await fs.readFile(this.longTermFilePath, 'utf-8');
         } catch (error) {
             return '';
         }
    }
}