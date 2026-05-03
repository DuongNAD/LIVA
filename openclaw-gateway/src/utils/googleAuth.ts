import * as fs from "node:fs";
import { promises as fsp } from "node:fs";
import * as path from "node:path";
import { google } from "googleapis";
import { logger } from "./logger";

// If modifying these scopes, delete token.json.
const SCOPES = [
  "https://www.googleapis.com/auth/documents",
  "https://www.googleapis.com/auth/spreadsheets",
  "https://www.googleapis.com/auth/drive",
];

const CREDENTIALS_PATH = path.join(process.cwd(), "credentials.json");
const TOKEN_PATH = path.join(process.cwd(), "token.json");

/**
 * Lấy đối tượng auth client để gọi Google API
 * Hỗ trợ cả Desktop OAuth2 (credentials.json) VÀ Service Account
 */
export async function getGoogleAuthClient() {
  if (!fs.existsSync(CREDENTIALS_PATH)) {
    throw new Error(
      `[Google Auth] Không tìm thấy file credentials.json tại ${CREDENTIALS_PATH}. Vui lòng tạo trên Google Cloud Console.`,
    );
  }

  // 🔒 [Audit H-3] Async I/O — avoid blocking event loop
  const credentials = JSON.parse(await fsp.readFile(CREDENTIALS_PATH, "utf8"));

  // Nếu là file Service Account (chứa type="service_account")
  if (credentials.type === "service_account") {
    logger.info("[Google Auth] Đang sử dụng Service Account.");
    const auth = new google.auth.GoogleAuth({
      keyFile: CREDENTIALS_PATH,
      scopes: SCOPES,
    });
    return await auth.getClient();
  }

  // Nếu là file OAuth2 Desktop Client
  const { client_secret, client_id, redirect_uris } =
    credentials.installed || credentials.web;
  if (!client_secret || !client_id) {
    throw new Error("[Google Auth] File credentials.json không hợp lệ.");
  }

  const oAuth2Client = new google.auth.OAuth2(
    client_id,
    client_secret,
    redirect_uris[0],
  );

  // Check if we have previously stored a token.
  if (!fs.existsSync(TOKEN_PATH)) {
    throw new Error(
      `[Google Auth] Chưa được cấp quyền OAuth2 (Không tìm thấy token.json). Vui lòng chạy lệnh: npx tsx src/utils/auth_google_script.ts để đăng nhập lần đầu.`,
    );
  }

  // 🔒 [Audit H-3] Async I/O
  const token = JSON.parse(await fsp.readFile(TOKEN_PATH, "utf8"));
  oAuth2Client.setCredentials(token);

  // Auto-refresh token if needed
  oAuth2Client.on("tokens", (tokens) => {
    if (tokens.refresh_token) {
      // Modify token locally and save
      token.refresh_token = tokens.refresh_token;
    }
    token.access_token = tokens.access_token;
    // 🔒 [Audit H-3] Atomic Write: .tmp + rename to prevent token corruption
    const tmpPath = `${TOKEN_PATH}.tmp`;
    fsp.writeFile(tmpPath, JSON.stringify(token), "utf-8")
      .then(() => fsp.rename(tmpPath, TOKEN_PATH))
      .catch(e => logger.error(e, "[Google Auth] Atomic token save failed"));
    logger.info("[Google Auth] Đã làm mới token thành công.");
  });

  return oAuth2Client;
}
