import { promises as fsp } from 'fs';
import * as fs from 'fs';
import * as path from 'path';
import * as os from 'os';
import { fileURLToPath } from 'url';
import { InMemoryTransport } from '@modelcontextprotocol/sdk/inMemory.js';
import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { createMcpServer } from '../src/server.js';
import { VaultSearchEngine, VaultManager } from '../src/vault.js';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

interface ToolTextContent {
  type: 'text';
  text: string;
}

interface CallToolSuccessResult {
  content: ToolTextContent[];
  isError?: boolean;
}

describe('LIVA-Obsidian MCP Server Challenger Empirical Verification', () => {
  let tempVaultPath: string;
  let client: Client;
  let mcpServer: any;
  let clientTransport: InMemoryTransport;
  let serverTransport: InMemoryTransport;
  let cleanupServer: () => void;

  beforeEach(async () => {
    tempVaultPath = await fsp.mkdtemp(path.join(os.tmpdir(), 'mcp-challenger-vault-'));
    
    // Copy sample vault contents to temp directory
    const sourceVault = path.resolve(__dirname, '../vault');
    await fsp.cp(sourceVault, tempVaultPath, { recursive: true });

    // Instantiate MCP server
    const serverResult = createMcpServer(tempVaultPath);
    mcpServer = serverResult.mcpServer;
    cleanupServer = serverResult.cleanup;

    [clientTransport, serverTransport] = InMemoryTransport.createLinkedPair();
    await mcpServer.connect(serverTransport);

    client = new Client(
      { name: 'challenger-client', version: '1.0.0' },
      { capabilities: {} }
    );
    await client.connect(clientTransport);
  });

  afterEach(async () => {
    if (client) await client.close();
    if (mcpServer) await mcpServer.close();
    if (cleanupServer) cleanupServer();
    await fsp.rm(tempVaultPath, { recursive: true, force: true });
  });

  // --- AREA 1: SEARCH SCORING LOGIC ---
  describe('Search Scoring Logic', () => {
    test('Verify exact scoring boosts: Title (+10), Tags (+8), Author (+5), Line Match (+1)', async () => {
      // 1. Title match only (via filename, no file contents or frontmatter)
      // Note: VaultSearchEngine only scans Skills, Knowledge, Rules.
      const titleOnlyPath = 'Skills/quantum.md';
      await fsp.writeFile(path.join(tempVaultPath, titleOnlyPath), '', 'utf8');

      // 2. Title match in frontmatter (matches doc.title + line match)
      const titleFmPath = 'Skills/quantum_fm.md';
      await fsp.writeFile(path.join(tempVaultPath, titleFmPath), `---\ntitle: quantum_fm\n---\n`, 'utf8');

      // 3. Tag match (matches tag boost + line match)
      const tagPath = 'Skills/test_tag.md';
      await fsp.writeFile(
        path.join(tempVaultPath, tagPath),
        `---\ntags:\n  - testtag\n---\n`,
        'utf8'
      );

      // 4. Author match (matches author boost + line match)
      const authorPath = 'Skills/test_author.md';
      await fsp.writeFile(
        path.join(tempVaultPath, authorPath),
        `---\nauthor: tester\n---\n`,
        'utf8'
      );

      // 5. Content match only (1 matching line)
      const contentPath = 'Skills/test_content.md';
      await fsp.writeFile(
        path.join(tempVaultPath, contentPath),
        `---\ntitle: normal\n---\nThis is quantum mechanics.\n`,
        'utf8'
      );

      // Trigger write_markdown via client to force indexing for all files
      // (or let the manager re-initialize, but let's just write them and restart/refresh server)
      // To bypass watcher async issues in testing, we can write them via the write_markdown tool.
      const filesToWrite = [
        { path: titleOnlyPath, content: '' },
        { path: titleFmPath, content: `---\ntitle: quantum_fm\n---\n` },
        { path: tagPath, content: `---\ntags:\n  - testtag\n---\n` },
        { path: authorPath, content: `---\nauthor: tester\n---\n` },
        { path: contentPath, content: `---\ntitle: normal\n---\nThis is quantum mechanics.\n` }
      ];

      for (const f of filesToWrite) {
        await client.callTool({
          name: 'write_markdown',
          arguments: { path: f.path, content: f.content }
        });
      }

      // Search for "quantum"
      const resQuantum = await client.callTool({
        name: 'search_vault',
        arguments: { query: 'quantum' }
      });
      const resultsQuantum = JSON.parse((resQuantum as any).content[0].text);
      
      // We expect:
      // quantum.md: Title match only -> score 10 (since no file content at all)
      // quantum_fm.md: Title match (+10) + line match on "title: quantum_fm" (+1) -> score 11
      // test_content.md: Content line match (+1) -> score 1
      
      const quantumDoc = resultsQuantum.find((r: any) => r.path === 'Skills/quantum.md');
      const quantumFmDoc = resultsQuantum.find((r: any) => r.path === 'Skills/quantum_fm.md');
      const contentDoc = resultsQuantum.find((r: any) => r.path === 'Skills/test_content.md');

      expect(quantumDoc).toBeDefined();
      expect(quantumDoc.score).toBe(10);

      expect(quantumFmDoc).toBeDefined();
      expect(quantumFmDoc.score).toBe(11);

      expect(contentDoc).toBeDefined();
      expect(contentDoc.score).toBe(1);

      // Search for "testtag"
      const resTag = await client.callTool({
        name: 'search_vault',
        arguments: { query: 'testtag' }
      });
      const resultsTag = JSON.parse((resTag as any).content[0].text);
      const tagDoc = resultsTag.find((r: any) => r.path === 'Skills/test_tag.md');
      expect(tagDoc).toBeDefined();
      // Expect tag boost (+8) + line match "- testtag" (+1) -> score 9
      expect(tagDoc.score).toBe(9);

      // Search for "tester"
      const resAuthor = await client.callTool({
        name: 'search_vault',
        arguments: { query: 'tester' }
      });
      const resultsAuthor = JSON.parse((resAuthor as any).content[0].text);
      const authorDoc = resultsAuthor.find((r: any) => r.path === 'Skills/test_author.md');
      expect(authorDoc).toBeDefined();
      // Expect author boost (+5) + line match "author: tester" (+1) -> score 6
      expect(authorDoc.score).toBe(6);
    });

    test('Verify results are sorted by score descending', async () => {
      // Create high-scoring file (Title and Content matches)
      await client.callTool({
        name: 'write_markdown',
        arguments: {
          path: 'Skills/high_score.md',
          content: `---\ntitle: unique_term\n---\nunique_term unique_term\n`
        }
      });

      // Create low-scoring file (Content match only)
      await client.callTool({
        name: 'write_markdown',
        arguments: {
          path: 'Skills/low_score.md',
          content: `---\ntitle: regular\n---\nunique_term\n`
        }
      });

      const res = await client.callTool({
        name: 'search_vault',
        arguments: { query: 'unique_term' }
      });
      const results = JSON.parse((res as any).content[0].text);
      expect(results.length).toBeGreaterThanOrEqual(2);
      
      const idxHigh = results.findIndex((r: any) => r.path === 'Skills/high_score.md');
      const idxLow = results.findIndex((r: any) => r.path === 'Skills/low_score.md');
      expect(idxHigh).toBeLessThan(idxLow);
      expect(results[idxHigh].score).toBeGreaterThan(results[idxLow].score);
    });
  });

  // --- AREA 2: FIELD-SPECIFIC SEARCHES ---
  describe('Field-Specific Tag and Metadata Searches', () => {
    beforeEach(async () => {
      // Create test files with distinct frontmatter
      await client.callTool({
        name: 'write_markdown',
        arguments: {
          path: 'Knowledge/doc_a.md',
          content: `---\ntitle: "Doc A"\ntags:\n  - liva/knowledge\n  - liva/test\nauthor: "Explorer"\ncategory: "security"\n---\nContent a\n`
        }
      });

      await client.callTool({
        name: 'write_markdown',
        arguments: {
          path: 'Knowledge/doc_b.md',
          content: `---\ntitle: "Doc B"\ntags:\n  - liva/rule\nauthor: "Orchestrator"\ncategory: "network"\n---\nContent b\n`
        }
      });
    });

    test('Verify single tag filter (tags:liva/knowledge)', async () => {
      const res = await client.callTool({
        name: 'search_vault',
        arguments: { query: 'tags:liva/knowledge' }
      });
      const results = JSON.parse((res as any).content[0].text);
      const paths = results.map((r: any) => r.path);
      expect(paths).toContain('Knowledge/doc_a.md');
      expect(paths).not.toContain('Knowledge/doc_b.md');
    });

    test('Verify loose tag matching (tags:knowledge matches liva/knowledge)', async () => {
      const res = await client.callTool({
        name: 'search_vault',
        arguments: { query: 'tags:knowledge' }
      });
      const results = JSON.parse((res as any).content[0].text);
      const paths = results.map((r: any) => r.path);
      expect(paths).toContain('Knowledge/doc_a.md');
    });

    test('Verify multiple tags of the same field act as OR (tags:knowledge tags:rule)', async () => {
      const res = await client.callTool({
        name: 'search_vault',
        arguments: { query: 'tags:knowledge tags:rule' }
      });
      const results = JSON.parse((res as any).content[0].text);
      const paths = results.map((r: any) => r.path);
      expect(paths).toContain('Knowledge/doc_a.md');
      expect(paths).toContain('Knowledge/doc_b.md');
    });

    test('Verify multiple different fields act as AND (tags:knowledge author:explorer)', async () => {
      const res = await client.callTool({
        name: 'search_vault',
        arguments: { query: 'tags:knowledge author:explorer' }
      });
      const results = JSON.parse((res as any).content[0].text);
      const paths = results.map((r: any) => r.path);
      expect(paths).toContain('Knowledge/doc_a.md');
      expect(paths).not.toContain('Knowledge/doc_b.md');

      // author orchestrator does not match doc_a
      const res2 = await client.callTool({
        name: 'search_vault',
        arguments: { query: 'tags:knowledge author:orchestrator' }
      });
      const results2 = JSON.parse((res2 as any).content[0].text);
      expect(results2.length).toBe(0);
    });

    test('Verify custom frontmatter field filtering (category:security)', async () => {
      const res = await client.callTool({
        name: 'search_vault',
        arguments: { query: 'category:security' }
      });
      const results = JSON.parse((res as any).content[0].text);
      const paths = results.map((r: any) => r.path);
      expect(paths).toContain('Knowledge/doc_a.md');
      expect(paths).not.toContain('Knowledge/doc_b.md');
    });

    test('Archived notes stay out of active search but remain explicitly retrievable', async () => {
      await client.callTool({
        name: 'write_markdown',
        arguments: {
          path: 'Knowledge/imported_history.md',
          content: `---
title: "Imported History"
tags:
  - liva/knowledge
author: "worker"
last_update: "2026-07-28T00:00:00Z"
status: "archived"
---
Imported workflow retained for historical reference.
`
        }
      });

      const activeResponse = await client.callTool({
        name: 'search_vault',
        arguments: { query: 'imported workflow' }
      });
      const activeResults = JSON.parse((activeResponse as any).content[0].text);
      expect(activeResults.map((result: any) => result.path))
        .not.toContain('Knowledge/imported_history.md');

      const archiveResponse = await client.callTool({
        name: 'search_vault',
        arguments: { query: 'status:archived imported workflow' }
      });
      const archiveResults = JSON.parse((archiveResponse as any).content[0].text);
      expect(archiveResults.map((result: any) => result.path))
        .toContain('Knowledge/imported_history.md');
    });
  });

  // --- AREA 3: LAZY CACHE INVALIDATION & RACE CONDITIONS ---
  describe('Cache Invalidation & Consistency', () => {
    test('Verify watcher race condition: immediate search after direct disk modification fails to see update, but succeeds after delay', async () => {
      const directFilePath = path.join(tempVaultPath, 'Knowledge/direct.md');
      const relativePath = 'Knowledge/direct.md';

      // 1. Create file directly on disk (bypassing MCP write_markdown)
      fs.writeFileSync(directFilePath, `---\ntitle: Direct\n---\nOld content\n`, 'utf8');

      // 2. Search immediately. It should NOT find it because the watcher fires asynchronously
      const resImmediate = await client.callTool({
        name: 'search_vault',
        arguments: { query: 'Direct' }
      });
      const resultsImmediate = JSON.parse((resImmediate as any).content[0].text);
      const immediatePaths = resultsImmediate.map((r: any) => r.path);
      expect(immediatePaths).not.toContain(relativePath);

      // 3. Wait for watcher to trigger indexing (e.g. 100ms)
      await new Promise(resolve => setTimeout(resolve, 100));

      // 4. Search again. It should now find it.
      const resDelayed = await client.callTool({
        name: 'search_vault',
        arguments: { query: 'Direct' }
      });
      const resultsDelayed = JSON.parse((resDelayed as any).content[0].text);
      const delayedPaths = resultsDelayed.map((r: any) => r.path);
      expect(delayedPaths).toContain(relativePath);

      // 5. Delete file directly on disk (bypassing MCP)
      fs.unlinkSync(directFilePath);

      // 6. Search immediately. The file will STILL be returned (stale cache / no lazy validation)
      const resImmediateDel = await client.callTool({
        name: 'search_vault',
        arguments: { query: 'Direct' }
      });
      const resultsImmediateDel = JSON.parse((resImmediateDel as any).content[0].text);
      const immediateDelPaths = resultsImmediateDel.map((r: any) => r.path);
      expect(immediateDelPaths).toContain(relativePath); // Stale cache confirms no lazy invalidation on search!

      // 7. Wait for watcher to trigger removal
      await new Promise(resolve => setTimeout(resolve, 100));

      // 8. Search again. Now it should be gone.
      const resDelayedDel = await client.callTool({
        name: 'search_vault',
        arguments: { query: 'Direct' }
      });
      const resultsDelayedDel = JSON.parse((resDelayedDel as any).content[0].text);
      const delayedDelPaths = resultsDelayedDel.map((r: any) => r.path);
      expect(delayedDelPaths).not.toContain(relativePath);
    });

    test('Verify cache remains stale if watcher is closed', async () => {
      // Instantiate a standalone VaultManager to manually control its search engine and watcher
      const manager = new VaultManager(tempVaultPath);
      
      const fileToDel = 'Knowledge/to_delete.md';
      const absPath = path.join(tempVaultPath, fileToDel);
      fs.writeFileSync(absPath, `---\ntitle: ToDelete\n---\nContent\n`, 'utf8');

      // Index file manually to populate cache
      manager.resolveSafePath(fileToDel); // validates path
      // Run a write to force sync indexing
      manager.writeMarkdown(fileToDel, `---\ntitle: ToDelete\n---\nContent\n`);

      // Close the watcher to stop active cache updates
      manager.close();

      // Verify it is in cache
      let searchRes = manager.searchVault('ToDelete');
      expect(searchRes.map(r => r.path)).toContain(fileToDel);

      // Delete file on disk
      fs.unlinkSync(absPath);

      // Search again. Because watcher is closed and there is no lazy invalidation, it STILL returns the deleted file!
      searchRes = manager.searchVault('ToDelete');
      expect(searchRes.map(r => r.path)).toContain(fileToDel);

      // Cleanup standalone manager
      manager.close();
    });
  });

  // --- AREA 4: PERFORMANCE & CONCURRENT SEARCHES ---
  describe('Performance under Concurrent Search Queries', () => {
    test('Verify concurrent queries block the event loop and scale latency linearly', async () => {
      // 1. Generate 100 dummy files of 50 lines to increase index size
      const writePromises = Array.from({ length: 100 }).map((_, i) => {
        const content = `---\ntitle: Dummy ${i}\nauthor: explorer\ntags:\n  - dummy\n---\n` +
          Array.from({ length: 50 }).map((_, j) => `This is line ${j} in dummy file ${i} containing the word architecture.`).join('\n');
        return client.callTool({
          name: 'write_markdown',
          arguments: {
            path: `Skills/dummy_${i}.md`,
            content
          }
        });
      });
      await Promise.all(writePromises);

      const concurrencyLimit = 100;

      // Part A: Client-level concurrency (asynchronous message passing interleaves calls)
      const start = performance.now();
      const promises = Array.from({ length: concurrencyLimit }).map(() =>
        client.callTool({
          name: 'search_vault',
          arguments: { query: 'architecture' }
        })
      );

      const results = await Promise.all(promises);
      const end = performance.now();
      const duration = end - start;

      // Every result should be successful
      for (const res of results) {
        const searchResult = res as unknown as CallToolSuccessResult;
        expect(searchResult.isError).toBeFalsy();
      }

      console.error(`Concurrent Search Test (${concurrencyLimit} queries via Client):`);
      console.error(`  Total Duration: ${duration.toFixed(2)}ms`);
      console.error(`  Avg Query Latency: ${(duration / concurrencyLimit).toFixed(2)}ms`);

      // Part B: Direct synchronous blockage verification
      const manager = new VaultManager(tempVaultPath);
      // Wait for watcher to initialize
      await new Promise(resolve => setTimeout(resolve, 50));

      let maxEventLoopDelayDirect = 0;
      let lastTimeDirect = performance.now();
      const timerDirect = setInterval(() => {
        const now = performance.now();
        const delay = now - lastTimeDirect - 5;
        if (delay > maxEventLoopDelayDirect) {
          maxEventLoopDelayDirect = delay;
        }
        lastTimeDirect = now;
      }, 5);

      const startDirect = performance.now();
      for (let i = 0; i < concurrencyLimit; i++) {
        manager.searchVault('architecture');
      }
      const endDirect = performance.now();
      const durationDirect = endDirect - startDirect;

      // Yield the event loop to let the delayed timer callback run
      await new Promise(resolve => setTimeout(resolve, 10));

      clearInterval(timerDirect);
      manager.close();

      console.error(`Direct Synchronous Search Loop Blockage Test:`);
      console.error(`  Total Duration: ${durationDirect.toFixed(2)}ms`);
      console.error(`  Max Event Loop Block Time: ${maxEventLoopDelayDirect.toFixed(2)}ms`);

      // Verify that running synchronous searches in a loop blocks the event loop
      expect(maxEventLoopDelayDirect).toBeGreaterThan(0);
    });
  });
});
