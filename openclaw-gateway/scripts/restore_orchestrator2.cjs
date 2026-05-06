const fs = require('fs');

let content = fs.readFileSync('src/core/ModelOrchestrator.ts', 'utf8');

// MED-2: readHardwareConfig
content = content.replace(/function readHardwareConfig\(\): Promise<HardwareConfig>/, 'async function readHardwareConfig(): Promise<HardwareConfig>');
content = content.replace(/const fs = require\('fs'\);/, `const fs = require('node:fs/promises');`);
content = content.replace(/fs\.readFileSync\(statePath, "utf-8"\)/g, 'await fs.readFile(statePath, "utf-8")');

// HIGH-4, HIGH-5: catch (e: any)
// 1. Line 186: catch (e: any) => fetch
content = content.replace(/catch \(e: any\) {\s+this\.failedPings\+\+;/, "catch (e: unknown) {\n                  const errMsg = e instanceof Error && 'cause' in e && e.cause instanceof Error ? e.cause.message : e instanceof Error ? e.message : String(e);\n                  this.failedPings++;");

// 2. Line 214: catch (e: any) { logger.error(❌ [RollbackManager] Phục hồi thất bại: ${e.message}); }
content = content.replace(/catch \(e: any\) {\s+logger\.error\(`❌ \[RollbackManager\] Phục hồi thất bại: \$\{e\.message\}`\);/, "catch (e: unknown) {\n          const errMsg = e instanceof Error ? e.message : String(e);\n          logger.error(`❌ [RollbackManager] Phục hồi thất bại: ${errMsg}`);");

// 3. Line 301 (startRouter healthcheck fetch):
content = content.replace(/catch \(e: any\) {\s+logger\.debug\("Router C\+\+ health check ping, retrying: " \+ e\.message\);/, "catch (e: unknown) {\n          const errMsg = e instanceof Error && 'cause' in e && e.cause instanceof Error ? e.cause.message : e instanceof Error ? e.message : String(e);\n          logger.debug(\"Router C++ health check ping, retrying: \" + errMsg);");

// 4. Line 413 (startExpert healthcheck fetch):
content = content.replace(/catch \(e: any\) {\s+logger\.debug\("Expert health check ping fail, retrying: " \+ e\.message\);/, "catch (e: unknown) {\n          const errMsg = e instanceof Error && 'cause' in e && e.cause instanceof Error ? e.cause.message : e instanceof Error ? e.message : String(e);\n          logger.debug(\"Expert health check ping fail, retrying: \" + errMsg);");

fs.writeFileSync('src/core/ModelOrchestrator.ts', content, 'utf8');
console.log('Successfully restored ModelOrchestrator.ts completely');
