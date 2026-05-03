const fs = require('fs');
let code = fs.readFileSync('src/MemoryManager.ts', 'utf-8');

code = code.replace(
    'import { logger } from "./utils/logger";',
    'import { logger } from "./utils/logger";\nimport { LanceMemoryManager } from "./memory/LanceMemory";\nimport { ConsolidationCron } from "./memory/ConsolidationCron";\nimport { BookIndex } from "./memory/BookIndex";\nimport type OpenAI from "openai";'
);

code = code.replace(
    'private memCache: ChatMessage[] = []; // In-memory Cache',
    'private memCache: ChatMessage[] = []; // In-memory Cache\n  public lanceMemory?: LanceMemoryManager;\n  public bookIndex?: BookIndex;\n  public consolidationCron?: ConsolidationCron;'
);

code = code.replace(
    '// [Z-MAS RAM Healer] Dọn dẹp tài nguyên ngầm khi shutdown',
    'public async initUHM(aiClient: OpenAI): Promise<void> {\n      try {\n          this.lanceMemory = new LanceMemoryManager("liv_async_core");\n          await this.lanceMemory.initialize();\n          this.bookIndex = new BookIndex();\n          this.consolidationCron = new ConsolidationCron(this.structuredMemory, this.lanceMemory, this.bookIndex, aiClient);\n          await this.consolidationCron.preflightCheck();\n          this.consolidationCron.start();\n          logger.info("[Memory] UHM with RAPTOR initialized.");\n      } catch (e: any) {\n          logger.error(`[Memory] initUHM failed: ${e.message}`);\n          throw e;\n      }\n  }\n\n  // [Z-MAS RAM Healer] Dọn dẹp tài nguyên ngầm khi shutdown'
);

code = code.replace(
    'public dispose() {',
    'public async dispose() {\n      if (this.lanceMemory) await this.lanceMemory.dispose();\n      if (this.consolidationCron) this.consolidationCron.dispose();'
);

fs.writeFileSync('src/MemoryManager.ts', code);
console.log('Done');
