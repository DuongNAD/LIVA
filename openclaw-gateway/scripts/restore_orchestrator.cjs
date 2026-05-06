const fs = require('fs');
let content = fs.readFileSync('src/core/ModelOrchestrator.ts', 'utf8');

// Task 4: Fix readHardwareConfig to be async
content = content.replace(/function readHardwareConfig\(\): Promise<HardwareConfig>/, 'async function readHardwareConfig(): Promise<HardwareConfig>');
content = content.replace(/const fs = require\('fs'\);/, `const fs = require('node:fs/promises');`);
content = content.replace(/fs\.readFileSync\([^,]+,\s*"utf-8"\)/g, 'await fs.readFile(statePath, "utf-8")');

// Task 3: Replace catch (e: any) with catch (e: unknown) and type narrow
// First, replace all catch (e: any) with catch (e: unknown) { const errMsg = e instanceof Error ? e.message : String(e);
content = content.replace(/catch\s*\(\s*e\s*:\s*any\s*\)\s*\{/g, `catch (e: unknown) {\n          const errMsg = e instanceof Error ? e.message : String(e);`);
// Also replace e.message with errMsg in those blocks
content = content.replace(/e\.message/g, 'errMsg');

fs.writeFileSync('src/core/ModelOrchestrator.ts', content, 'utf8');
console.log('Restored ModelOrchestrator.ts fixes');
