const { chromium } = require('playwright');
const fs = require('fs');

async function runStressTest() {
  console.log("Starting Playwright E2E Stress Test...");
  const browser = await chromium.launch({ headless: true });
  const context = await browser.newContext();
  const page = await context.newPage();

  // Mở trang LIVA UI
  await page.goto('http://localhost:5173');

  // Đợi cho đến khi nhận được config (ws kết nối xong)
  await page.waitForTimeout(3000); // 3 seconds to let backend initialize
  
  // Mở rộng giao diện chat (click vào nút expand)
  try {
    const expandBtn = await page.$('button[title="Mở rộng"]');
    if (expandBtn) await expandBtn.click();
    else {
      const icon = await page.$('.collapsed-capsule button');
      if (icon) await icon.click();
    }
    await page.waitForTimeout(1000);
  } catch(e) {}
  
  const html = await page.innerHTML('body');
  require('fs').writeFileSync('tests/dump.html', html);



  // Kịch bản sinh 1000 câu thoại
  const messagePool = [
    "Chào LIVA", // Chitchat
    "Hôm nay bạn thế nào?", // Chitchat
    "Nhắn tin nhắc bạn Hoàng Hiếu nhớ học toán", // Messaging
    "Email cho sếp bảo em xin nghỉ ốm", // Messaging
    "System status", // System Command
    "Mở dashboard", // System Command
    "messager", // Disambiguation typo test
    "Zalo", // Disambiguation response
    "Gửi cho Hùng qua zalo", // Disambiguation response
    "Bây giờ là mấy giờ?", // Chitchat
  ];

  console.log("Sending 1000 messages...");
  
  let successCount = 0;
  let halluCount = 0;

  // Lặp gửi 1000 tin nhắn
  for (let i = 0; i < 1000; i++) {
    const msg = messagePool[i % messagePool.length];
    
    // Tìm input 
    const input = await page.$('input[type="text"]');
    if (input) {
      await input.fill(msg);
      await input.press('Enter');
    }
    
    // Gửi dồn dập (thử nghiệm rate limit)
    // Delay 10ms giữa các tin
    await page.waitForTimeout(10); 
  }

  console.log("Finished sending 1000 messages. Waiting for responses to settle...");
  try {
    await page.waitForSelector('.chat-bubble-ai', { timeout: 30000 });
  } catch (e) {
    console.log("Timeout waiting for AI response.");
  }
  await page.waitForTimeout(5000); // Wait an extra 5s

  // Kiểm tra UI có hiện "system_busy" toast hay log không
  const chatBubbles = await page.$$('.chat-bubble-ai');
  console.log(`Total AI responses rendered: ${chatBubbles.length}`);

  // Phân tích nội dung AI phản hồi để phát hiện Hallucination
  let hasHallucination = false;
  for (const bubble of chatBubbles) {
    const text = await bubble.innerText();
    // Nếu có mention "sở thích" hay "phở Hà Nội", là hallucination (từ lần trước)
    if (text.includes("phở") || text.includes("cà phê trứng") || text.includes("sở thích")) {
      hasHallucination = true;
      halluCount++;
    }
  }

  if (hasHallucination) {
    console.log("❌ Lỗi: Phát hiện Hallucination trong kết quả trả về.");
  } else {
    console.log("✅ Không phát hiện Hallucination. Context đã được giới hạn thành công.");
  }

  // Đếm số lượng phản hồi (nếu Rate Limiter hoạt động đúng, nó phải loại bỏ các tin nhắn dồn dập
  // và chỉ phản hồi các tin hợp lệ. 1000 tin gửi với 10ms delay => chỉ vài tin lọt qua)
  if (chatBubbles.length > 0 && chatBubbles.length < 500) {
    console.log(`✅ Rate Limiter hoạt động tốt, chặn bớt yêu cầu: số lượng phản hồi AI = ${chatBubbles.length} / 1000`);
  } else {
    console.log(`❌ Lỗi: Có quá nhiều hoặc không có phản hồi (${chatBubbles.length}), Rate Limiter có thể chưa hoạt động đúng hoặc UI render sai.`);
  }
  
  await page.screenshot({ path: 'tests/screenshot.png' });

  await browser.close();
}

runStressTest().catch(e => {
  console.error("Test failed:", e);
  process.exit(1);
});
