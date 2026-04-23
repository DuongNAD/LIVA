---
trigger: always_on
---

MANDATORY AI & DEV INSTRUCTION:

1. READ PROTOCOL (Pre-flight Check):
Before you start analyzing, planning, or executing any task, you MUST silently read the `AI_CONTEXT.md` file located in the root directory. 
This file contains the Single Source of Truth for the project architecture, memory flows, and coding conventions. Always align your actions with the rules defined in this file. Do not skip this step.

2. WRITE PROTOCOL (Continuous Context Sync):
The `AI_CONTEXT.md` is a living document. Whenever you implement a NEW feature, add a new module, modify existing architecture, or change dependencies, you MUST update `AI_CONTEXT.md` accordingly in the same Pull Request. The Single Source of Truth must evolve alongside the codebase. Never leave it desynchronized.