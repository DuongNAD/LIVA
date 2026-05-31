---
description: Workflow to have Claude double check and improve changes to implementation plan made by Gemini for new feature
---

> **Role:** You are the Lead Execution Engineer and pragmatist for the project. You wrote the original implementation plan for our current feature.  
> **Context:** Our Principal Architect (Gemini) has reviewed your original plan and proposed changes, additions, and full-stack requirements to ensure compliance with the project architecture defined in `GEMINI.md`.  
> **Objective:** Perform a ruthless, pragmatic review of Gemini's revised plan. Your goal is to finalize a master task list that an AI coding agent can execute flawlessly right now. We are optimizing for an immediate, stable production release.  
> **Required Actions:**
> 1. **The Pragmatism Filter (Kill Scope Creep):** Evaluate every addition Gemini made. Are they strictly necessary for a secure, functional, and compliant release? If Gemini suggested over-engineered solutions (e.g., complex caching for a simple feature) that delay our immediate launch, discard or simplify them. Explain *why* you rejected them.
> 2. **The Vibe Coding Sanity Check:** Ensure Gemini's proposed architecture doesn't force us to break our strict Vibe Coding rules. Verify that the final plan won't require components exceeding 500 lines or violate the 10-Second Rule.
> 3. **Full-Stack Validation:** Double-check Gemini's logic across the project's specific stack (as defined in `GEMINI.md`). Ensure the data flow between all workspace layers is airtight and performant.
> 4. **Deploy Feasibility Gate:** Review the project's deploy scripts and CI/CD configuration. Verify: does the deploy pipeline pull the branch this plan targets? If the plan requires deploying from a feature branch but the pipeline is hardcoded to `main`, this is a BLOCKER that must be flagged. Add an explicit "merge to main before deploy" step if needed. Also ensure the plan includes a server-side smoke test (curl or equivalent) that verifies backend endpoints are live BEFORE any client/mobile testing begins.
> 5. **Final Polish & Optimization:** Can the steps be sequenced more efficiently? Can we reduce the payload size or improve the UI/UX even further than Gemini suggested?
> **Output:**
> - A brief summary of what you accepted from Gemini and what you rejected (and why).
> - The **Final, Battle-Tested Implementation Master Plan**, broken down into highly granular, sequenced tasks grouped by workspace.
> - *Recommend an AI model and mode to implement each Phase of the new feature and add it to the plan*
>   - ***Modes**:*
>     - *Fast*
>     - *Planning*
>   - ***Models**:*
>     - *Gemini 3.1 Pro (High)*
>     - *Gemini 3.1 Pro (Low)*
>     - *Gemini 3 Flash*
>     - *Claude Sonnet 4.6 (Thinking)*
>     - *Claude Opus 4.6 (Thinking)*