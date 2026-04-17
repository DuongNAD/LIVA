import * as dotenv from "dotenv";
import { CoreKernel } from "./core/CoreKernel";
import { logger } from "./utils/logger";

dotenv.config();

async function start() {
  try {
    const kernel = new CoreKernel();
    await kernel.fetchSystemLocation();
    await kernel.bootstrap();
  } catch (e: any) {
    logger.error("System Fatal Error:", e.stack || e);
  }
}

start();
