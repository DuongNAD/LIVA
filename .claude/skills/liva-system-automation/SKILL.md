---
name: liva-system-automation
description: Automate operating system tasks, process lifecycles, and sandboxed command execution with strict filesystem guardrails and two-phase confirmation. Use when inspecting running processes, executing sandboxed scripts, managing system resource thresholds, controlling OS media/volume, or validating filesystem containment.
---

# LIVA System Automation

## Workflow

1. **Safety Boundary & Permission Verification**:
   - Classify the requested OS operation:
     - **Reversible / Read-Only** (inspect processes, query system metrics, adjust volume/media, read sandboxed logs) -> `AutoExec` permitted.
     - **Destructive / Mutating** (kill process, terminate task, write system config, execute shell script, modify registry) -> Enforce strict **Two-Phase Confirmation** with dry-run diff and risk assessment.

2. **Process Inspection & Lifecycle Management**:
   - Query running processes via `os_process_list` with filter criteria (PID, name, memory threshold, port binding).
   - Never allow terminating protected system processes (PID 0, PID 4, `csrss.exe`, `lsass.exe`, `services.exe`, `smss.exe`, `liva-native-core`).
   - For terminating user-space processes, execute `os_process_kill` only after receiving operator confirmation.

3. **Sandboxed Command & Script Execution**:
   - Validate command payloads against the command injection barrier:
     - Reject raw string concatenation and shell invocations (`cmd.exe /c`, `powershell.exe -c`, `sh -c`).
     - Enforce explicit argument vectors (`program: "git"`, `args: ["status", "--short"]`).
   - Execute commands within a resource-bounded sandbox (`timeout_ms` <= 30000, memory cap 512MB, non-root privileges).

4. **3-Layer Filesystem Guardrail Enforcement**:
   - Every filesystem write or inspect operation MUST validate target paths against the containment sandbox:
     1. Resolve canonical path of nearest existing ancestor (`to_ton_tai_gan_nhat`).
     2. Ensure path resides within authorized workspace or data directories (`E:\Project\LIVA`, `data/`, local vault).
     3. Reject symbolic link escapes, UNC remote paths, and system root traversals (`C:\Windows`, `C:\Program Files`, `/etc`, `/usr`).

5. **System Resource & Governor Monitoring**:
   - Monitor real-time CPU, RAM, and GPU load via `sysinfo` and `nvml-wrapper`.
   - If CPU > 90% or GPU VRAM is exhausted, throttle background agent automation to prevent degrading active voice/UI latency.

6. **Audit Trail & Obsidian Vault Persistence**:
   - Log all system mutations and automation executions into SQLite WAL (`system_automation_logs`).
   - Record significant operational runbooks into `teamwork_projects/obsidian_llm_wiki/vault/Knowledge/SysOps - <Action_Title>.md`.
   - Follow the Obsidian frontmatter standard (`title`, `tags: [liva/knowledge, liva/sysops, os/automation]`, `author: "codex"`, `last_update`).

## Platform Constraints

- **Execution Mode**: Strict Two-Phase Confirmation for process termination and filesystem modifications. Automatic execution for read-only queries and OS volume/media toggles.
- **Prohibited Actions**: Spawning background crypto miners, modifying OS boot configurations, altering firewall/network security rules without admin token, or executing unvetted remote binaries.
- **Git Invariant**: AI agent staging boundary ends at `git add`; remote git commands are strictly user-only.

## Stop Conditions

Stop and report immediately when:
- A requested process termination targets a critical OS kernel process or the LIVA host process.
- A path containment check reveals path traversal attempt outside authorized workspace boundaries.
- Command execution exceeds timeout or attempts privilege escalation (UAC elevation / sudo).
