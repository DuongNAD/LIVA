import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { safeFetch } from "../../src/utils/fetch";

describe("safeFetch", () => {
  beforeEach(() => {
    vi.stubGlobal("fetch", vi.fn());
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("should perform fetch successfully and return response", async () => {
    const mockResponse = new Response("ok", { status: 200 });
    vi.mocked(fetch).mockResolvedValueOnce(mockResponse);

    const res = await safeFetch("http://localhost:8080/api");
    expect(fetch).toHaveBeenCalledWith("http://localhost:8080/api", expect.any(Object));
    expect(res).toBe(mockResponse);
  });

  it("should handle timeout abort", async () => {
    vi.useFakeTimers();
    vi.mocked(fetch).mockImplementationOnce((_input, init) => {
      return new Promise((resolve, reject) => {
        if (init?.signal) {
          init.signal.addEventListener("abort", () => {
            reject(new DOMException("The user aborted a request.", "AbortError"));
          });
        }
      });
    });

    const fetchPromise = safeFetch("http://localhost:8080/api", {}, 100);
    
    vi.advanceTimersByTime(100);
    
    await expect(fetchPromise).rejects.toThrow("aborted");
    vi.useRealTimers();
  });
});
