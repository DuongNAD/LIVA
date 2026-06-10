import * as fs from 'node:fs/promises';
import * as path from 'node:path';
import { describe, it, expect, beforeEach, afterAll, vi } from 'vitest';
import { SkillRegistry } from '../../src/SkillRegistry';

vi.mock('../../src/services/EmbeddingService', () => {
    const mockEmbedSvc = {
        embedWithTimeout: vi.fn().mockResolvedValue([0.1, 0.2]),
        embed: vi.fn().mockResolvedValue([0.1, 0.2]),
        embedBatch: vi.fn().mockResolvedValue([[0.1, 0.2]]),
        ensureReady: vi.fn().mockResolvedValue(undefined),
        ready: true,
        isVramYielded: vi.fn().mockReturnValue(false)
    };
    return {
        EmbeddingService: {
            getInstance: vi.fn().mockReturnValue(mockEmbedSvc)
        }
    };
});

vi.mock('../../src/utils/logger', () => ({
    logger: {
        info: vi.fn(),
        warn: vi.fn(),
        error: vi.fn(),
        debug: vi.fn()
    }
}));

const tempSkillPath = path.resolve(process.cwd(), 'src', 'skills', 'TempStressSkill.ts');

describe('Skill Hot-Reload Stress & Leak Verification', () => {
    let registry: SkillRegistry;

    beforeEach(async () => {
        registry = new SkillRegistry();
        await registry.registerLocalSkills();
    });

    afterAll(async () => {
        try {
            await fs.unlink(tempSkillPath);
        } catch {}
    });

    it('should handle repeated reloads without hanging or exceeding listener limits', async () => {
        const startMem = process.memoryUsage().heapUsed;
        const initialListeners = process.listenerCount('warning') + process.listenerCount('uncaughtException');

        // Loop 50 times to simulate high-frequency reloading
        const iterations = 50;
        for (let i = 0; i < iterations; i++) {
            const skillCode = `
export const metadata = {
  name: "temp_stress_skill",
  search_keywords: ["temp_stress_skill"],
  description: "Temporary stress skill description V${i}",
  parameters: {
    type: "object",
    properties: {
      msg: { type: "string" }
    },
    required: ["msg"]
  }
};

export const execute = async (args: any) => {
  return "V${i}: " + args.msg;
};
`;
            await fs.writeFile(tempSkillPath, skillCode, 'utf8');

            const event = i === 0 ? 'add' : 'change';
            await registry.reloadLocalSkill(tempSkillPath, event);

            // Execute the skill to verify it works fine at each step
            const result = await registry.executeSkill('temp_stress_skill', { msg: 'test' });
            expect(result).toBe(`V${i}: test`);
        }

        // Cleanup
        await fs.unlink(tempSkillPath);
        await registry.reloadLocalSkill(tempSkillPath, 'unlink');

        // Assert skills list is clean of the stress skill
        const skills = registry.getAllSkills();
        expect(skills.find(s => s.name === 'temp_stress_skill')).toBeUndefined();

        const endMem = process.memoryUsage().heapUsed;
        const finalListeners = process.listenerCount('warning') + process.listenerCount('uncaughtException');

        // Check if there was any listener leak warning or if listener count ballooned
        expect(finalListeners).toBeLessThanOrEqual(initialListeners + 2); // Allow small buffer
        
        console.log(`Stress reload test completed successfully.`);
        console.log(`Heap usage delta: ${((endMem - startMem) / 1024 / 1024).toFixed(2)} MB`);
    });
});
