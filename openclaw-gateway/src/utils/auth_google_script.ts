import * as fs from "node:fs/promises";
import * as path from "node:path";
import { google } from "googleapis";
import * as http from "node:http";
import { URL } from "node:url";
import { logger } from "./logger";

const SCOPES = [
  "https://www.googleapis.com/auth/documents",
  "https://www.googleapis.com/auth/spreadsheets",
  "https://www.googleapis.com/auth/drive",
];

const CREDENTIALS_PATH = path.join(process.cwd(), "credentials.json");
const TOKEN_PATH = path.join(process.cwd(), "token.json");

/**
 * Atomic write pattern - prevents corruption on crash/interrupt.
 * Writes to .tmp file first, then renames atomically.
 */
async function atomicWriteFile(filePath: string, content: string): Promise<void> {
  const tmpPath = `${filePath}.tmp`;
  await fs.writeFile(tmpPath, content, "utf8");
  await fs.rename(tmpPath, filePath);
}

async function authorize(): Promise<void> {
  try {
    await fs.access(CREDENTIALS_PATH);
  } catch {
    logger.error(
      { context: "auth_google_script", path: CREDENTIALS_PATH },
      `🚨 Không tìm thấy file credentials.json`
    );
    logger.info(
      { context: "auth_google_script" },
      `Vui lòng tải file OAuth2 Client từ Google Cloud Console và đổi tên thành credentials.json`
    );
    process.exit(1);
  }

  const content = await fs.readFile(CREDENTIALS_PATH, "utf8");
  const credentials = JSON.parse(content);

  if (credentials.type === "service_account") {
    logger.info(
      { context: "auth_google_script" },
      `✅ Bạn đang dùng Service Account, không cần chạy script này để lấy token. LIVA đã có thể hoạt động ngay.`
    );
    process.exit(0);
  }

  const { client_secret, client_id, redirect_uris } =
    credentials.installed || credentials.web;
  const oAuth2Client = new google.auth.OAuth2(
    client_id,
    client_secret,
    redirect_uris[0] || "http://localhost:3000"
  );

  logger.info({ context: "auth_google_script" }, `🔗 Đang tạo URL xác thực...`);
  const authUrl = oAuth2Client.generateAuthUrl({
    access_type: "offline",
    prompt: "consent",
    scope: SCOPES,
  });

  logger.info({ context: "auth_google_script" }, `\n======================================================`);
  logger.info({ context: "auth_google_script" }, `👉 BƯỚC 1: Hãy mở địa chỉ sau trên trình duyệt để cấp quyền:`);
  logger.info({ context: "auth_google_script" }, `${authUrl}`);
  logger.info({ context: "auth_google_script" }, `======================================================\n`);

  if (
    redirect_uris &&
    redirect_uris.some((r: string) => r.includes("localhost"))
  ) {
    const portObj = new URL(
      redirect_uris.find((r: string) => r.includes("localhost"))
    );
    const port = portObj.port || 3000;
    logger.info(
      { context: "auth_google_script", port },
      `👂 Đang lắng nghe phản hồi tại cổng ${port}...`
    );

    const server = http.createServer(async (req, res) => {
      try {
        if (req.url && req.url.indexOf("code=") > -1) {
          const qs = new URL(req.url, "http://localhost:3000").searchParams;
          const code = qs.get("code");
          logger.info(
            { context: "auth_google_script" },
            `✅ BƯỚC 2: Đã nhận được mã xác thực (code). Đang lấy Token...`
          );
          res.end(
            "<h1>LIVA Authentication Successful!</h1><p>You can close this tab and return to the terminal.</p>"
          );
          server.close();

          if (code) {
            const { tokens } = await oAuth2Client.getToken(code);
            oAuth2Client.setCredentials(tokens);
            await atomicWriteFile(TOKEN_PATH, JSON.stringify(tokens));
            logger.info(
              { context: "auth_google_script", path: TOKEN_PATH },
              `🎉 Xác thực thành công! Đã lưu token tại ${TOKEN_PATH}`
            );
            process.exit(0);
          }
        }
      } catch (err) {
        const errMsg = err instanceof Error ? err.message : String(err);
        logger.error(
          { context: "auth_google_script", error: errMsg },
          `❌ Lỗi khi lấy token`
        );
        res.end("<h1>LIVA Authentication Flow Failed!</h1>");
        server.close();
        process.exit(1);
      }
    });
    server.listen(port);
  } else {
    logger.info(
      { context: "auth_google_script" },
      `👉 BƯỚC 2: (Do không dùng localhost redirect) Sau khi đăng nhập, copy cái 'code' vào đây.`
    );
    const readline = await import("node:readline");
    const rl = readline.createInterface({
      input: process.stdin,
      output: process.stdout,
    });

    rl.question("Nhập code: ", async (code: string) => {
      try {
        const { tokens } = await oAuth2Client.getToken(code);
        oAuth2Client.setCredentials(tokens);
        await atomicWriteFile(TOKEN_PATH, JSON.stringify(tokens));
        logger.info(
          { context: "auth_google_script", path: TOKEN_PATH },
          `🎉 Xác thực thành công! Đã lưu token tại ${TOKEN_PATH}`
        );
      } catch (err) {
        const errMsg = err instanceof Error ? err.message : String(err);
        logger.error(
          { context: "auth_google_script", error: errMsg },
          `❌ Lỗi khi lấy token`
        );
      }
      rl.close();
    });
  }
}

authorize();
