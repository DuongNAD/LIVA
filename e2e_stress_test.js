const { chromium } = require('playwright');
const fs = require('fs');

const CHITCHAT = [
    "Xin chào", "Cậu khỏe không?", "Hôm nay tôi mệt quá", "Kể tôi nghe một câu chuyện",
    "Bạn tên là gì?", "Bạn làm được những gì?", "Thời tiết hôm nay thế nào?",
    "Cảm ơn bạn nhé", "Tạm biệt", "Chúc ngủ ngon", "Bạn có thích âm nhạc không?"
];

const CORE_SKILLS = [
    "Mấy giờ rồi?", "Hôm nay là ngày bao nhiêu?", "Bây giờ là mấy giờ ở New York?",
    "Thời tiết ở Hà Nội hôm nay thế nào?", "Dự báo thời tiết ngày mai"
];

const MESSAGING_SKILLS = [
    "Nhắn tin cho Mẹ trên Zalo",
    "Nhắn với Phạm Vũ trên messages đuổi nó về nhà",
    "Gửi tin nhắn cho sếp xin nghỉ ốm",
    "Nhắn tin hỏi thăm sức khỏe Minh",
    "Gửi messenger cho Phạm Vũ bảo tôi đang bận",
    "Nhắn Zalo cho anh Huy gửi báo cáo",
    "Nhắn tin choi Minh Nguyễn đuổi nó về nhà", // Testing our previous fix!
    "Gửi mail cho khách hàng"
];

const AMBIGUOUS_SKILLS = [
    "Nhắn tin cho Mẹ", // Should trigger disambiguation gate
    "Nhắn cho Phạm Vũ báo tôi đến trễ" // Should trigger disambiguation gate
];

function generate1000Phrases() {
    const phrases = [];
    // We want a good mix, primarily testing the context window and rate limiting
    // 70% chitchat/core, 20% messaging, 10% ambiguous
    for (let i = 0; i < 1000; i++) {
        const rand = Math.random();
        if (rand < 0.7) {
            phrases.push(Math.random() < 0.5 ? CHITCHAT[Math.floor(Math.random() * CHITCHAT.length)] : CORE_SKILLS[Math.floor(Math.random() * CORE_SKILLS.length)]);
        } else if (rand < 0.9) {
            phrases.push(MESSAGING_SKILLS[Math.floor(Math.random() * MESSAGING_SKILLS.length)]);
        } else {
            phrases.push(AMBIGUOUS_SKILLS[Math.floor(Math.random() * AMBIGUOUS_SKILLS.length)]);
        }
    }
    // Make sure we end with something specific to check hallucination
    phrases[998] = "Nhắn tin choi Minh Nguyễn đuổi nó về nhà";
    phrases[999] = "Hãy tóm tắt lại những gì tôi vừa yêu cầu";
    return phrases;
}

async function run() {
    const phrases = generate1000Phrases();
    const logStream = fs.createWriteStream('tests/e2e/stress_test_log.txt', { flags: 'a' });
    
    console.log("Starting Playwright stress test with 1000 messages...");
    logStream.write("=== STARTING 1000 MESSAGE STRESS TEST ===\n");
    
    const browser = await chromium.launch({ headless: true }); // headless for faster execution
    const context = await browser.newContext();
    const page = await context.newPage();
    
    // Listen to console to catch any Vue/Frontend errors
    page.on('console', msg => {
        if (msg.type() === 'error') {
            fs.appendFileSync('tests/e2e/stress_test_errors.txt', `[BROWSER ERROR] ${msg.text()}\n`);
        }
    });

const { unpack } = require('msgpackr');

    let aiResponseResolver = null;
    page.on('websocket', ws => {
        ws.on('framereceived', frame => {
            let data = null;
            if (Buffer.isBuffer(frame.payload)) {
                try {
                    const type = frame.payload[0];
                    if (type === 0x02) {
                        data = unpack(frame.payload.slice(1));
                    }
                } catch (e) {}
            } else if (typeof frame.payload === 'string') {
                try {
                    data = JSON.parse(frame.payload);
                } catch (e) {}
            }

            if (data && data.event) {
                if (data.event === 'ai_spoken_response') {
                    const responseText = data.payload?.text || '';
                    logStream.write(`[AI Response] ${responseText.substring(0, 500)}\n`); // truncated to prevent massive logs from html
                    // Auto-approve HITL requests for testing
                    if (responseText.includes('hitl-btn-approve')) {
                        logStream.write(`[Test Auto-HITL] Detected HITL guard. Auto-approving with 'yes'...\n`);
                        setTimeout(async () => {
                            try {
                                const inputSelector = 'input[type="text"]';
                                await page.fill(inputSelector, 'yes');
                                await page.press(inputSelector, 'Enter');
                            } catch(e) {}
                        }, 1500);
                        // Do NOT resolve aiResponseResolver yet
                    } else if (responseText.toLowerCase().includes('kênh nào') || responseText.toLowerCase().includes('qua đâu')) {
                        logStream.write(`[Test Auto-Reply] AI asked for channel. Replying with 'Zalo'...\n`);
                        setTimeout(async () => {
                            try {
                                const inputSelector = 'input[type="text"]';
                                await page.fill(inputSelector, 'Zalo');
                                await page.press(inputSelector, 'Enter');
                            } catch(e) {
                                logStream.write(`[Test Auto-Reply Error] ${e.message}\n`);
                            }
                        }, 1500);
                    } else if (responseText.toLowerCase().includes('nội dung gì') || responseText.toLowerCase().includes('nhắn gì') || responseText.toLowerCase().includes('nội dung tin nhắn')) {
                        logStream.write(`[Test Auto-Reply] AI asked for content. Replying with 'Chào bạn, đây là tin nhắn test tự động'...\n`);
                        setTimeout(async () => {
                            try {
                                const inputSelector = 'input[type="text"]';
                                await page.fill(inputSelector, 'Chào bạn, đây là tin nhắn test tự động');
                                await page.press(inputSelector, 'Enter');
                            } catch(e) {
                                logStream.write(`[Test Auto-Reply Error] ${e.message}\n`);
                            }
                        }, 1500);
                    } else if (aiResponseResolver) {
                        // Regular final response
                        aiResponseResolver();
                        aiResponseResolver = null;
                    }
                } else if (data.event === 'system_notification' || data.event === 'gateway_error') {
                    logStream.write(`[System] ${JSON.stringify(data.payload)}\n`);
                } else if (data.event === 'system_busy') {
                    logStream.write(`[System Busy] Spam detected by Gateway. Resolving to prevent hang.\n`);
                    if (aiResponseResolver) {
                        aiResponseResolver();
                        aiResponseResolver = null;
                    }
                }
            }
        });
    });

    await page.goto('http://127.0.0.1:5173');
    await page.waitForTimeout(5000); // Wait for connection and Live2D model to load

    for (let i = 0; i < 1000; i++) {
        const text = phrases[i];
        
        try {
            const inputSelector = 'input[type="text"]';
            await page.fill(inputSelector, text);
            
            logStream.write(`[Test ${i}] Sent: ${text}\n`);
            
            // Wait for AI to respond, or timeout after 60 seconds
            let timedOut = false;
            let timeoutId;
            const responsePromise = new Promise(resolve => {
                aiResponseResolver = resolve;
                timeoutId = setTimeout(() => {
                    timedOut = true;
                    resolve();
                }, 60000);
            });
            
            // Press Enter AFTER setting up the listener
            await page.press(inputSelector, 'Enter');
            
            await responsePromise;
            clearTimeout(timeoutId);
            
            if (timedOut) {
                logStream.write(`[Error] AI did not respond after 60 seconds for Test ${i}!\n`);
            }
            
            // Wait 1.5 seconds between tests to avoid spam protection
            await new Promise(r => setTimeout(r, 1500));
            
            if (i % 10 === 0) {
                console.log(`[Progress] Sent ${i}/1000 messages`);
            }
        } catch (e) {
            console.error(`Error at message ${i}:`, e);
            logStream.write(`Error at message ${i}: ${e.message}\n`);
            break;
        }
    }

    console.log("Finished sending 1000 messages! Waiting a bit for final responses...");
    await page.waitForTimeout(5000);
    
    logStream.write("=== FINISHED ===\n");
    logStream.end();
    
    await browser.close();
}

run().catch(console.error);
