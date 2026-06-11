import { promises as fs } from 'node:fs';
import * as path from 'node:path';

export default async function () {
    // This function runs once before all tests.
    
    // Return a teardown function that runs once after all tests complete.
    return async () => {
        const agentsDir = path.join(process.cwd(), 'data', 'agents');
        try {
            const files = await fs.readdir(agentsDir);
            for (const file of files) {
                if (
                    file.startsWith('__test_') ||
                    file.startsWith('stress_test_agent_') ||
                    file.startsWith('__brutal_stress_test_') ||
                    file.startsWith('hmem_v18_test_agent_') ||
                    file === 'timer_test' ||
                    file === 'test-agent'
                ) {
                    const dirPath = path.join(agentsDir, file);
                    await fs.rm(dirPath, { recursive: true, force: true }).catch(() => {});
                }
            }
        } catch (e) {
            // Directory data/agents might not exist or be empty, ignore
        }
    };
}
