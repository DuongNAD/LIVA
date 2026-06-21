import { execSync } from 'child_process';
import * as fs from 'fs';
import * as path from 'path';
import { fileURLToPath } from 'url';
const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
// Helper to sanitize/clean symbol name for filename
function getCleanSymbolName(symbol) {
    return symbol.replace(/[^a-zA-Z0-9_-]/g, '_');
}
function main() {
    const args = process.argv.slice(2);
    let symbolName = '';
    const symbolIndex = args.indexOf('--symbol');
    if (symbolIndex !== -1 && symbolIndex < args.length - 1) {
        symbolName = args[symbolIndex + 1];
    }
    else {
        // Try to find a non-flag argument
        const nonFlagArgs = args.filter(arg => !arg.startsWith('-'));
        if (nonFlagArgs.length > 0) {
            symbolName = nonFlagArgs[0];
        }
    }
    if (!symbolName) {
        console.error('Error: --symbol <symbolName> is required.');
        process.exit(1);
    }
    console.log(`==================================================`);
    console.log(`GitNexus to Obsidian Sync: Symbol Analysis`);
    console.log(`Symbol: ${symbolName}`);
    console.log(`==================================================\n`);
    // Path to the runner
    // E:\Project\LIVA\.gitnexus\run.cjs
    // We resolve relative to this script
    const gitnexusRunner = path.resolve(__dirname, '../../../.gitnexus/run.cjs');
    if (!fs.existsSync(gitnexusRunner)) {
        console.error(`Error: GitNexus runner not found at: ${gitnexusRunner}`);
        process.exit(1);
    }
    // Programmatically run GitNexus context and impact commands
    const contextCmd = `node "${gitnexusRunner}" context "${symbolName}" -r LIVA`;
    const impactCmd = `node "${gitnexusRunner}" impact "${symbolName}" -r LIVA`;
    console.log(`Running: ${contextCmd}`);
    let contextJson = null;
    try {
        const stdout = execSync(contextCmd, { encoding: 'utf8', stdio: ['pipe', 'pipe', 'ignore'] });
        contextJson = JSON.parse(stdout);
    }
    catch (err) {
        console.error(`Failed to execute or parse context command:`, err.message);
        process.exit(1);
    }
    console.log(`Running: ${impactCmd}`);
    let impactJson = null;
    try {
        const stdout = execSync(impactCmd, { encoding: 'utf8', stdio: ['pipe', 'pipe', 'ignore'] });
        impactJson = JSON.parse(stdout);
    }
    catch (err) {
        console.error(`Failed to execute or parse impact command:`, err.message);
        process.exit(1);
    }
    // Handle GitNexus CLI errors
    if (contextJson.error) {
        console.error(`GitNexus context error: ${contextJson.error}`);
        process.exit(1);
    }
    if (impactJson.error) {
        console.error(`GitNexus impact error: ${impactJson.error}`);
        process.exit(1);
    }
    // Extract necessary details
    const symbolInfo = contextJson.symbol || {};
    const filePath = symbolInfo.filePath || 'Unknown';
    const startLine = symbolInfo.startLine ?? 0;
    const endLine = symbolInfo.endLine ?? 0;
    const incomingCalls = contextJson.incoming?.calls || [];
    const outgoingCalls = contextJson.outgoing?.calls || [];
    const targetInfo = impactJson.target || {};
    const type = targetInfo.type || 'Unknown';
    const risk = impactJson.risk || 'UNKNOWN';
    const direction = impactJson.direction || 'upstream';
    const impactedCount = impactJson.impactedCount ?? 0;
    const summary = impactJson.summary || {};
    const affectedProcesses = impactJson.affected_processes || [];
    const affectedModules = impactJson.affected_modules || [];
    const byDepth = impactJson.byDepth || {};
    // Formulate markdown sections
    const cleanSymbolName = getCleanSymbolName(symbolName);
    const lastUpdate = new Date().toISOString();
    // Create call graph tables
    let incomingCallsContent = '';
    if (incomingCalls.length === 0) {
        incomingCallsContent = 'No direct incoming callers identified.';
    }
    else {
        incomingCallsContent = `| Symbol Name | File Path | Unique Identifier |\n| :--- | :--- | :--- |\n` +
            incomingCalls.map((c) => `| \`${c.name}\` | \`${c.filePath}\` | \`${c.uid}\` |`).join('\n');
    }
    let outgoingCallsContent = '';
    if (outgoingCalls.length === 0) {
        outgoingCallsContent = 'No direct outgoing calls/dependencies identified.';
    }
    else {
        outgoingCallsContent = `| Symbol Name | File Path | Unique Identifier |\n| :--- | :--- | :--- |\n` +
            outgoingCalls.map((c) => `| \`${c.name}\` | \`${c.filePath}\` | \`${c.uid}\` |`).join('\n');
    }
    // Affected processes
    const affectedProcessesContent = affectedProcesses.length === 0
        ? 'None'
        : affectedProcesses.map((p) => `\n- \`${p}\``).join('');
    // Affected modules
    let affectedModulesContent = '';
    if (affectedModules.length === 0) {
        affectedModulesContent = 'No modules affected.';
    }
    else {
        affectedModulesContent = `| Module Name | Hits | Impact Type |\n| :--- | :--- | :--- |\n` +
            affectedModules.map((m) => `| \`${m.name}\` | ${m.hits} | \`${m.impact}\` |`).join('\n');
    }
    // Depth content
    let depthContent = '';
    const depthKeys = Object.keys(byDepth).sort((a, b) => parseInt(a) - parseInt(b));
    if (depthKeys.length === 0) {
        depthContent = 'No depth analysis data available.';
    }
    else {
        depthContent = `| Depth | Symbol Name | Relation | File Path | ID |\n| :--- | :--- | :--- | :--- | :--- |\n` +
            depthKeys.flatMap(depth => byDepth[depth].map((item) => `| ${depth} | \`${item.name}\` | \`${item.relationType}\` | \`${item.filePath}\` | \`${item.id}\` |`)).join('\n');
    }
    // Conformance to Templates/Knowledge Template.md
    // including title (matching filename), tags (containing liva/knowledge and liva/architecture),
    // author (set to gitnexus-bridge), and last_update (ISO 8601 timestamp)
    const markdownReport = `---
title: "${cleanSymbolName}_architecture"
tags:
  - liva/knowledge
  - liva/architecture
author: "gitnexus-bridge"
last_update: "${lastUpdate}"
confidence: "high"
sources:
  - "gitnexus"
---

# Knowledge: ${cleanSymbolName}_architecture

## Executive Summary
This document provides an automated architectural analysis of the symbol \`${symbolName}\` in the LIVA codebase. It details the symbol's location, incoming/outgoing call relationships, and upstream dependency impact metrics, generated using GitNexus code intelligence.

## Detailed Description
The symbol \`${symbolName}\` is analyzed within its structural context. Below is the detailed breakdown of its location, relations, and code impact footprint.

### Code Location & Definition
- **Symbol Name**: \`${symbolName}\`
- **File Path**: \`${filePath}\`
- **Line Range**: Lines ${startLine} to ${endLine}
- **Type**: \`${type}\`

### Call Graph (Incoming & Outgoing Calls)
The following tables list the direct callers (incoming) and dependencies (outgoing) of the symbol.

#### Incoming Calls (Dependents)
${incomingCallsContent}

#### Outgoing Calls (Dependencies)
${outgoingCallsContent}

### Impact & Risk Analysis
The symbol's risk rating and affected modules if modified, analyzed in the upstream direction:
- **Upstream Risk**: \`${risk}\`
- **Impacted Entities Count**: ${impactedCount}
- **Affected Processes**: ${affectedProcessesContent}
- **Affected Modules**:
${affectedModulesContent}

#### Dependency Path by Depth
${depthContent}

## Relationships & References
- [[liva_architecture]]
- [[GitNexus Guide]]
`;
    // Write file to vault/Knowledge/<cleanSymbolName>_architecture.md
    const targetDir = path.resolve(__dirname, '../vault/Knowledge');
    if (!fs.existsSync(targetDir)) {
        fs.mkdirSync(targetDir, { recursive: true });
    }
    const targetFile = path.join(targetDir, `${cleanSymbolName}_architecture.md`);
    console.log(`Writing architecture report to: ${targetFile}`);
    fs.writeFileSync(targetFile, markdownReport, 'utf8');
    console.log(`🎉 Successfully synchronized architecture for symbol: ${symbolName}`);
}
main();
//# sourceMappingURL=gitnexus-obsidian-sync.js.map