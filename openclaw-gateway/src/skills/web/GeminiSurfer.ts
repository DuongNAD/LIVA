import { chromium, Browser } from 'playwright-core';
import { logger } from "@utils/logger.js";
import * as fs from "node:fs";
import { exec } from "node:child_process";
import { promisify } from 'node:util';

const execAsync = promisify(exec);

export const metadata = {
    name: "gemini_surfer",
    search_keywords: ["gemini", "gemini_surfer", "google gemini", "hỏi gemini", "tra cứu gemini", "gemini ai", "hỏi ai khác", "deep research", "thinking model"],
    description: "Use when user says 'Ask Gemini', 'Search Gemini', 'Let Gemini solve this'. Piggybacks on user's Chrome browser (Port 9222) to query Gemini without blocks. Supports file upload, Thinking and Deep Research modes.",
    parameters: {
        type: "object",
        properties: {
            query: {
                type: "string",
                description: "Detailed question to search, ask, or request Gemini to analyze.",
            },
            modelType: {
                type: "string",
                enum: ["fast", "thinking", "pro"],
                description: "Model type: fast (default), thinking (complex problem solving), pro (advanced)"
            },
            useDeepResearch: {
                type: "boolean",
                description: "Enable Deep Research mode for in-depth information"
            },
            files: {
                type: "array",
                items: { type: "string" },
                description: "List of absolute file paths to upload (PDF, images, txt...)"
            }
        },
        required: ["query"],
    },
    requiresApproval: false
};

export const execute = async (args: { query: string, modelType?: string, useDeepResearch?: boolean, files?: string[] }): Promise<string> => {
    let browser: Browser | null = null;
    let page: any = null;

    logger.info(`[GeminiSurfer] Bắt đầu kết nối CDP (Cổng 9222)...`);

    // 1. File Guard: Xác thực file
    const validFiles: string[] = [];
    if (args.files && args.files.length > 0) {
        for (const file of args.files) {
            if (fs.existsSync(file)) {
                validFiles.push(file);
            } else {
                logger.warn(`[GeminiSurfer] Cảnh báo: File không tồn tại hoặc đường dẫn sai: ${file}`);
            }
        }
        if (validFiles.length === 0) {
            return "[System Error: Toàn bộ file đính kèm không tồn tại. Vui lòng kiểm tra lại đường dẫn.]";
        }
    }

    try {
        // 1. CDP Handshake
        browser = await chromium.connectOverCDP('http://127.0.0.1:9222');
    } catch {
        logger.warn(`[GeminiSurfer] Chrome chưa mở cổng 9222. Đang tự động kích hoạt Chrome...`);
        try {
            // Tắt Chrome cũ nếu bị treo
            await execAsync('taskkill /F /IM chrome.exe /T').catch(() => {});
            
            // Khởi chạy Chrome với Profile ẩn danh/cách ly bằng spawn (detached)
            let chromePath = "C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe";
            if (!fs.existsSync(chromePath)) {
                chromePath = "C:\\Program Files (x86)\\Google\\Chrome\\Application\\chrome.exe";
            }
            
            const { spawn } = await import('child_process');
            const child = spawn(chromePath, ['--remote-debugging-port=9222', '--user-data-dir=C:\\liva-chrome-profile'], {
                detached: true,
                stdio: 'ignore'
            });
            child.unref();
            
            // Đợi Chrome khởi động
            logger.info(`[GeminiSurfer] Chờ 3 giây để Chrome khởi động hoàn tất...`);
            await new Promise(r => setTimeout(r, 3000));
            
            browser = await chromium.connectOverCDP('http://127.0.0.1:9222');
        } catch (err: unknown) {
        const errMsg = err instanceof Error ? err.message : String(err);
            return `[System Error: Không thể tự động mở Chrome. Vui lòng kiểm tra lại đường dẫn cài đặt: ${errMsg}]`;
        }
    }

    try {
        // 2. Context Sharing & Tab Lifecycle
        const contexts = browser.contexts();
        const context = contexts.length > 0 ? contexts[0] : await browser.newContext();
        page = await context.newPage();

        logger.info(`[GeminiSurfer] Mở tab mới. Điều hướng tới Gemini...`);

        // 3. Login Guard & Navigation
        await page.goto('https://gemini.google.com/app', { waitUntil: 'domcontentloaded' });
        await page.waitForTimeout(2000); 

        const currentUrl = page.url();
        if (currentUrl.includes('accounts.google.com') || currentUrl.includes('Sign+in')) {
            return "[System Error: Trình duyệt chưa đăng nhập tài khoản Google. Vui lòng nhắc User mở tab mới và tự đăng nhập vào Google/Gemini trước.]";
        }

        // --- BẮT ĐẦU ADVANCED AUTOMATION ---

        // 4. Custom Gem Selection (GEM_AI)
        try {
            logger.info(`[GeminiSurfer] Đang kiểm tra và truy cập Custom Gem: GEM_AI...`);
            
            // Dùng xpath hoặc getByText để bắt chính xác
            const gemAiLocator = page.locator('text="GEM_AI"').first();
            
            // Nếu không thấy GEM_AI, thử mở Menu (Hamburger)
            if (!(await gemAiLocator.isVisible({ timeout: 2000 }).catch(() => false))) {
                logger.info(`[GeminiSurfer] Sidebar đang đóng. Thử click Hamburger Menu...`);
                // Nút Hamburger thường là nút đầu tiên trên Header có icon svg
                const menuBtn = page.locator('button[aria-label*="Trình đơn" i], button[aria-label*="Menu" i], header button:has(svg)').first();
                await menuBtn.click({ timeout: 2000 }).catch(() => {});
                await page.waitForTimeout(1000);
            }
            
            if (await gemAiLocator.isVisible({ timeout: 2000 }).catch(() => false)) {
                await gemAiLocator.click({ timeout: 3000 });
                await page.waitForTimeout(3000); // Đợi load môi trường Gem
                logger.info(`[GeminiSurfer] Đã truy cập thành công vào GEM_AI.`);
            } else {
                logger.info(`[GeminiSurfer] Không tìm thấy GEM_AI ở Sidebar. Tiếp tục với môi trường mặc định.`);
            }
        } catch {
            logger.warn(`[GeminiSurfer] Lỗi khi chuyển sang GEM_AI. Bỏ qua...`);
        }

        // 4a. Model Switcher (Idempotent & Soft-Fail)
        const targetModel = args.modelType || "fast";
        try {
            logger.info(`[GeminiSurfer] Đang kiểm tra Model... (Yêu cầu: ${targetModel})`);
            
            // Tìm nút chọn Model hiện tại
            const modelSelectorBtn = page.locator('button[aria-haspopup="menu"]').first();
            const currentModelText = await modelSelectorBtn.innerText({ timeout: 2000 }).catch(() => "");
            
            let needsSwitch = false;
            let targetRegex: RegExp | null = null;

            if (targetModel === "fast" && !currentModelText.match(/(flash|nhanh)/i)) {
                needsSwitch = true;
                targetRegex = /(flash|nhanh)/i;
            } else if (targetModel === "thinking" && !currentModelText.match(/(thinking|tư duy)/i)) {
                needsSwitch = true;
                targetRegex = /(thinking|tư duy)/i;
            } else if (targetModel === "pro" && !currentModelText.match(/(advanced|pro|nâng cao)/i)) {
                needsSwitch = true;
                targetRegex = /(advanced|pro|nâng cao)/i;
            }

            if (needsSwitch && targetRegex) {
                logger.info(`[GeminiSurfer] Bắt đầu chuyển đổi sang Model: ${targetModel}`);
                await modelSelectorBtn.click();
                await page.waitForTimeout(500); // Wait for menu
                
                // Click mục menu tương ứng bằng Regex đa ngôn ngữ
                const targetOption = page.locator(`[role="menuitem"]`).filter({ hasText: targetRegex }).first();
                await targetOption.click({ timeout: 2000 });
                await page.waitForTimeout(500); // Wait for UI update
            } else {
                logger.info(`[GeminiSurfer] Model hiện tại đã khớp, bỏ qua chuyển đổi.`);
            }
        } catch {
            logger.warn(`[GeminiSurfer] Soft-Fail: Không thể chuyển đổi Model (có thể do giới hạn tài khoản). Bỏ qua...`);
        }

        // 4b. Deep Research Toggle
        if (args.useDeepResearch) {
            try {
                logger.info(`[GeminiSurfer] Bật tính năng Deep Research...`);
                // Placeholder: Sẽ click toggle Deep Research khi Google mở UI (hiện tại soft-fail)
                const deepResearchToggle = page.locator('button[aria-label*="Deep Research"]').first();
                if (await deepResearchToggle.isVisible({ timeout: 1000 }).catch(() => false)) {
                    await deepResearchToggle.click();
                } else {
                    throw new Error("Not found");
                }
            } catch {
                logger.warn(`[GeminiSurfer] Soft-Fail: Không tìm thấy nút bật Deep Research. Bỏ qua...`);
            }
        }

        // 4c. File Upload (Smart Wait)
        if (validFiles.length > 0) {
            try {
                logger.info(`[GeminiSurfer] Đang tải lên ${validFiles.length} file...`);
                
                // Lắng nghe sự kiện filechooser TRƯỚC khi click
                const fileChooserPromise = page.waitForEvent('filechooser', { timeout: 8000 }).catch((e: any) => {
                    logger.warn(`[GeminiSurfer] Lỗi filechooser: ${e.message}`);
                    return null;
                });
                
                logger.info(`[GeminiSurfer] Đang tìm nút +...`);
                // Tìm nút "Thêm tệp đính kèm" (+) dựa trên cấu trúc DOM thực tế
                // CHÚ Ý: Google Gemini dùng icon có fonticon="add_2" cho nút +
                const attachBtn = page.locator('button[mattooltip="Mở trình đơn tệp đính kèm"], button[aria-label="Mở trình đơn tệp đính kèm"], button:has(mat-icon[fonticon="add_2"]), button:has(mat-icon[data-mat-icon-name="add_2"])').last();
                
                logger.info(`[GeminiSurfer] Đã tìm thấy nút +, tiến hành click bằng JS thuần...`);
                // Sử dụng JS thuần để click, vượt qua mọi rào cản overlay/ripple của Angular Material
                await attachBtn.waitFor({ state: 'attached', timeout: 3000 }).catch(() => {});
                await attachBtn.evaluate((node: any) => node.click()).catch(() => attachBtn.click({ timeout: 2000, force: true }));
                
                logger.info(`[GeminiSurfer] Đã click nút +, chờ menu bung ra...`);
                await page.waitForTimeout(1000); // Chờ menu bung ra hoàn toàn

                logger.info(`[GeminiSurfer] Đang tìm mục "Tải tệp lên" trong menu...`);
                // Thử click mục "Tải tệp lên" trong menu dựa trên data-test-id từ hình ảnh Inspect
                const uploadMenu = page.locator('[data-test-id="uploader-images-files-button-advanced"], images-files-uploader, [role="menuitem"]:has-text("Tải tệp lên")').last();
                
                logger.info(`[GeminiSurfer] Đã tìm thấy mục Tải tệp lên, tiến hành click bằng JS thuần...`);
                await uploadMenu.waitFor({ state: 'attached', timeout: 2000 }).catch(() => {});
                await uploadMenu.evaluate((node: any) => node.click()).catch(() => uploadMenu.click({ timeout: 2000, force: true }));
                
                logger.info(`[GeminiSurfer] Đã click Tải tệp lên, chờ hộp thoại OS...`);
                const fileChooser = await fileChooserPromise;
                if (fileChooser) {
                    logger.info(`[GeminiSurfer] Đã kích hoạt fileChooser, đang gán file...`);
                    await fileChooser.setFiles(validFiles);
                    
                    logger.info(`[GeminiSurfer] Chờ tải file hoàn tất...`);
                    await page.waitForSelector('file-attachment-chip, file-thumbnail, [data-test-id="uploaded-file"], .chunk-card', { timeout: 15000 }).catch(() => {});
                    await page.waitForTimeout(1000); 
                } else {
                    logger.warn(`[GeminiSurfer] Soft-Fail: Không thể kích hoạt hộp thoại chọn file. Bỏ qua upload...`);
                }
            } catch(e: unknown) {
            const errMsg = e instanceof Error ? e.message : String(e);
                logger.warn(`[GeminiSurfer] Lỗi trong quá trình upload file: ${errMsg}. Tiếp tục không cần file...`);
            }
        }

        // --- KẾT THÚC ADVANCED AUTOMATION ---

        // 5. Smart DOM Injection (Chống Ghosting)
        logger.info(`[GeminiSurfer] Đang điền câu hỏi...`);
        const inputLocator = page.locator('rich-textarea div[contenteditable="true"], div.ql-editor, div[contenteditable="true"]').first();
        await inputLocator.waitFor({ state: 'visible', timeout: 10000 });
        await inputLocator.fill(args.query);

        await page.waitForTimeout(500);

        logger.info(`[GeminiSurfer] Gửi câu hỏi...`);
        // Nhấn Escape để đóng bất kỳ menu/tooltip nào (như menu Tải tệp lên) đang che khuất màn hình
        await page.keyboard.press('Escape').catch(() => {});
        await page.waitForTimeout(500);
        
        const sendBtnLocator = page.locator('button[aria-label*="Send"], button[aria-label*="Gửi"], button.send-button, [data-test-id="send-button"]').first();
        
        // Dùng force: true để vượt qua mọi Overlay chặn click
        await sendBtnLocator.click({ force: true }).catch(async () => {
            // Fallback: Nhấn Enter
            await inputLocator.press('Enter');
        });

        // 6. Kích hoạt & Heartbeat Polling (Chống Hôn mê hệ thống)
        logger.info(`[GeminiSurfer] Đang chờ Gemini suy nghĩ... (Tối đa 5 phút)`);
        
        const responseLocator = page.locator('message-content, .model-response-text, [data-message-author-role="model"], model-response').last();
        
        // Timeout 5 phút cho model Tư duy/Deep Research
        await responseLocator.waitFor({ state: 'visible', timeout: 300000 }).catch(() => {});

        // Polling loop: Wait until text stops changing for 2 seconds (Max 5 mins)
        let previousText = "";
        let stableCount = 0;
        let elapsed = 0;
        
        while (elapsed < 300) {
            const currentText = await responseLocator.innerText().catch(() => "");
            
            if (currentText.length > 0 && currentText === previousText) {
                stableCount++;
                if (stableCount >= 2) {
                    break; // Hoàn thành
                }
            } else {
                stableCount = 0;
                previousText = currentText;
            }

            await page.waitForTimeout(5000);
            elapsed += 5;
            
            // Bắn nhịp tim về Log/FSM
            logger.info(`[GeminiSurfer] [Heartbeat] Gemini đang suy nghĩ... (${elapsed}s / 300s)`);
            // Nếu có UIController thì gọi: UIController.getInstance().broadcastUIEvent({ type: "TOOL_STATUS", msg: `Gemini đang suy nghĩ... (${elapsed}s/300s)` });
        }

        // 7. Data Extraction
        logger.info(`[GeminiSurfer] Đang trích xuất nội dung...`);
        const responseText = await responseLocator.innerText().catch(() => null);

        if (!responseText) {
            return "[System Warning: Lệnh đã được thực thi trên Gemini, nhưng không thể trích xuất chính xác văn bản trả lời do cấu trúc web bị thay đổi. Sếp có thể mở Chrome để tự xem.]";
        }

        logger.info(`[GeminiSurfer] Thành công!`);
        return `Đây là phản hồi từ Google Gemini:\n\n${responseText}`;

    } catch (err: unknown) {
    const errMsg = err instanceof Error ? err.message : String(err);
        logger.error({ err: errMsg }, `[GeminiSurfer] Lỗi Automation`);
        return `[System Error: Lỗi trong quá trình tự động hóa thao tác trình duyệt: ${errMsg}]`;
    } finally {
        // Cleanup
        if (page) {
            try { await page.close(); } catch (e: unknown) {
            const errMsg = e instanceof Error ? e.message : String(e); void e; }
        }
        if (browser) {
            try { await browser.close(); } catch (e: unknown) {
            const errMsg = e instanceof Error ? e.message : String(e); void e; } 
        }
        
        // Auto-kill Chrome để giải phóng RAM và tắt giao diện theo yêu cầu sếp
        logger.info(`[GeminiSurfer] Hoàn tất công việc. Đang tự động tắt Chrome...`);
        try {
            await execAsync('taskkill /F /IM chrome.exe /T');
            logger.info(`[GeminiSurfer] Đã tắt Chrome an toàn.`);
        } catch (e: unknown) {
        const errMsg = e instanceof Error ? e.message : String(e); void e; }
    }
};
