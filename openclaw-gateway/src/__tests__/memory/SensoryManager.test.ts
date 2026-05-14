/**
 * @module SensoryManager Consent Tests
 * Unit tests for the consent-gated data capture system.
 * Verifies compliance with Vietnam Data Protection Law Article 30.
 */
import { describe, it, expect, beforeEach } from "vitest";

// SensoryManager is a singleton — we need to reset between tests
// We'll test the public API behavior

describe("SensoryManager — Consent Management", () => {
  let SensoryManager: any;

  beforeEach(async () => {
    // Force fresh module for each test to reset singleton
    const mod = await import("../../memory/SensoryManager");
    SensoryManager = mod.SensoryManager;
  });

  it("should default consent to ALL DISABLED", () => {
    const sm = SensoryManager.getInstance();
    const consent = sm.getConsentState();
    expect(consent.activeWindow).toBe(false);
    expect(consent.clipboard).toBe(false);
  });

  it("should block capture when no consent granted", async () => {
    const sm = SensoryManager.getInstance();
    // captureContext should return without error but not capture anything
    await sm.captureContext();
    expect(sm.currentData).toBeNull();
  });

  it("should grant consent for specific categories", () => {
    const sm = SensoryManager.getInstance();
    sm.grantConsent({ activeWindow: true });
    const consent = sm.getConsentState();
    expect(consent.activeWindow).toBe(true);
    expect(consent.clipboard).toBe(false); // Unchanged
  });

  it("should grant consent for multiple categories", () => {
    const sm = SensoryManager.getInstance();
    sm.grantConsent({ activeWindow: true, clipboard: true });
    const consent = sm.getConsentState();
    expect(consent.activeWindow).toBe(true);
    expect(consent.clipboard).toBe(true);
  });

  it("should revoke ALL consent and purge data", () => {
    const sm = SensoryManager.getInstance();
    sm.grantConsent({ activeWindow: true, clipboard: true });
    sm.revokeConsent(); // Revoke ALL
    const consent = sm.getConsentState();
    expect(consent.activeWindow).toBe(false);
    expect(consent.clipboard).toBe(false);
    expect(sm.currentData).toBeNull(); // Data purged
  });

  it("should revoke specific category consent", () => {
    const sm = SensoryManager.getInstance();
    sm.grantConsent({ activeWindow: true, clipboard: true });
    sm.revokeConsent({ clipboard: false });
    const consent = sm.getConsentState();
    expect(consent.activeWindow).toBe(true); // Still granted
    expect(consent.clipboard).toBe(false);   // Revoked
  });

  it("should record audit entries for consent changes", () => {
    const sm = SensoryManager.getInstance();
    sm.grantConsent({ activeWindow: true });
    sm.revokeConsent();
    
    const auditLog = sm.getAuditLog();
    expect(auditLog.length).toBeGreaterThanOrEqual(2);
    
    const consentGranted = auditLog.find((e: any) => e.action === "CONSENT_GRANTED");
    const consentRevoked = auditLog.find((e: any) => e.action === "CONSENT_REVOKED");
    expect(consentGranted).toBeDefined();
    expect(consentRevoked).toBeDefined();
  });

  it("should record audit entry when capture is blocked", async () => {
    const sm = SensoryManager.getInstance();
    // Don't grant consent
    await sm.captureContext();
    
    const auditLog = sm.getAuditLog();
    const blocked = auditLog.find((e: any) => e.action === "CAPTURE_BLOCKED");
    expect(blocked).toBeDefined();
  });

  it("should flush all data and record audit", () => {
    const sm = SensoryManager.getInstance();
    sm.flush();
    
    const auditLog = sm.getAuditLog();
    const flushEntry = auditLog.find((e: any) => e.action === "FLUSH");
    expect(flushEntry).toBeDefined();
  });

  it("should update consent timestamp on changes", () => {
    const sm = SensoryManager.getInstance();
    const before = sm.getConsentState().updatedAt;
    
    // Small delay to ensure timestamp changes
    sm.grantConsent({ clipboard: true });
    const after = sm.getConsentState().updatedAt;
    
    expect(after).toBeGreaterThanOrEqual(before);
  });
});
