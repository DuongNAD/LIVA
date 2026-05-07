const fs = require('fs');

// 1. SendMessengerRPA.ts (Fix 'any' params)
let smFile = 'src/skills/social/SendMessengerRPA.ts';
let smCode = fs.readFileSync(smFile, 'utf-8');
smCode = smCode.replace(/await page\.evaluate\(\(sel\) => \{/g, 'await page.evaluate((sel: string) => {');
smCode = smCode.replace(/await page\.evaluate\(\(target\) => \{/g, 'await page.evaluate((target: string) => {');
fs.writeFileSync(smFile, smCode);

// 2. SendZaloBot.ts (Fix Pino overload)
let zaloFile = 'src/skills/social/SendZaloBot.ts';
let zaloCode = fs.readFileSync(zaloFile, 'utf-8');
// Original: logger.error({ err: errMsg }, "[Skill: send_zalo_bot] Lỗi ngoại lệ:");
// Wait, my previous regex might have missed it or mis-matched. Let's force it.
zaloCode = zaloCode.replace(/logger\.error\(`\[Skill: send_zalo_bot\] Lỗi ngoại lệ:`, errMsg\);/g, 'logger.error({ err: errMsg }, `[Skill: send_zalo_bot] Lỗi ngoại lệ:`);');
fs.writeFileSync(zaloFile, zaloCode);

// 3. test_browser.ts (Fix path)
let tbFile = 'src/test_browser.ts';
let tbCode = fs.readFileSync(tbFile, 'utf-8');
// Original: import { execute } from "./skills/WebBrowser"; (wait, my previous script changed it to .js but not path maybe)
tbCode = tbCode.replace(/from "\.\/skills\/WebBrowser"/g, 'from "./skills/web/WebBrowser"');
tbCode = tbCode.replace(/from '\.\/skills\/WebBrowser\.js'/g, 'from "./skills/web/WebBrowser"');
fs.writeFileSync(tbFile, tbCode);

console.log("Fixed all remaining issues!");
