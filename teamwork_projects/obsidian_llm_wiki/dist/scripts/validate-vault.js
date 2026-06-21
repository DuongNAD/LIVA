import * as fs from 'fs';
import * as path from 'path';
// @ts-ignore
import yaml from 'js-yaml';
import { fileURLToPath } from 'url';
const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
/**
 * Normalizes strings for robust loose matching (e.g., comparing filenames and titles).
 */
function normalizeString(str) {
    return str.toLowerCase().replace(/[^a-z0-9]/g, '');
}
/**
 * Validates the Obsidian Vault structure, template files, frontmatter schemas, and wiki links.
 *
 * @param vaultRoot Absolute path to the Obsidian Vault root directory
 */
export function validateVault(vaultRoot) {
    const report = {
        totalFilesChecked: 0,
        invalidFilesCount: 0,
        brokenLinksCount: 0,
        errors: [],
        fileResults: [],
    };
    // 1. Verify Basic Directories Exist
    const requiredDirs = ['Skills', 'Knowledge', 'Rules', 'Templates'];
    for (const dir of requiredDirs) {
        const dirPath = path.join(vaultRoot, dir);
        if (!fs.existsSync(dirPath)) {
            report.errors.push(`Directory missing: '${dir}' must exist in the vault root`);
        }
        else if (!fs.statSync(dirPath).isDirectory()) {
            report.errors.push(`Path is not a directory: '${dir}'`);
        }
    }
    // If any core directory is missing, we stop early as structural integrity is compromised
    if (report.errors.length > 0) {
        return report;
    }
    // 2. Verify Template Files Exist
    const requiredTemplates = [
        'Skill Template.md',
        'Knowledge Template.md',
        'Rule Template.md'
    ];
    for (const template of requiredTemplates) {
        const templatePath = path.join(vaultRoot, 'Templates', template);
        if (!fs.existsSync(templatePath)) {
            report.errors.push(`Template file missing: '${template}' must exist in 'Templates/'`);
        }
        else if (!fs.statSync(templatePath).isFile()) {
            report.errors.push(`Path is not a file: 'Templates/${template}'`);
        }
    }
    // 3. Retrieve All Markdown Files in the Vault
    const mdFiles = [];
    function traverse(currentDir) {
        const entries = fs.readdirSync(currentDir, { withFileTypes: true });
        for (const entry of entries) {
            const fullPath = path.join(currentDir, entry.name);
            if (entry.isDirectory()) {
                traverse(fullPath);
            }
            else if (entry.isFile() && entry.name.endsWith('.md')) {
                mdFiles.push(fullPath);
            }
        }
    }
    traverse(vaultRoot);
    // Build an index of valid wiki link targets
    // Obsidian supports linking via relative path or unique filename
    const validTargets = new Set();
    for (const filePath of mdFiles) {
        const relPath = path.relative(vaultRoot, filePath).replace(/\\/g, '/');
        const baseName = path.basename(filePath);
        const baseNameNoExt = path.basename(filePath, '.md');
        validTargets.add(relPath.toLowerCase());
        validTargets.add(relPath.slice(0, -3).toLowerCase()); // remove '.md'
        validTargets.add(baseName.toLowerCase());
        validTargets.add(baseNameNoExt.toLowerCase());
    }
    // 4. Validate Each File's Frontmatter and Extract Wiki Links
    const allWikiLinks = [];
    for (const filePath of mdFiles) {
        const relativeDocPath = path.relative(vaultRoot, filePath).replace(/\\/g, '/');
        const isTemplate = relativeDocPath.startsWith('Templates/');
        report.totalFilesChecked++;
        const fileErrors = [];
        const fileWarnings = [];
        let fileContent = '';
        try {
            fileContent = fs.readFileSync(filePath, 'utf8');
        }
        catch (err) {
            fileErrors.push(`Failed to read file: ${err.message}`);
            report.fileResults.push({
                filePath,
                relativeDocPath,
                isValid: false,
                errors: fileErrors,
                warnings: fileWarnings,
                isTemplate,
            });
            report.invalidFilesCount++;
            continue;
        }
        // Parse Frontmatter
        const frontmatterRegex = /^---\r?\n([\s\S]*?)\r?\n---(?:\r?\n|$)/;
        const match = fileContent.match(frontmatterRegex);
        if (!match) {
            fileErrors.push("Missing or invalid YAML frontmatter block. It must start and end with '---' lines.");
            report.fileResults.push({
                filePath,
                relativeDocPath,
                isValid: false,
                errors: fileErrors,
                warnings: fileWarnings,
                isTemplate,
            });
            report.invalidFilesCount++;
            continue;
        }
        const yamlText = match[1];
        const bodyContent = fileContent.slice(match[0].length);
        let frontmatter = null;
        try {
            frontmatter = yaml.load(yamlText);
            if (typeof frontmatter !== 'object' || frontmatter === null) {
                fileErrors.push("Frontmatter is not a valid YAML object");
            }
        }
        catch (yamlErr) {
            fileErrors.push(`YAML parser error: ${yamlErr.message}`);
        }
        if (fileErrors.length > 0) {
            report.fileResults.push({
                filePath,
                relativeDocPath,
                isValid: false,
                errors: fileErrors,
                warnings: fileWarnings,
                isTemplate,
            });
            report.invalidFilesCount++;
            continue;
        }
        // Validate Schema Compliance
        // Required fields: title, tags, author, last_update
        const requiredFields = ['title', 'tags', 'author', 'last_update'];
        for (const field of requiredFields) {
            if (frontmatter[field] === undefined || frontmatter[field] === null) {
                fileErrors.push(`Missing required frontmatter field: '${field}'`);
            }
        }
        if (fileErrors.length === 0) {
            // Detailed field validations (skipped/relaxed for templates containing placeholders)
            if (!isTemplate) {
                // Validate 'title'
                if (typeof frontmatter.title !== 'string') {
                    fileErrors.push("'title' must be a string");
                }
                else {
                    const filenameNoExt = path.basename(filePath, '.md');
                    if (frontmatter.title !== filenameNoExt) {
                        // Check if loose match (ignoring case, spaces, symbols)
                        if (normalizeString(frontmatter.title) !== normalizeString(filenameNoExt)) {
                            fileErrors.push(`'title' ("${frontmatter.title}") does not match filename ("${filenameNoExt}")`);
                        }
                        else {
                            fileWarnings.push(`'title' ("${frontmatter.title}") differs in casing/spacing from filename ("${filenameNoExt}")`);
                        }
                    }
                }
                // Validate 'tags'
                if (!Array.isArray(frontmatter.tags)) {
                    fileErrors.push("'tags' must be an array of strings");
                }
                else {
                    if (frontmatter.tags.length === 0) {
                        fileErrors.push("'tags' must contain at least one tag");
                    }
                    const hasLivaTag = frontmatter.tags.some((tag) => typeof tag === 'string' && tag.startsWith('liva/'));
                    if (!hasLivaTag) {
                        fileErrors.push("At least one tag must start with 'liva/' (e.g. 'liva/skill')");
                    }
                    const nonStringTags = frontmatter.tags.filter((tag) => typeof tag !== 'string');
                    if (nonStringTags.length > 0) {
                        fileErrors.push("All tags must be string values");
                    }
                }
                // Validate 'author'
                if (typeof frontmatter.author !== 'string' || frontmatter.author.trim() === '') {
                    fileErrors.push("'author' must be a non-empty string");
                }
                // Validate 'last_update'
                const iso8601Regex = /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|[+-]\d{2}:\d{2})$/;
                if (typeof frontmatter.last_update !== 'string' || !iso8601Regex.test(frontmatter.last_update)) {
                    fileErrors.push(`'last_update' ("${frontmatter.last_update}") must be a valid ISO 8601 datetime string (e.g., 'YYYY-MM-DDTHH:mm:ssZ')`);
                }
                else if (isNaN(Date.parse(frontmatter.last_update))) {
                    fileErrors.push(`'last_update' ("${frontmatter.last_update}") is not a valid date`);
                }
            }
            else {
                // If it is a template, just check that fields exist (they will contain placeholders like "{{title}}")
                if (typeof frontmatter.title !== 'string') {
                    fileErrors.push("'title' in template must be a placeholder string");
                }
                if (!Array.isArray(frontmatter.tags) && typeof frontmatter.tags !== 'string') {
                    fileErrors.push("'tags' in template must be an array or placeholder string");
                }
            }
        }
        // Extract Wiki Links from Body (only for non-template documents)
        // Match [[Wiki Link]], [[Wiki Link#Section]], [[Wiki Link|Alias]]
        if (!isTemplate) {
            const wikiLinkRegex = /\[\[([^\]|#]+)(?:#[^\]|]*)?(?:\|[^\]]*)?\]\]/g;
            const lines = bodyContent.split(/\r?\n/);
            lines.forEach((lineText, index) => {
                let match;
                wikiLinkRegex.lastIndex = 0;
                while ((match = wikiLinkRegex.exec(lineText)) !== null) {
                    allWikiLinks.push({
                        sourceFile: relativeDocPath,
                        target: match[1].trim(),
                        line: index + 1 + match[0].split('\n').length - 1,
                    });
                }
            });
        }
        const isValid = fileErrors.length === 0;
        if (!isValid) {
            report.invalidFilesCount++;
        }
        report.fileResults.push({
            filePath,
            relativeDocPath,
            isValid,
            errors: fileErrors,
            warnings: fileWarnings,
            isTemplate,
        });
    }
    // 5. Verify Internal Wiki Links
    for (const link of allWikiLinks) {
        const normalizedTarget = link.target.toLowerCase();
        if (!validTargets.has(normalizedTarget)) {
            report.brokenLinksCount++;
            const fileResult = report.fileResults.find(r => r.relativeDocPath === link.sourceFile);
            if (fileResult) {
                fileResult.isValid = false;
                fileResult.errors.push(`Broken wiki link found: '[[${link.target}]]' (referenced at line ${link.line})`);
            }
        }
    }
    report.invalidFilesCount = report.fileResults.filter(r => !r.isValid).length;
    return report;
}
/**
 * Main execution handler.
 */
function main() {
    const defaultVaultPath = path.resolve(__dirname, '../vault');
    const vaultPath = process.argv[2] ? path.resolve(process.argv[2]) : defaultVaultPath;
    console.log(`=========================================`);
    console.log(`Obsidian Vault Automated Validation Script`);
    console.log(`Target Vault: ${vaultPath}`);
    console.log(`=========================================\n`);
    if (!fs.existsSync(vaultPath)) {
        console.error(`Error: Vault root path does not exist: ${vaultPath}`);
        process.exit(1);
    }
    const report = validateVault(vaultPath);
    // Print File Results
    console.log('Checked Files:');
    for (const res of report.fileResults) {
        const status = res.isValid ? '✅ VALID' : '❌ INVALID';
        const typeLabel = res.isTemplate ? '[Template]' : '[Document]';
        console.log(` - ${status} ${typeLabel} ${res.relativeDocPath}`);
        if (res.errors.length > 0) {
            for (const err of res.errors) {
                console.log(`     Error: ${err}`);
            }
        }
        if (res.warnings.length > 0) {
            for (const warn of res.warnings) {
                console.log(`     Warning: ${warn}`);
            }
        }
    }
    console.log(`\n=============================`);
    console.log(`Validation Summary:`);
    console.log(`=============================`);
    console.log(`Total Files Checked:  ${report.totalFilesChecked}`);
    console.log(`Invalid Files:        ${report.invalidFilesCount}`);
    console.log(`Broken Wiki Links:    ${report.brokenLinksCount}`);
    if (report.errors.length > 0) {
        console.log(`\nGlobal Structure Errors:`);
        for (const err of report.errors) {
            console.log(` - ❌ ${err}`);
        }
    }
    const isSuccessful = report.errors.length === 0 && report.invalidFilesCount === 0;
    if (isSuccessful) {
        console.log(`\n🎉 Success: Vault structure and schemas are fully compliant!`);
        process.exit(0);
    }
    else {
        console.error(`\n🛑 Failure: Vault validation failed. See errors listed above.`);
        process.exit(1);
    }
}
// Environment-independent check for direct execution
const isMainFile = () => {
    if (typeof require !== 'undefined' && require.main === module) {
        return true;
    }
    if (process.argv[1]) {
        const mainPath = fs.realpathSync(process.argv[1]);
        try {
            const currentPath = fs.realpathSync(__filename);
            return mainPath === currentPath;
        }
        catch {
            return process.argv[1].endsWith('validate-vault.ts') || process.argv[1].endsWith('validate-vault.js');
        }
    }
    return false;
};
if (isMainFile()) {
    main();
}
//# sourceMappingURL=validate-vault.js.map