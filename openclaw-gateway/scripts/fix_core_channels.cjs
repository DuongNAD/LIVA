const fs = require('fs');

function fixTelegramBridge() {
    let content = fs.readFileSync('src/channels/TelegramBridge.ts', 'utf8');
    content = content.replace(/this\.#bot\.use/g, 'this.#bot!.use');
    content = content.replace(/this\.#bot\.on/g, 'this.#bot!.on');
    content = content.replace(/this\.#bot\.api/g, 'this.#bot!.api'); // Just in case
    content = content.replace(/this\.#bot\.start/g, 'this.#bot!.start');
    content = content.replace(/this\.#bot\.stop/g, 'this.#bot!.stop');
    fs.writeFileSync('src/channels/TelegramBridge.ts', content, 'utf8');
}

function fixPromptBuilder() {
    let content = fs.readFileSync('src/core/PromptBuilder.ts', 'utf8');
    content = content.replace(/memory\.getLanceMemory\(\)/g, 'memory.lanceMemory');
    
    // anchors -> safeAnchors
    if (!content.includes('const safeAnchors')) {
        content = content.replace(/if\s*\(anchors\s*&&\s*anchors\.length\s*>\s*0\)\s*\{/, 'const safeAnchors = (anchors as string[]) || [];\n            if (safeAnchors && safeAnchors.length > 0) {');
        content = content.replace(/anchors\.forEach/g, 'safeAnchors.forEach');
    }
    fs.writeFileSync('src/core/PromptBuilder.ts', content, 'utf8');
}

function fixCoreKernel() {
    let content = fs.readFileSync('src/core/CoreKernel.ts', 'utf8');
    
    // Boolean param: action === "approve"
    // The user says "Dòng ~296: Lỗi kiểu dữ liệu boolean. Đổi tham số thứ hai thành action === "approve"."
    // Let's do a regex that replaces something like `(..., action, ...)` with `(..., action === "approve", ...)` ? It's risky. I'd better view the file first.
    
    // Line 805: this.agentLoop.stop()
    if (!content.includes('// @ts-ignore\n        this.agentLoop.stop()') && !content.includes('// @ts-ignore\\s*this.agentLoop.stop()')) {
        content = content.replace(/this\.agentLoop\.stop\(\)/, '// @ts-ignore\n        this.agentLoop.stop()');
    }
    fs.writeFileSync('src/core/CoreKernel.ts', content, 'utf8');
}

function fixTaskLaneWorker() {
    let content = fs.readFileSync('src/core/orchestrators/TaskLaneWorker.ts', 'utf8');
    content = content.replace(/\.catch\(\s*e\s*=>/g, '.catch((e: unknown) =>');
    content = content.replace(/async\s+processQueue\(\)/, 'async processQueue(): Promise<void>');
    fs.writeFileSync('src/core/orchestrators/TaskLaneWorker.ts', content, 'utf8');
}

function fixASTGraphBuilder() {
    let content = fs.readFileSync('src/evolution/ASTGraphBuilder.ts', 'utf8');
    content = content.replace(/import\s+\{\s*lang\s*\}/, 'import { Lang }');
    // or maybe the module name? "đổi chữ lang thành Lang (viết hoa chữ L)"
    // Let's replace 'lang' with 'Lang' on line 1.
    let lines = content.split('\n');
    lines[0] = lines[0].replace(/lang/, 'Lang');
    fs.writeFileSync('src/evolution/ASTGraphBuilder.ts', lines.join('\n'), 'utf8');
}

function fixBookIndex() {
    let content = fs.readFileSync('src/memory/BookIndex.ts', 'utf8');
    content = content.replace(/new Graph\(\{\s*directed:\s*true\s*\}\)/g, 'new Graph()');
    fs.writeFileSync('src/memory/BookIndex.ts', content, 'utf8');
}

fixTelegramBridge();
fixPromptBuilder();
// fixCoreKernel(); // will view_file first
fixTaskLaneWorker();
fixASTGraphBuilder();
fixBookIndex();
console.log('Fixed multiple files');
