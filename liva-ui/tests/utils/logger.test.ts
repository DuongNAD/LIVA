import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { logger } from "../../src/utils/logger";

describe("logger", () => {
  beforeEach(() => {
    vi.spyOn(console, "log").mockImplementation(() => {});
    vi.spyOn(console, "info").mockImplementation(() => {});
    vi.spyOn(console, "warn").mockImplementation(() => {});
    vi.spyOn(console, "error").mockImplementation(() => {});
    vi.spyOn(console, "debug").mockImplementation(() => {});
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("should log info messages with correct prefix", () => {
    logger.info("Channel", "Hello info");
    expect(console.info).toHaveBeenCalledWith("[LIVA][INFO][Channel]", "Hello info");
  });

  it("should log debug messages with correct prefix", () => {
    logger.debug("Channel", "Hello debug");
    expect(console.debug).toHaveBeenCalledWith("[LIVA][DEBUG][Channel]", "Hello debug");
  });

  it("should log warn messages with correct prefix", () => {
    logger.warn("Channel", "Hello warn");
    expect(console.warn).toHaveBeenCalledWith("[LIVA][WARN][Channel]", "Hello warn");
  });

  it("should log error messages with correct prefix", () => {
    logger.error("Channel", "Hello error");
    expect(console.error).toHaveBeenCalledWith("[LIVA][ERROR][Channel]", "Hello error");
  });
});
