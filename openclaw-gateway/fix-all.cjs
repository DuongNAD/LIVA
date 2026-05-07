const fs = require('fs');

// 1. ComputerUse.ts
let cuPath = 'src/skills/web/ComputerUse.ts';
let cuCode = fs.readFileSync(cuPath, 'utf-8');
cuCode = cuCode.replace(
    /import \* as fs from "node:fs";/g,
    'import * as fs from "node:fs";\nimport { Page } from "playwright";'
);
cuCode = cuCode.replace(
    /fs\.mkdir\(/g,
    'fs.mkdirSync('
);
fs.writeFileSync(cuPath, cuCode);

// 2. test_browser.ts
let tbPath = 'src/test_browser.ts';
let tbCode = fs.readFileSync(tbPath, 'utf-8');
tbCode = tbCode.replace(
    /from '\.\/skills\/WebBrowser'/g,
    "from './skills/web/WebBrowser.js'"
);
fs.writeFileSync(tbPath, tbCode);

// 3. SendMessengerRPA.ts
let mrpaPath = 'src/skills/social/SendMessengerRPA.ts';
let mrpaCode = fs.readFileSync(mrpaPath, 'utf-8');
if (!mrpaCode.includes('import { logger }')) {
    mrpaCode = mrpaCode.replace(
        /import \{ Page \} from "playwright";/g,
        'import { Page } from "playwright";\nimport { logger } from "@utils/logger";'
    );
    if (!mrpaCode.includes('import { logger }')) {
        mrpaCode = 'import { logger } from "@utils/logger";\n' + mrpaCode;
    }
}
fs.writeFileSync(mrpaPath, mrpaCode);

// 4. SendZaloBot.ts
let zaloPath = 'src/skills/social/SendZaloBot.ts';
let zaloCode = fs.readFileSync(zaloPath, 'utf-8');
zaloCode = zaloCode.replace(
    /logger\.error\("\[Skill: send_zalo_bot\] Lỗi ngoại lệ:", errMsg\);/g,
    'logger.error({ err: errMsg }, "[Skill: send_zalo_bot] Lỗi ngoại lệ:");'
);
fs.writeFileSync(zaloPath, zaloCode);

console.log("All residual errors fixed!");
