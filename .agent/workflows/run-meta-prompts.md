---
description: Execute all meta-optimization prompts to refine the project's core AI prompts based on the current codebase state.
---

1.  **Identify Meta Prompts**
    - List all markdown files in the `docs/prompts/_meta` directory.
    - These files are "Meta Prompts" that contain instructions for optimizing other specific prompts.
    - **Known meta prompts** (verify all are present):
        | Meta Prompt | Target Output |
        |:------------|:--------------|
        | `optimize-code-review.md` | `docs/prompts/code-review-prompt.md` |
        | `optimize-spring-cleaning.md` | `docs/prompts/spring-cleaning-prompt.md` |
        | `optimize-readme.md` | `docs/prompts/readme-generation-prompt.md` |
        | `optimize-architecture-review.md` | `docs/prompts/architecture-review.md` |

2.  **Execute Optimizations**
    - For each meta-prompt file found:
        1.  **Read**: Read the content of the meta-prompt.
        2.  **Execute**: Follow the instructions contained within the meta-prompt **strictly**.
            - This typically involves analyzing the current codebase (Structure, Tech Stack, etc.) and rewriting a target prompt file to be more context-aware.
            - Ensure you identify the correct target file mentioned in the meta-prompt (e.g., `docs/prompts/readme-generation-prompt.md`).
    - **Concurrency**: These optimizations are **independent** of each other. You should execute them in the most efficient manner possible.

3.  **Completion**
    - Once all meta-prompts have been executed and the target files updated, verify that the new prompts are saved correctly.
    - Confirm all target output files listed in step 1 were regenerated.
