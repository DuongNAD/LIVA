import { describe, it, expect, afterEach } from "vitest";
import { MCPHostClient } from "../../src/mcp/MCPHostClient";
import * as path from "node:path";

describe("MCPHostClient Integration Tests", () => {
    let client: MCPHostClient | null = null;

    afterEach(async () => {
        if (client) {
            await client.disconnect();
            client = null;
        }
    });

    it("should connect, list remote tools, execute a tool, and register as agent skills", async () => {
        const tsxPath = path.resolve(process.cwd(), "../node_modules/tsx/dist/cli.mjs");
        const mockServerPath = path.resolve(process.cwd(), "tests/mcp/mockServer.ts");

        client = new MCPHostClient({
            id: "test-mcp-server",
            name: "Test-MCP-Client",
            transportType: "stdio",
            stdioConfig: {
                command: "node",
                args: [tsxPath, mockServerPath]
            }
        });

        // 1. Connect
        await client.connect();

        // 2. List Remote Tools
        const tools = await client.listRemoteTools();
        expect(tools).toBeDefined();
        expect(tools.length).toBeGreaterThan(0);
        
        const echoTool = tools.find(t => t.name === "echo");
        expect(echoTool).toBeDefined();
        expect(echoTool?.description).toBe("Echoes back the message");

        // 3. Execute Remote Tool
        const executionResult = await client.executeRemoteTool("echo", { message: "Hello World" });
        expect(executionResult).toBeDefined();
        expect(executionResult.content).toBeDefined();
        expect(executionResult.content[0].text).toBe("Echo: Hello World");

        // 4. Register As Agent Skills
        const skills = await client.registerAsAgentSkills();
        expect(skills).toBeDefined();
        expect(skills.length).toBeGreaterThan(0);

        const echoSkill = skills.find(s => s.name === "echo");
        expect(echoSkill).toBeDefined();
        expect(echoSkill?.description).toBe("Echoes back the message");
        expect(echoSkill?.short_desc).toBe("Echoes back the message");
        expect(echoSkill?.execute).toBeDefined();

        if (echoSkill && echoSkill.execute) {
            const skillResult = await echoSkill.execute({ message: "Skill Message" });
            expect(skillResult).toBeDefined();
            expect(skillResult.content[0].text).toBe("Echo: Skill Message");
        }
    });
});
