export interface SensoryContext {
    activeApp: string;
    windowTitle: string;
    clipboardText: string;
    capturedAt: number;
}

export class SensoryManager {
    private static instance: SensoryManager;
    private context: SensoryContext | null = null;
    
    // Tuổi thọ của Sensory Context: 30 giây (Theo yêu cầu UX tốt nhất)
    private readonly TTL_MS = 30000; 

    public static getInstance(): SensoryManager {
        if (!this.instance) {
            this.instance = new SensoryManager();
        }
        return this.instance;
    }

    /**
     * Hàm này sẽ được gọi thông qua một Global Hotkey trên máy tính
     * để LIVA đọc nhanh màn hình và clipboard.
     */
    public async captureContext(): Promise<void> {
        try {
            // Dynamic import để tương thích với các module phòng trường hợp file là CommonJS
            const activeWinModule = await import('active-win');
            const activeWindow = activeWinModule.default;
            const clipboardyModule = await import('clipboardy');
            const clipboardy = clipboardyModule.default;

            const win = await activeWindow();
            const clipText = await clipboardy.read();
            
            this.context = {
                activeApp: win?.owner?.name || 'Unknown',
                windowTitle: win?.title || 'Unknown',
                clipboardText: clipText || '',
                capturedAt: Date.now()
            };
            
            console.log(`[SensoryMemory] 👁️ Tóm gọn màn hình & clipboard.`);
        } catch (error) {
            console.error('[SensoryMemory] Lỗi khi kích hoạt giác quan:', error);
        }
    }

    /**
     * Bơm Sensory Memory vào System Prompt của Agent, 
     * nếu quá 30 giây kể từ lúc user bấm phím tóm màn hình, trí nhớ này bốc hơi.
     */
    public injectSensoryPrompt(): string {
        if (!this.context) return "";
        
        // Kiểm tra Time-To-Live (TTL)
        if (Date.now() - this.context.capturedAt > this.TTL_MS) {
            console.log(`[SensoryMemory] 🌬️ Ký ức cảm giác đã tự huỷ sau ${this.TTL_MS/1000}s`);
            this.context = null;
            return "";
        }
        
        let prompt = `\n<SystemSensory timestamp="${new Date(this.context.capturedAt).toLocaleTimeString('vi-VN')}">\n`;
        prompt += `- Người dùng đang thao tác trên phần mềm: ${this.context.activeApp}\n`;
        prompt += `- Dòng tiêu đề cửa sổ: ${this.context.windowTitle}\n`;
        
        if (this.context.clipboardText) {
            prompt += `- Nội dung Clipboard (Vừa copy): """${this.context.clipboardText}"""\n`;
        }
        prompt += `</SystemSensory>\n`;
        
        return prompt;
    }

    /**
     * Dọn dẹp trí nhớ cảm giác sau khi Agent đã phản hồi (Flushed context)
     */
    public flush(): void {
        this.context = null;
    }
}
