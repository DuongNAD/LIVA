const fs = require('fs');

function refactorAuth() {
    let content = fs.readFileSync('openclaw-gateway/src/utils/auth_google_script.ts', 'utf8');
    
    // Add promises import if not there
    if (!content.includes('promises as fsp')) {
        content = content.replace('import * as fs from "node:fs";', 'import * as fs from "node:fs";\nimport { promises as fsp } from "node:fs";');
    }
    
    // Replace existsSync
    content = content.replace('if (!fs.existsSync(CREDENTIALS_PATH)) {', 
        `let hasCreds = true;
  try { await fsp.access(CREDENTIALS_PATH); } catch { hasCreds = false; }
  if (!hasCreds) {`);
  
    // Replace readFileSync
    content = content.replace('fs.readFileSync(CREDENTIALS_PATH, "utf8")', 'await fsp.readFile(CREDENTIALS_PATH, "utf8")');
    
    // Replace writeFileSync with Atomic Write
    const atomicWrite = `const tmpPath = TOKEN_PATH + '.tmp';
            await fsp.writeFile(tmpPath, JSON.stringify(tokens));
            await fsp.rename(tmpPath, TOKEN_PATH);`;
    
    content = content.replace(/fs\.writeFileSync\(TOKEN_PATH,\s*JSON\.stringify\(tokens\)\);/g, atomicWrite);
    
    fs.writeFileSync('openclaw-gateway/src/utils/auth_google_script.ts', content);
    console.log('Refactored auth_google_script.ts');
}

function refactorSkeleton() {
    let content = fs.readFileSync('openclaw-gateway/src/utils/generate_skeleton.ts', 'utf8');
    
    if (!content.includes('promises as fsp')) {
        content = content.replace('import * as fs from "node:fs";', 'import * as fs from "node:fs";\nimport { promises as fsp } from "node:fs";');
    }
    
    // Fix existing cache loading
    const cacheLoad = `
try {
    if (fs.existsSync(cacheFile)) {
        cache = JSON.parse(fs.readFileSync(cacheFile, "utf-8"));
    }
} catch {
    logger.warn("[AST Cache] Cache corrupted. Rebuilding from scratch.");
}`;
    const asyncCacheLoad = `
try {
    try { await fsp.access(cacheFile); } catch { throw new Error('No cache'); }
    cache = JSON.parse(await fsp.readFile(cacheFile, "utf-8"));
} catch {
    logger.warn("[AST Cache] Cache corrupted. Rebuilding from scratch.");
}`;
    content = content.replace(cacheLoad, asyncCacheLoad);
    
    // export function extractSkeleton -> export async function extractSkeleton
    content = content.replace('export function extractSkeleton', 'export async function extractSkeleton');
    
    // fs.statSync -> await fsp.stat
    content = content.replace('fs.statSync(filePath)', 'await fsp.stat(filePath)');
    
    // fs.readFileSync -> await fsp.readFile
    content = content.replace('fs.readFileSync(filePath, "utf-8")', 'await fsp.readFile(filePath, "utf-8")');
    
    // function scan -> async function scan
    content = content.replace('function scan(dir: string)', 'async function scan(dir: string)');
    
    // fs.readdirSync -> await fsp.readdir
    content = content.replace('fs.readdirSync(dir)', 'await fsp.readdir(dir)');
    
    // fs.statSync in scan -> await fsp.stat
    content = content.replace('fs.statSync(full).isDirectory()', '(await fsp.stat(full)).isDirectory()');
    
    // scan(full) -> await scan(full)
    content = content.replace('scan(full);', 'await scan(full);');
    
    // extractSkeleton(full) -> await extractSkeleton(full)
    content = content.replace('extractSkeleton(full);', 'await extractSkeleton(full);');
    
    // Top-level async wrap
    const topLevelExec = `logger.info("[AST Skeleton] Bắt đầu gọt dũa hệ thống...");
const startTime = performance.now();

scan(targetDir);

// Save generated map and cache
fs.writeFileSync(mapFile, resultStr, "utf-8");
fs.writeFileSync(cacheFile, JSON.stringify(cache, null, 2), "utf-8");

const endTime = performance.now();
logger.info(\`[AST Skeleton] Hoành thành! Tổng cộng: \${processedFiles} files. (Cache hits: \${cacheHits})\`);
logger.info(\`[AST Skeleton] Kích cỡ MAP: \${Math.round(resultStr.length / 1024)}KB (~\${Math.floor(resultStr.length / 3.8)} Tokens).\`);
logger.info(\`[AST Skeleton] Tốc độ xử lý: \${(endTime - startTime).toFixed(2)}ms\`);`;

    const asyncTopLevelExec = `(async () => {
logger.info("[AST Skeleton] Bắt đầu gọt dũa hệ thống...");
const startTime = performance.now();

await scan(targetDir);

// Save generated map and cache (Atomic Write)
await fsp.writeFile(mapFile + '.tmp', resultStr, "utf-8");
await fsp.rename(mapFile + '.tmp', mapFile);

await fsp.writeFile(cacheFile + '.tmp', JSON.stringify(cache, null, 2), "utf-8");
await fsp.rename(cacheFile + '.tmp', cacheFile);

const endTime = performance.now();
logger.info(\`[AST Skeleton] Hoành thành! Tổng cộng: \${processedFiles} files. (Cache hits: \${cacheHits})\`);
logger.info(\`[AST Skeleton] Kích cỡ MAP: \${Math.round(resultStr.length / 1024)}KB (~\${Math.floor(resultStr.length / 3.8)} Tokens).\`);
logger.info(\`[AST Skeleton] Tốc độ xử lý: \${(endTime - startTime).toFixed(2)}ms\`);
})();`;
    content = content.replace(topLevelExec, asyncTopLevelExec);
    
    fs.writeFileSync('openclaw-gateway/src/utils/generate_skeleton.ts', content);
    console.log('Refactored generate_skeleton.ts');
}

refactorAuth();
refactorSkeleton();
