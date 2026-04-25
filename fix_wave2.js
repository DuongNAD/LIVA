/**
 * Wave 2: Fix the remaining high-volume auto-fixable rules
 *
 * S7772 (52x) — Prefer node: prefix imports
 * S7781 (50x) — Prefer String#replaceAll() over String#replace()
 * S7764 (32x) — Prefer globalThis over window/global
 * S3735 (28x) — Remove void operator
 * S6582 (24x) — Optional chaining
 * S6594 (13x) — indexOf → includes
 */
const fs = require('fs');
const path = require('path');
const data = require('./sonar_all.json');
let fixedCount = 0;

const byFile = {};
for (const issue of data.issues) {
    if (!issue.component || !issue.textRange) continue;
    const fp = path.join(__dirname, issue.component.replace('Liva:', ''));
    if (!byFile[fp]) byFile[fp] = [];
    byFile[fp].push(issue);
}

for (const [filePath, issues] of Object.entries(byFile)) {
    if (!fs.existsSync(filePath)) continue;
    let lines = fs.readFileSync(filePath, 'utf-8').split('\n');
    let modified = false;
    
    issues.sort((a, b) => b.textRange.startLine - a.textRange.startLine);
    
    for (const issue of issues) {
        const li = issue.textRange.startLine - 1;
        if (li < 0 || li >= lines.length) continue;
        let line = lines[li];
        const orig = line;
        
        // S7772: node: prefix
        if (issue.rule === 'typescript:S7772' && issue.message.startsWith('Prefer `node:')) {
            // Extract the module name from "Prefer `node:fs/promises` over `fs/promises`."
            const match = issue.message.match(/Prefer `node:([^`]+)` over `([^`]+)`/);
            if (match) {
                const nodeModule = match[1]; // fs/promises
                const bareModule = match[2]; // fs/promises
                // Replace in import/require
                line = line.replace(new RegExp(`from\\s+['"]${bareModule.replace(/\//g, '\\/')}['"]`), `from 'node:${nodeModule}'`);
                line = line.replace(new RegExp(`require\\(['"]${bareModule.replace(/\//g, '\\/')}['"]\\)`), `require('node:${nodeModule}')`);
            }
        }
        
        // S7781: String#replaceAll
        if (issue.rule === 'typescript:S7781' && issue.message.includes('replaceAll')) {
            // .replace("str", ...) → .replaceAll("str", ...)
            // Only for literal string first arg (not regex)
            if (line.match(/\.replace\(\s*["'`]/)) {
                line = line.replace(/\.replace\(/, '.replaceAll(');
            }
        }
        
        // S7764: globalThis
        if (issue.rule === 'typescript:S7764') {
            if (issue.message.includes('over `window`')) {
                // Don't replace window.xxx with globalThisxxx - need word boundary
                line = line.replace(/\bwindow\b/g, 'globalThis');
            } else if (issue.message.includes('over `global`') && !issue.message.includes('globalThis')) {
                line = line.replace(/\bglobal\b(?!This)/g, 'globalThis');
            }
        }

        // S3735: Remove void operator
        if (issue.rule === 'typescript:S3735') {
            // void expr → expr (skip void 0)
            if (line.match(/\bvoid\s+(?!0\b)/) && !line.match(/catch/)) {
                line = line.replace(/\bvoid\s+/, '');
            }
        }

        // S6594: indexOf → includes  
        if (issue.rule === 'typescript:S6594') {
            line = line.replace(/\.indexOf\(([^)]+)\)\s*!==\s*-1/g, '.includes($1)');
            line = line.replace(/\.indexOf\(([^)]+)\)\s*>=\s*0/g, '.includes($1)');
            line = line.replace(/\.indexOf\(([^)]+)\)\s*>\s*-1/g, '.includes($1)');
            // Negative check
            if (line.match(/\.indexOf\([^)]+\)\s*===\s*-1/)) {
                line = line.replace(/(\S+)\.indexOf\(([^)]+)\)\s*===\s*-1/g, '!$1.includes($2)');
            }
        }
        
        if (line !== orig) {
            lines[li] = line;
            modified = true;
            fixedCount++;
        }
    }
    
    if (modified) fs.writeFileSync(filePath, lines.join('\n'));
}

console.log(`Wave 2 fixed: ${fixedCount} issues`);
