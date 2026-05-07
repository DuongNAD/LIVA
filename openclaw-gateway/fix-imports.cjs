const fs = require('fs');

let file = 'src/skills/web/ComputerUse.ts';
let code = fs.readFileSync(file, 'utf-8');

code = code.replace(
    /import \{ RPAGuardrails \} from "@security\/RPAGuardrails";/g,
    `import { RPAGuardrails } from "@security/RPAGuardrails";
import { getOrCreateBrowser, getActivePage } from "@utils/PlaywrightBrowser";
import { logger } from "@utils/logger";
import * as fs from "node:fs";`
);

// Also fix `fs.existsSync` in ComputerUse to `fsp.access` because we migrated away from sync, but let's just import fs for now.
// Or actually I can fix `d` parameter error in ComputerUse.ts
// `src/skills/web/ComputerUse.ts(195,38): error TS7006: Parameter 'd' implicitly has an 'any' type.`
code = code.replace(/\(d\) =>/g, '(d: any) =>');

// Also remove the existing getOrCreateBrowser if it's there
code = code.replace(/import \{ getOrCreateBrowser \} from "@utils\/PlaywrightBrowser";\n/g, "");

fs.writeFileSync(file, code);
console.log("Fixed ComputerUse.ts imports!");
