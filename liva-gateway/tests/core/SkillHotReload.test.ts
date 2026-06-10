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

const tempSkillPath = path.resolve(process.cwd(), 'src', 'skills', 'TempTestSkill.ts');

describe('Skill Hot-Reload Integration', () => {
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

    it('should register, reload, and unlink a dynamic skill correctly', async () => {
        // 1. Initially, temp_test_skill should not exist in the registry
        let skills = registry.getAllSkills();
        expect(skills.find(s => s.name === 'temp_test_skill')).toBeUndefined();

        // 2. Create the temporary skill file
        const skillCodeV1 = `
export const metadata = {
  name: "temp_test_skill",
  search_keywords: ["temp_test_skill"],
  description: "Temporary test skill description V1",
  parameters: {
    type: "object",
    properties: {
      msg: { type: "string" }
    },
    required: ["msg"]
  }
};

export const execute = async (args: any) => {
  return "V1: " + args.msg;
};
`;
        await fs.writeFile(tempSkillPath, skillCodeV1, 'utf8');

        // 3. Trigger reloadLocalSkill with event 'add'
        await registry.reloadLocalSkill(tempSkillPath, 'add');

        // Verify it was added
        skills = registry.getAllSkills();
        let testSkill = skills.find(s => s.name === 'temp_test_skill');
        expect(testSkill).toBeDefined();
        expect(testSkill?.description).toBe("Temporary test skill description V1");

        // Execute it to see if it works
        const resultV1 = await registry.executeSkill('temp_test_skill', { msg: 'hello' });
        expect(resultV1).toBe('V1: hello');

        // 4. Update the temporary skill file (V2)
        const skillCodeV2 = `
export const metadata = {
  name: "temp_test_skill",
  search_keywords: ["temp_test_skill"],
  description: "Temporary test skill description V2",
  parameters: {
    type: "object",
    properties: {
      msg: { type: "string" }
    },
    required: ["msg"]
  }
};

export const execute = async (args: any) => {
  return "V2: " + args.msg;
};
`;
        await fs.writeFile(tempSkillPath, skillCodeV2, 'utf8');

        // Trigger reloadLocalSkill with event 'change'
        await registry.reloadLocalSkill(tempSkillPath, 'change');

        // Verify description and execution are updated
        skills = registry.getAllSkills();
        testSkill = skills.find(s => s.name === 'temp_test_skill');
        expect(testSkill).toBeDefined();
        expect(testSkill?.description).toBe("Temporary test skill description V2");

        const resultV2 = await registry.executeSkill('temp_test_skill', { msg: 'hello' });
        expect(resultV2).toBe('V2: hello');

        // 5. Test renaming the skill (V3)
        const skillCodeV3 = `
export const metadata = {
  name: "temp_test_skill_renamed",
  search_keywords: ["temp_test_skill_renamed"],
  description: "Renamed temporary test skill description",
  parameters: {
    type: "object",
    properties: {
      msg: { type: "string" }
    },
    required: ["msg"]
  }
};

export const execute = async (args: any) => {
  return "V3: " + args.msg;
};
`;
        await fs.writeFile(tempSkillPath, skillCodeV3, 'utf8');

        // Trigger reloadLocalSkill with event 'change'
        await registry.reloadLocalSkill(tempSkillPath, 'change');

        // Verify old name is gone and new name is registered
        skills = registry.getAllSkills();
        expect(skills.find(s => s.name === 'temp_test_skill')).toBeUndefined();

        let renamedSkill = skills.find(s => s.name === 'temp_test_skill_renamed');
        expect(renamedSkill).toBeDefined();
        expect(renamedSkill?.description).toBe("Renamed temporary test skill description");

        // 6. Test unlinking (deleting) the skill
        await fs.unlink(tempSkillPath);

        // Trigger reloadLocalSkill with event 'unlink'
        await registry.reloadLocalSkill(tempSkillPath, 'unlink');

        // Verify the skill is completely removed
        skills = registry.getAllSkills();
        expect(skills.find(s => s.name === 'temp_test_skill_renamed')).toBeUndefined();
    });
});
