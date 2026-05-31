import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';

/**
 * audit-tenant-queries.ts
 *
 * Diagnostic script that performs regex scanning on route files
 * inside `server/routes/` to ensure all endpoints referencing
 * "/:id" parameters utilize `withFeedOwnership` or `requireOrgAdmin`.
 */

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const ROUTES_DIR = path.resolve(__dirname, '../../../server/routes');

console.log('🔍 Starting security IDOR audit across backend router directories...');
console.log(`📂 Path: ${ROUTES_DIR}\n`);

let issuesFound = 0;

function scanDirectory(dir: string) {
    const files = fs.readdirSync(dir);

    for (const file of files) {
        const fullPath = path.join(dir, file);
        const stat = fs.statSync(fullPath);

        if (stat.isDirectory()) {
            scanDirectory(fullPath);
        } else if (file.endsWith('.ts') || file.endsWith('.js')) {
            auditFile(fullPath);
        }
    }
}

function auditFile(filepath: string) {
    const relativePath = path.relative(ROUTES_DIR, filepath);
    const content = fs.readFileSync(filepath, 'utf-8');

    // Skip native auth router (intentionally unauthenticated)
    if (relativePath.includes('nativeAuth')) return;

    const lines = content.split('\n');
    
    for (let i = 0; i < lines.length; i++) {
        const line = lines[i];

        // Match typical Express router HTTP methods targeting "/:id"
        if (line.includes("router.") && line.includes("/:id")) {
            const hasOwnershipMiddleware = line.includes('withFeedOwnership') 
                || line.includes('requireOrgAdmin')
                || line.includes('withVaultOwnership'); // mock visual check

            if (!hasOwnershipMiddleware) {
                console.warn(`⚠️  Potential IDOR risk: ${relativePath} [Line ${i + 1}]`);
                console.warn(`   👉 ${line.trim()}`);
                issuesFound++;
            }
        }
    }
}

try {
    scanDirectory(ROUTES_DIR);
    console.log('\n------------------------------------------------');
    if (issuesFound === 0) {
        console.log('✅ Audit complete. Zero potential IDOR route patterns detected.');
    } else {
        console.log(`⚠️  Audit complete. Identified ${issuesFound} potential IDOR patterns.`);
        console.log('   Action: Verify if these routes are properly gated or isolated.');
    }
} catch (error) {
    console.error('❌ Failed to execute query audit:', error);
}
