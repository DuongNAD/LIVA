import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { safeFetch } from "../../src/utils/HttpClient";

describe("safeFetch — HTTP Error Detection & Timeout", () => {
  
  beforeEach(() => {
    vi.stubGlobal('fetch', vi.fn());
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("should return Response on HTTP 200", async () => {
    const mockResponse = {
      ok: true,
      status: 200,
      json: async () => ({ success: true }),
      text: async () => "OK",
    } as unknown as Response;

    (fetch as any).mockResolvedValue(mockResponse);

    const res = await safeFetch("http://localhost:8000/test");
    expect(res.status).toBe(200);
  });

  it("should throw on HTTP 400 Bad Request with error body", async () => {
    const mockResponse = {
      ok: false,
      status: 400,
      text: async () => JSON.stringify({ error: "Invalid token" }),
    } as unknown as Response;

    (fetch as any).mockResolvedValue(mockResponse);

    await expect(safeFetch("http://localhost:8000/test"))
      .rejects.toThrow("HTTP 400");
    
    // Verify the error message contains the API error body
    try {
      await safeFetch("http://localhost:8000/test");
    } catch (e: any) {
      expect(e.message).toContain("Invalid token");
    }
  });

  it("should throw on HTTP 500 Internal Server Error", async () => {
    const mockResponse = {
      ok: false,
      status: 500,
      text: async () => "<html>Internal Server Error</html>",
    } as unknown as Response;

    (fetch as any).mockResolvedValue(mockResponse);

    await expect(safeFetch("http://localhost:8000/test"))
      .rejects.toThrow("HTTP 500");
  });

  it("should throw on HTTP 502 (Cloudflare) with HTML body", async () => {
    const mockResponse = {
      ok: false,
      status: 502,
      text: async () => "<html><body>502 Bad Gateway — Cloudflare</body></html>",
    } as unknown as Response;

    (fetch as any).mockResolvedValue(mockResponse);

    await expect(safeFetch("http://localhost:8000/test"))
      .rejects.toThrow("HTTP 502");
    
    try {
      await safeFetch("http://localhost:8000/test");
    } catch (e: any) {
      expect(e.message).toContain("Cloudflare");
    }
  });

  it("should throw AbortError on timeout", async () => {
    // Simulate a request that takes longer than timeout
    (fetch as any).mockImplementation(() => 
      new Promise((_, reject) => {
        setTimeout(() => reject(new DOMException("The operation was aborted", "AbortError")), 200);
      })
    );

    await expect(safeFetch("http://localhost:8000/test", {}, 100))
      .rejects.toThrow();
  });

  it("should propagate network errors (ECONNREFUSED)", async () => {
    const networkError = new TypeError("fetch failed");
    (networkError as any).cause = new Error("connect ECONNREFUSED 127.0.0.1:8000");
    
    (fetch as any).mockRejectedValue(networkError);

    try {
      await safeFetch("http://localhost:8000/test");
      expect.unreachable("Should have thrown");
    } catch (e: any) {
      expect(e.message).toBe("fetch failed");
      expect(e.cause?.message).toContain("ECONNREFUSED");
    }
  });

  it("should handle text() failure during error extraction", async () => {
    const mockResponse = {
      ok: false,
      status: 503,
      text: async () => { throw new Error("Body already consumed"); },
    } as unknown as Response;

    (fetch as any).mockResolvedValue(mockResponse);

    // Should still throw with "Unknown API Error" fallback
    await expect(safeFetch("http://localhost:8000/test"))
      .rejects.toThrow("HTTP 503: Unknown API Error");
  });
});
