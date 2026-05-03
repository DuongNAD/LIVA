import * as fs from 'fs/promises';
import * as path from 'path';

const SKILLS_DIR = path.join(process.cwd(), 'src', 'skills');

async function fixImports(dir) {
    const entries = await fs.readdir(dir, { withFileTypes: true });
    for (const entry of entries) {
        const fullPath = path.join(dir, entry.name);
        if (entry.isDirectory()) {
            await fixImports(fullPath);
        } else if (entry.isFile() && entry.name.endsWith('.ts')) {
            let content = await fs.readFile(fullPath, 'utf8');
            let modified = false;

            if (content.match(/from\s+["']\.\.\/evolution\/(.*?)["']/g)) {
                content = content.replace(/from\s+["']\.\.\/evolution\/(.*?)["']/g, 'from "@evolution/$1"');
                modified = true;
            }
            if (content.match(/from\s+["']\.\.\/\.\.\/evolution\/(.*?)["']/g)) {
                content = content.replace(/from\s+["']\.\.\/\.\.\/evolution\/(.*?)["']/g, 'from "@evolution/$1"');
                modified = true;
            }

            if (modified) {
                await fs.writeFile(fullPath, content, 'utf8');
                console.log(`Fixed evolution in ${entry.name}`);
            }
        }
    }
}

fixImports(SKILLS_DIR).catch(console.error);
