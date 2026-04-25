/**
 * MEGA FIX: Targets the top auto-fixable SonarQube rules
 * 
 * S2933 (96x) - Properties should be readonly → add 'readonly'
 * S2486 (67x) - Catch binding unused → remove binding 
 * S7781 (60x) - String.raw for escaped backslash → use String.raw
 * S7772 (52x) - Unnecessary negated condition → flip
 * S7780 (43x) - Use RegExp.exec() instead of .match() → rewrite
 * S7764 (32x) - String.raw`` for regex with backslash → String.raw
 * S1128 (28x) - Unused imports → remove line
 * S3735 (28x) - Void operator misuse → remove void
 * S6535 (27x) - forEach → for...of
 * S6582 (24x) - Optional chaining → use ?.
 * S7735 (22x) - Prefer 'using' over manual cleanup
 * S6594 (13x) - indexOf !== -1 → includes()
 * S1874 (19x) - Deprecated API usage
 * S7748 (15x) - Unnecessary condition
 * S4325 (10x) - Unnecessary type assertion
 * S1854 (7x) - Dead stores
 */
const fs = require('fs');
const path = require('path');
const data = require('./sonar_all.json');

let fixedCount = 0;
let skipped = {};

// Group by file
const byFile = {};
for (const issue of data.issues) {
    if (!issue.component || !issue.textRange) continue;
    const fp = path.join(__dirname, issue.component.replace('Liva:', ''));
    if (!byFile[fp]) byFile[fp] = [];
    byFile[fp].push(issue);
}

for (const [filePath, issues] of Object.entries(byFile)) {
    if (!fs.existsSync(filePath)) continue;
    
    let content = fs.readFileSync(filePath, 'utf-8');
    let lines = content.split('\n');
    let modified = false;
    
    // Sort DESCENDING by line to avoid shifts
    issues.sort((a, b) => b.textRange.startLine - a.textRange.startLine);
    
    for (const issue of issues) {
        const li = issue.textRange.startLine - 1;
        if (li < 0 || li >= lines.length) continue;
        
        let line = lines[li];
        const orig = line;
        
        switch (issue.rule) {

            // S2486: Catch binding unused → remove binding completely
            case 'typescript:S2486': {
                // } catch (e) { ... } where e is not used
                const m = line.match(/catch\s*\((\w+)(?::\s*\w+)?\)/);
                if (m) {
                    // Check next ~15 lines in catch block for usage
                    const varName = m[1];
                    let braceCount = 0;
                    let used = false;
                    for (let j = li; j < Math.min(li + 20, lines.length); j++) {
                        const l = lines[j];
                        if (j === li) {
                            // Check after catch(...) on same line
                            const afterCatch = l.substring(l.indexOf(')') + 1);
                            if (new RegExp('\\b' + varName + '\\b').test(afterCatch)) { used = true; break; }
                        } else {
                            if (new RegExp('\\b' + varName + '\\b').test(l)) { used = true; break; }
                        }
                        braceCount += (l.match(/\{/g) || []).length - (l.match(/\}/g) || []).length;
                        if (braceCount <= 0 && j > li) break;
                    }
                    if (!used) {
                        line = line.replace(/catch\s*\(\w+(?::\s*\w+)?\)/, 'catch');
                    }
                }
                break;
            }

            // S1128: Unused imports → delete line
            case 'typescript:S1128': {
                if (line.match(/^\s*import\s/)) {
                    lines.splice(li, 1);
                    modified = true;
                    fixedCount++;
                    continue;
                }
                break;
            }

            // S3735: void operator → remove
            case 'typescript:S3735': {
                // void expr; → expr; (but NOT in catch blocks with void e;)
                // Also void 0 is fine
                if (line.match(/\bvoid\s+\w+/) && !line.match(/catch/) && !line.match(/void\s+0/)) {
                    line = line.replace(/\bvoid\s+(\w+)/, '$1');
                }
                break;
            }

            // S6594: indexOf → includes
            case 'typescript:S6594': {
                line = line.replace(/\.indexOf\(([^)]+)\)\s*!==\s*-1/g, '.includes($1)');
                line = line.replace(/\.indexOf\(([^)]+)\)\s*>=\s*0/g, '.includes($1)');
                line = line.replace(/\.indexOf\(([^)]+)\)\s*>\s*-1/g, '.includes($1)');
                line = line.replace(/\.indexOf\(([^)]+)\)\s*===\s*-1/g, '!$&'.replace(/!.*/, function() {
                    return line.replace(/(\S+)\.indexOf\(([^)]+)\)\s*===\s*-1/, '!$1.includes($2)');
                }));
                break;
            }

            // S6582: Optional chaining
            case 'typescript:S6582': {
                // a && a.b → a?.b (only safe for simple patterns)
                const optMatch = line.match(/(\w+)\s*&&\s*\1\./);
                if (optMatch) {
                    line = line.replace(new RegExp(optMatch[1] + '\\s*&&\\s*' + optMatch[1] + '\\.'), optMatch[1] + '?.');
                }
                break;
            }

            // S4138: for...of instead of forEach for array iteration
            case 'typescript:S4138': {
                // Simple forEach → for...of (only safe for trivial cases)
                // Skip - too risky for auto-fix
                skipped[issue.rule] = (skipped[issue.rule] || 0) + 1;
                continue;
            }

            // S1854: Dead stores → remove assignment
            case 'typescript:S1854': {
                // Too risky for auto-fix
                skipped[issue.rule] = (skipped[issue.rule] || 0) + 1;
                continue;
            }

            // S2933: readonly properties - too risky without type analysis
            case 'typescript:S2933':
            // S3776: Cognitive complexity - requires refactoring
            case 'typescript:S3776':
            // S7781/S7764: String.raw - complex transformation
            case 'typescript:S7781':
            case 'typescript:S7764':
            // S7772: Negated conditions - logic change
            case 'typescript:S7772':
            // S7780: RegExp.exec - needs context
            case 'typescript:S7780':
            // S6535: forEach → for...of
            case 'typescript:S6535':
            // S7735: using keyword
            case 'typescript:S7735':
            // S7748: unnecessary condition
            case 'typescript:S7748':
            // S4325: unnecessary assertion
            case 'typescript:S4325':
                skipped[issue.rule] = (skipped[issue.rule] || 0) + 1;
                continue;

            default:
                continue;
        }
        
        if (line !== orig) {
            lines[li] = line;
            modified = true;
            fixedCount++;
        }
    }
    
    if (modified) {
        fs.writeFileSync(filePath, lines.join('\n'));
    }
}

console.log(`Fixed: ${fixedCount} issues`);
console.log('Skipped (need manual/complex fix):');
Object.entries(skipped).sort((a,b)=>b[1]-a[1]).forEach(([r,c])=>console.log(`  ${c}x ${r}`));
