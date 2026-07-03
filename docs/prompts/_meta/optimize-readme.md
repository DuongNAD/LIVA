# Meta-Prompt: Optimize README Prompt
Target Output: `docs/prompts/readme-generation-prompt.md`

You are the prompt architect for the LIVA system. Your task is to generate and optimize the `readme-generation-prompt.md` file. This generated prompt will be used by other AI agents to regenerate the project's main `README.md`.

## Instructions for Writing the Output Prompt

The generated `readme-generation-prompt.md` must instruct the AI agent to:

1. **Extract Architecture & Features**:
   - Parse `AGENTS.md` and the Obsidian vault (`teamwork_projects/obsidian_llm_wiki/vault/`) to extract the system overview, architecture boundaries, and platform support. (`AI_CONTEXT.md` is archived at `docs/archive/AI_CONTEXT.md`.)
   - Describe the four pillars of hardware optimization (Preemptive VRAM Yielding, Semantic Cache L0.5, On-Demand Screen Awareness, Wake-Word Edge Offloading).

2. **Map Codebase Directory**:
   - Provide a high-level mapping of directories (`liva-gateway/src`, `liva-ui/src`, etc.).
   - Explain the core components: AgentLoop, CoreKernel, ModelOrchestrator, and LIVA-UHM (HiGMem).

3. **Incorporate Visual Mermaid Diagrams**:
   - Write clear Mermaid diagrams (System Overview flow, Sequence message flow, H-MEM layering).

4. **Add Setup & CLI Commands**:
   - Provide setup instructions (`npm run setup`, `npm run dev`, `npm run test:gateway`).
   - Mention the sequential hot-swap architecture and dynamic WS handshake setup.
   - Document standard GitNexus commands (`npx gitnexus analyze`, `npx gitnexus analyze --embeddings`).

5. **Style Guidelines**:
   - Use clean, premium markdown styling, clear alerts (`> [!IMPORTANT]`, etc.), and proper tables.
