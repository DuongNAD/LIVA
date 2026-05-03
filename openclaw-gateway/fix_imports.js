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

            if (content.match(/from\s+["']\.\.\/security\/(.*?)["']/g)) {
                content = content.replace(/from\s+["']\.\.\/security\/(.*?)["']/g, 'from "@security/$1"');
                modified = true;
            }
            if (content.match(/from\s+["']\.\.\/\.\.\/security\/(.*?)["']/g)) {
                content = content.replace(/from\s+["']\.\.\/\.\.\/security\/(.*?)["']/g, 'from "@security/$1"');
                modified = true;
            }
            if (content.match(/from\s+["']\.\.\/mcp\/(.*?)["']/g)) {
                content = content.replace(/from\s+["']\.\.\/mcp\/(.*?)["']/g, 'from "@mcp/$1"');
                modified = true;
            }
            if (content.match(/from\s+["']\.\.\/\.\.\/mcp\/(.*?)["']/g)) {
                content = content.replace(/from\s+["']\.\.\/\.\.\/mcp\/(.*?)["']/g, 'from "@mcp/$1"');
                modified = true;
            }
            
            // Fix any ../utils left behind (some might be ../../utils depending on nested)
            if (content.match(/from\s+["']\.\.\/\.\.\/utils\/(.*?)["']/g)) {
                content = content.replace(/from\s+["']\.\.\/\.\.\/utils\/(.*?)["']/g, 'from "@utils/$1"');
                modified = true;
            }

            if (modified) {
                await fs.writeFile(fullPath, content, 'utf8');
                console.log(`Fixed imports in ${entry.name}`);
            }
        }
    }
}

fixImports(SKILLS_DIR).catch(console.error);
