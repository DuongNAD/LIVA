import * as fs from "node:fs";
import * as path from "node:path";
import { google } from "googleapis";
import * as http from "node:http";
import { URL } from "url";

const SCOPES = [
  "https://www.googleapis.com/auth/documents",
  "https://www.googleapis.com/auth/spreadsheets",
  "https://www.googleapis.com/auth/drive",
];

const CREDENTIALS_PATH = path.join(process.cwd(), "credentials.json");
const TOKEN_PATH = path.join(process.cwd(), "token.json");

async function authorize() {
  if (!fs.existsSync(CREDENTIALS_PATH)) {
    console.error(
      `🚨 Không tìm thấy file credentials.json tại ${CREDENTIALS_PATH}`,
    );
    console.log(
      `Vui lòng tải file OAuth2 Client từ Google Cloud Console và đổi tên thành credentials.json`,
    );
    process.exit(1);
  }

  const content = fs.readFileSync(CREDENTIALS_PATH, "utf8");
  const credentials = JSON.parse(content);

  if (credentials.type === "service_account") {
    console.log(
      `✅ Bạn đang dùng Service Account, không cần chạy script này để lấy token. LIVA đã có thể hoạt động ngay.`,
    );
    process.exit(0);
  }

  const { client_secret, client_id, redirect_uris } =
    credentials.installed || credentials.web;
  const oAuth2Client = new google.auth.OAuth2(
    client_id,
    client_secret,
    redirect_uris[0] || "http://localhost:3000",
  );

  console.log("🔗 Đang tạo URL xác thực...");
  const authUrl = oAuth2Client.generateAuthUrl({
    access_type: "offline",
    prompt: "consent",
    scope: SCOPES,
  });

  console.log("\n======================================================");
  console.log("👉 BƯỚC 1: Hãy mở địa chỉ sau trên trình duyệt để cấp quyền:");
  console.log(authUrl);
  console.log("======================================================\n");

  // Nếu dùng localhost redirect
  if (
    redirect_uris &&
    redirect_uris.some((r: string) => r.includes("localhost"))
  ) {
    const portObj = new URL(
      redirect_uris.find((r: string) => r.includes("localhost")),
    );
    const port = portObj.port || 3000;
    console.log(`👂 Đang lắng nghe phản hồi tại cổng ${port}...`);

    const server = http.createServer(async (req, res) => {
      try {
        if (req.url && req.url.indexOf("code=") > -1) {
          const qs = new URL(req.url, "http://localhost:3000").searchParams;
          const code = qs.get("code");
          console.log(
            "✅ BƯỚC 2: Đã nhận được mã xác thực (code). Đang lấy Token...",
          );
          res.end(
            "<h1>LIVA Authentication Successful!</h1><p>You can close this tab and return to the terminal.</p>",
          );
          server.close();

          if (code) {
            const { tokens } = await oAuth2Client.getToken(code);
            oAuth2Client.setCredentials(tokens);
            fs.writeFileSync(TOKEN_PATH, JSON.stringify(tokens));
            console.log(
              `🎉 Xác thực thành công! Đã lưu token tại ${TOKEN_PATH}`,
            );
            process.exit(0);
          }
        }
      } catch (err) {
        console.error("Lỗi khi lấy token:", err);
        res.end("<h1>LIVA Authentication Flow Failed!</h1>");
        server.close();
        process.exit(1);
      }
    });
    server.listen(port);
  } else {
    console.log(
      `👉 BƯỚC 2: (Do không dùng localhost redirect) Sau khi đăng nhập, copy cái 'code' vào đây.`,
    );
    const readline = require("readline").createInterface({
      input: process.stdin,
      output: process.stdout,
    });
    readline.question("Nhập code: ", async (code: string) => {
      try {
        const { tokens } = await oAuth2Client.getToken(code);
        oAuth2Client.setCredentials(tokens);
        fs.writeFileSync(TOKEN_PATH, JSON.stringify(tokens));
        console.log(`🎉 Xác thực thành công! Đã lưu token tại ${TOKEN_PATH}`);
      } catch (err) {
        console.error("Lỗi khi lấy token:", err);
      }
      readline.close();
    });
  }
}

authorize();
