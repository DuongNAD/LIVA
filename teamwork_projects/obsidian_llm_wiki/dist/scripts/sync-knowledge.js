import * as fs from 'fs';
import * as path from 'path';
import { fileURLToPath } from 'url';
import { validateVault } from './validate-vault.js';
const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
function getMdFiles(baseDir, subDirs) {
    const files = [];
    for (const subDir of subDirs) {
        const subDirPath = path.join(baseDir, subDir);
        if (!fs.existsSync(subDirPath))
            continue;
        function traverse(currentDir) {
            const entries = fs.readdirSync(currentDir, { withFileTypes: true });
            for (const entry of entries) {
                const fullPath = path.join(currentDir, entry.name);
                if (entry.isDirectory()) {
                    traverse(fullPath);
                }
                else if (entry.isFile() && entry.name.endsWith('.md')) {
                    files.push(path.relative(baseDir, fullPath).replace(/\\/g, '/'));
                }
            }
        }
        traverse(subDirPath);
    }
    return files;
}
function main() {
    const args = process.argv.slice(2);
    const dryRun = args.includes('--dry-run');
    const force = args.includes('--force');
    const vaultRoot = path.resolve(__dirname, '../vault');
    const docsRoot = path.resolve(__dirname, '../../../docs');
    console.log(`==================================================`);
    console.log(`Obsidian Vault & Codebase Docs Sync Script`);
    console.log(`Vault Root: ${vaultRoot}`);
    console.log(`Docs Root:  ${docsRoot}`);
    console.log(`Flags:      dry-run=${dryRun}, force=${force}`);
    console.log(`==================================================\n`);
    if (!fs.existsSync(vaultRoot)) {
        console.error(`Error: Vault root path does not exist: ${vaultRoot}`);
        process.exit(1);
    }
    // 1. Run Vault Validation
    console.log(`Running format validation on Vault...`);
    const validationReport = validateVault(vaultRoot);
    const isValid = validationReport.errors.length === 0 && validationReport.invalidFilesCount === 0;
    if (isValid) {
        console.log(`🎉 Vault validation passed! Format conforms strictly to specifications.\n`);
    }
    else {
        console.warn(`❌ Vault validation failed!`);
        console.warn(`Total Files Checked:  ${validationReport.totalFilesChecked}`);
        console.warn(`Invalid Files Count:  ${validationReport.invalidFilesCount}`);
        console.warn(`Broken Wiki Links:    ${validationReport.brokenLinksCount}`);
        for (const err of validationReport.errors) {
            console.warn(` - Global Error: ${err}`);
        }
        for (const res of validationReport.fileResults) {
            if (!res.isValid) {
                console.warn(` - File Invalid: ${res.relativeDocPath}`);
                for (const err of res.errors) {
                    console.warn(`     Error: ${err}`);
                }
            }
        }
        if (!force) {
            console.error(`\n🛑 Aborting sync: Vault has format violations. Fix them or run with --force to bypass.`);
            process.exit(1);
        }
        else {
            console.log(`\n⚠️ Force flag enabled: proceeding with sync despite validation failures.\n`);
        }
    }
    // 2. Scan directories
    const syncFolders = ['Skills', 'Knowledge', 'Rules', 'Templates'];
    const vaultFiles = getMdFiles(vaultRoot, syncFolders);
    const docsFiles = getMdFiles(docsRoot, syncFolders);
    const allRelPaths = Array.from(new Set([...vaultFiles, ...docsFiles])).sort();
    const actions = [];
    let vaultOnlyCount = 0;
    let docsOnlyCount = 0;
    let outOfSyncCount = 0;
    let inSyncCount = 0;
    for (const relPath of allRelPaths) {
        const vaultPath = path.join(vaultRoot, relPath);
        const docsPath = path.join(docsRoot, relPath);
        const inVault = fs.existsSync(vaultPath);
        const inDocs = fs.existsSync(docsPath);
        if (inVault && !inDocs) {
            vaultOnlyCount++;
            actions.push({
                type: 'copy_to_docs',
                relPath,
                reason: 'File exists in vault but is missing from codebase docs',
            });
        }
        else if (!inVault && inDocs) {
            docsOnlyCount++;
            actions.push({
                type: 'copy_to_vault',
                relPath,
                reason: 'File exists in codebase docs but is missing from vault',
            });
        }
        else {
            // Exists in both, check if contents are different
            const vaultContent = fs.readFileSync(vaultPath, 'utf8');
            const docsContent = fs.readFileSync(docsPath, 'utf8');
            if (vaultContent === docsContent) {
                inSyncCount++;
                if (force) {
                    // Force option copies everything from vault to docs to be absolutely certain
                    actions.push({
                        type: 'copy_to_docs',
                        relPath,
                        reason: 'Force sync: overwriting in-sync file from vault to docs',
                    });
                }
            }
            else {
                outOfSyncCount++;
                if (force) {
                    actions.push({
                        type: 'copy_to_docs',
                        relPath,
                        reason: 'Force sync: overwriting modified file from vault to docs',
                    });
                }
                else {
                    // Compare modification times
                    const vaultMtime = fs.statSync(vaultPath).mtimeMs;
                    const docsMtime = fs.statSync(docsPath).mtimeMs;
                    if (vaultMtime >= docsMtime) {
                        actions.push({
                            type: 'update_docs',
                            relPath,
                            reason: `Vault version is newer than docs version (${new Date(vaultMtime).toISOString()} >= ${new Date(docsMtime).toISOString()})`,
                        });
                    }
                    else {
                        actions.push({
                            type: 'update_vault',
                            relPath,
                            reason: `Docs version is newer than vault version (${new Date(docsMtime).toISOString()} > ${new Date(vaultMtime).toISOString()})`,
                        });
                    }
                }
            }
        }
    }
    // 3. Execute actions
    console.log(`Proposed Actions:`);
    if (actions.length === 0) {
        console.log(` - No sync actions needed. Everything is in sync.`);
    }
    else {
        for (const action of actions) {
            const prefix = dryRun ? `[Dry-Run]` : `[Executing]`;
            let desc = '';
            if (action.type === 'copy_to_docs') {
                desc = `Copy Vault -> Docs: "${action.relPath}"`;
            }
            else if (action.type === 'copy_to_vault') {
                desc = `Copy Docs -> Vault: "${action.relPath}"`;
            }
            else if (action.type === 'update_docs') {
                desc = `Update Vault -> Docs: "${action.relPath}"`;
            }
            else if (action.type === 'update_vault') {
                desc = `Update Docs -> Vault: "${action.relPath}"`;
            }
            console.log(` - ${prefix} ${desc}`);
            console.log(`     Reason: ${action.reason}`);
            if (!dryRun) {
                try {
                    const vaultPath = path.join(vaultRoot, action.relPath);
                    const docsPath = path.join(docsRoot, action.relPath);
                    if (action.type === 'copy_to_docs' || action.type === 'update_docs') {
                        fs.mkdirSync(path.dirname(docsPath), { recursive: true });
                        fs.copyFileSync(vaultPath, docsPath);
                    }
                    else if (action.type === 'copy_to_vault' || action.type === 'update_vault') {
                        fs.mkdirSync(path.dirname(vaultPath), { recursive: true });
                        fs.copyFileSync(docsPath, vaultPath);
                    }
                }
                catch (err) {
                    console.error(`   ❌ Failed to sync "${action.relPath}": ${err.message}`);
                }
            }
        }
    }
    console.log(`\n==================================================`);
    console.log(`Sync Summary:`);
    console.log(`==================================================`);
    console.log(`Total checked in Vault: ${vaultFiles.length}`);
    console.log(`Total checked in Docs:  ${docsFiles.length}`);
    console.log(`Vault-only files:       ${vaultOnlyCount}`);
    console.log(`Docs-only files:        ${docsOnlyCount}`);
    console.log(`Out-of-sync files:      ${outOfSyncCount}`);
    console.log(`In-sync files:          ${inSyncCount}`);
    console.log(`Total actions proposed/taken: ${actions.length}`);
    console.log(`==================================================`);
    if (!dryRun && actions.length > 0) {
        console.log(`\n🎉 Synchronization completed successfully!`);
    }
    else if (dryRun) {
        console.log(`\n💡 Dry-run completed. No files were modified.`);
    }
    else {
        console.log(`\n🎉 Everything is already up to date.`);
    }
}
main();
//# sourceMappingURL=sync-knowledge.js.map