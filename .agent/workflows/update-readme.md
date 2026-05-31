---
description: Regenerate the project README.md using AI analysis
---

# Update README Workflow

1. **Read the README Generation Prompt**:
   Review the instructions in `docs/prompts/readme-generation-prompt.md`.

2. **Analyze the Current Codebase**:
   Scan the workspace to gather:
   - Project name and description from `package.json`
   - Tech stack from dependencies
   - Directory structure
   - Available scripts
   - Environment variable requirements
   - Mobile/hybrid app presence (Capacitor config, Android project, push notifications, deep linking)
   - Modular 2.0 skills inside `.skills/` directory and configured MCP context servers

3. **Generate the README**:
   Create a comprehensive `README.md` following the prompt template.
   Include all required sections: Header, About, Tech Stack, Getting Started, Features.
   - Always include a **Developer Context & Runbooks** section documenting modular `.skills/` runbooks, active context servers (MCP), and onboarding using the **2.0 Execution Environments Matrix** (IDE vs. App vs. CLI).
   - If a hybrid mobile app exists (e.g., Capacitor `android/` directory), include a **Mobile App** section covering:
     - Architecture overview (hybrid approach, shared codebase)
     - Key mobile features (push notifications, in-app browser, deep linking)
     - Build instructions and prerequisites
     - Links to developer guide (`docs/guides/android-app-development.md`)
     - CI/CD pipeline reference (`.github/workflows/build-android.yml`)
     - App store listing guide reference (`docs/guides/google-play-listing.md`)

4. **Replace README.md**:
   Overwrite the `README.md` file in the project root.

5. **Confirm Completion**:
   Show the user a summary of the updated README structure.
