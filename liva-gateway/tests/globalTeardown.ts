import * as fs from 'node:fs';
import * as path from 'node:path';

export default async function globalTeardown() {
  const agentsDir = path.join(process.cwd(), 'data', 'agents');
  if (fs.existsSync(agentsDir)) {
    try {
      const files = fs.readdirSync(agentsDir);
      for (const file of files) {
        if (
          file.startsWith('test') ||
          file.startsWith('e2e') ||
          file.includes('async') ||
          file === 'benchmark_agent_liva_brutal'
        ) {
          const filePath = path.join(agentsDir, file);
          fs.rmSync(filePath, { recursive: true, force: true });
        }
      }
    } catch (err) {
      console.error('Failed to cleanup data/agents during teardown:', err);
    }
  }
}
