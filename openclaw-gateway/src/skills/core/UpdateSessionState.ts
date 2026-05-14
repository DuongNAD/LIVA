import { SkillMetadata } from "../../types/Contracts";

export const update_session_state: SkillMetadata = {
  name: "update_session_state",
  description:
    "[SILENT] Update the working session state (SESSION-STATE.md) according to Write-Ahead Logging (WAL) principles. MUST use this skill TO SAVE THOUGHTS OR PLANS BEFORE responding to the user or executing long-term tasks. This prevents progress loss on restart.",
  isCoreSkill: true,
  parameters: {
    type: "object",
    properties: {
      intent: {
        type: "string",
        description: "Core goal of the current session (e.g., 'Analyze source code', 'Generate report').",
      },
      current_context: {
        type: "string",
        description: "Current context: Data being processed, errors encountered, or general progress.",
      },
      pending_tasks: {
        type: "array",
        items: {
          type: "string",
        },
        description: "List of specific tasks to do next.",
      },
    },
    required: ["intent", "current_context", "pending_tasks"],
  },
  execute: async (args: any) => {
    const { intent, current_context, pending_tasks } = args;

    if (!intent || !current_context || !Array.isArray(pending_tasks)) {
      return "Error: Invalid parameters. Must provide intent, current_context, and pending_tasks array.";
    }

    // Build the formatted markdown content
    let content = `# SESSION STATE\n\n`;
    content += `## Core Intent\n${intent}\n\n`;
    content += `## Current Context\n${current_context}\n\n`;
    content += `## Pending Tasks\n`;
    for (const task of pending_tasks) {
      content += `- [ ] ${task}\n`;
    }

    const memory = globalThis.kernelInstance?.memory;
    if (memory) {
      await memory.updateSessionState(content);
      return "✅ Session State saved successfully. System is safe to continue execution or respond to user.";
    } else {
      return "Error: MemoryManager not found.";
    }
  },
};
