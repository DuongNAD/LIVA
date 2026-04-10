import * as fs from "fs";
import * as path from "path";

class Logger {
  private logFile: string;

  constructor() {
    const logDir = path.resolve(process.cwd(), "logs");
    if (!fs.existsSync(logDir)) {
      fs.mkdirSync(logDir, { recursive: true });
    }
    this.logFile = path.join(logDir, "ai_debug.log");
    this.info("=== [SYSTEM START] Logger Initialized ===");
  }

  private write(level: string, message: string, meta?: any) {
    const timestamp = new Date().toISOString();
    let logLine = `[${timestamp}] [${level}] ${message}`;

    if (meta !== undefined) {
      logLine += `\n${typeof meta === "object" ? JSON.stringify(meta, null, 2) : String(meta)}`;
    }

    // Output to console
    console.log(logLine);

    // Output to file
    fs.appendFileSync(this.logFile, logLine + "\n\n", "utf8");
  }

  public info(message: string, meta?: any) {
    this.write("INFO", message, meta);
  }

  public debug(message: string, meta?: any) {
    this.write("DEBUG", message, meta);
  }

  public warn(message: string, meta?: any) {
    this.write("WARN", message, meta);
  }

  public error(message: string, meta?: any) {
    this.write("ERROR", message, meta);
  }
}

export const logger = new Logger();
