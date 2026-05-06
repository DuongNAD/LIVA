const fs = require('fs');
const path = require('path');

function walk(dir) {
    let results = [];
    const list = fs.readdirSync(dir);
    list.forEach(file => {
        file = path.join(dir, file);
        const stat = fs.statSync(file);
        if (stat && stat.isDirectory()) {
            results = results.concat(walk(file));
        } else if (file.endsWith('.ts')) {
            results.push(file);
        }
    });
    return results;
}

const files = walk('e:/project/openclaw_remake/openclaw-gateway/src');

files.forEach(file => {
    let content = fs.readFileSync(file, 'utf8');
    let newContent = content.replace(/logger\.(error|warn|info)\((\"[^\"]+\"),\s*errMsg\)/g, 'logger.$1($2 + \" \" + errMsg)');
    
    if (content !== newContent) {
        fs.writeFileSync(file, newContent, 'utf8');
        console.log(`Fixed logger in ${file}`);
    }
});
