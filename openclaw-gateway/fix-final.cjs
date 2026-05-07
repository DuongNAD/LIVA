const fs = require('fs');

// 1. DocumentWriterBase.ts
let dwFile = 'src/skills/docs/DocumentWriterBase.ts';
let dwCode = fs.readFileSync(dwFile, 'utf-8');
dwCode = dwCode.replace(/if \(err\.code !== "EEXIST"\) throw err;/g, 'if ((err as any).code !== "EEXIST") throw err;');
dwCode = dwCode.replace(/logger\.error\(`Error generating \$\{part\.name\}:`, errMsg\);/g, 'logger.error({ err: errMsg }, `Error generating ${part.name}:`);');
fs.writeFileSync(dwFile, dwCode);

// 2. SendMessengerRPA.ts
let smFile = 'src/skills/social/SendMessengerRPA.ts';
let smCode = fs.readFileSync(smFile, 'utf-8');
if (!smCode.includes('import { BrowserContext, Page }')) {
    smCode = 'import { BrowserContext, Page } from "playwright";\nimport { getOrCreateBrowser } from "@utils/PlaywrightBrowser";\n' + smCode;
}
fs.writeFileSync(smFile, smCode);

console.log("Fixed final types!");
