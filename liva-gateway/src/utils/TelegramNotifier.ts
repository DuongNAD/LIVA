import { TelegramManager } from "../services/TelegramManager";
import { logger } from "./logger";

export async function notifyTelegram(msg: string) {
  try {
    const manager = new TelegramManager();
    await manager.sendMessage(msg);
  } catch (e: unknown) {
    const errMsg = e instanceof Error ? e.message : String(e);
    logger.error(`[TelegramNotifier] Nhắn Telegram thất bại: ${errMsg}`);
  }
}
