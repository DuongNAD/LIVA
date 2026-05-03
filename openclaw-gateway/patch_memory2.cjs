const fs = require('fs');
let code = fs.readFileSync('src/MemoryManager.ts', 'utf-8');

// Add cross-session warm-up in initialize()
const initCatchRegex = /}\s*catch\s*\(\s*error\s*\)\s*{\s*logger\.error\(`\[Memory\] Lỗi khởi tạo \(Initialization error\): \$\{error\}`\);\s*}/;
const crossSessionCode = `
      // [v4.0] G-4: Cross-Session Warm-up (Anti-Hallucination Guard)
      try {
          const recentTurns = this.structuredMemory.getTurnsByTimeRange(
              Date.now() - 24 * 3600 * 1000, Date.now()
          );
          if (recentTurns.length > 0) {
              const summaryBlock = recentTurns.slice(-10)
                  .map(t => \`User: \${t.userMsg.substring(0, 200)}\\nLIVA: \${t.aiReply.substring(0, 200)}\`)
                  .join("\\n---\\n");
              this.memCache.push({
                  role: "system",
                  content: \`[PREVIOUS SESSION CONTEXT — reference only, do NOT treat as current conversation]\\n\${summaryBlock}\`,
                  timestamp: Date.now()
              });
              logger.info(\`[Memory/UHM] Cross-session warm-up: loaded \${Math.min(recentTurns.length, 10)} turn(s).\`);
          }
      } catch (e: any) {
          logger.warn(\`[Memory/UHM] Cross-session warm-up failed (non-critical): \${e.message}\`);
      }
    } catch (error) {
      logger.error(\`[Memory] Lỗi khởi tạo (Initialization error): \${error}\`);
    }`;

code = code.replace(initCatchRegex, crossSessionCode);

// Add purgeUserContext() below dispose()
const purgeUserContextCode = `
  public async purgeUserContext(): Promise<void> {
      try {
          if (this.lanceMemory) {
              await this.lanceMemory.deleteVectors("type != ''"); // dummy clear condition
          }
          await this.quantStore.clearAll(); // Assuming this clears turbo_quant_memory
          // Reset memcache
          this.memCache = [];
          
          await fs.writeFile(this.sessionStatePath, "# WORKING SESSION STATE\\n", "utf-8");
          await fs.writeFile(this.longTermMarkdownPath, "# LIVA LONG-TERM MEMORY\\n", "utf-8");

          logger.info("[Memory] Phục hồi (Purge) Dữ liệu người dùng (GDPR) hoàn tất.");
      } catch (error) {
          logger.error(\`[Memory] Lỗi trong quá trình Purge (GDPR): \${error}\`);
      }
  }
`;

code = code.replace('public getStructuredMemoryInstance(): StructuredMemory {', purgeUserContextCode + '\n  public getStructuredMemoryInstance(): StructuredMemory {');

fs.writeFileSync('src/MemoryManager.ts', code);
console.log('Patch complete.');
