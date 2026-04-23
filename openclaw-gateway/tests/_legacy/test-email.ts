import { ImapFlow } from 'imapflow';
import { config } from 'dotenv';
import { execute as readEmailsExecute } from './src/skills/ReadEmails';

config(); // Tải biến từ .env

async function testConnection() {
    console.log("-----------------------------------------");
    console.log("Kiểm tra thư viện thuần (Raw IMAPFlow):");
    
    // Test 1: Thử dùng cấu hình môi trường
    const host = process.env.EMAIL_HOST;
    const port = parseInt(process.env.EMAIL_PORT || "993", 10);
    const user = process.env.EMAIL_USER?.replace(/^"|"$/g, '');
    const pass = process.env.EMAIL_PASS?.replace(/^"|"$/g, '');

    console.log(`- HOST: ${host}`);
    console.log(`- PORT: ${port}`);
    console.log(`- USER: ${user}`);
    console.log(`- PASS: ${pass ? '****(Đã cấp)' : '(Trống)'}`);

    const client = new ImapFlow({
        host,
        port,
        secure: port === 993,
        auth: {
            user,
            pass
        },
        logger: false 
    });

    try {
        console.log("Đang thử kết nối IMAP...");
        await client.connect();
        console.log("✅ Kết nối IMAP thành công!");
        await client.logout();
    } catch (err: any) {
        console.error("❌ Lỗi cấu hình IMAP:", err.message);
        console.log("--> Hãy chắc chắn bạn đã điền đúng email và App Password trong .env");
    }

    console.log("\n-----------------------------------------");
    console.log("Kiểm tra thông qua cấu trúc kỹ năng của Agent:");
    const result = await readEmailsExecute({ limit: 2 });
    console.log(result);
}

testConnection();
