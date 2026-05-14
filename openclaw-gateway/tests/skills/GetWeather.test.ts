/**
 * GetWeather.test.ts — Weather Forecast Skill Unit Tests
 * =======================================================
 * Tests: location detection, geocoding, weather API, error handling.
 * All network calls are MOCKED via safeFetch.
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("../../src/utils/logger", () => ({
    logger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() },
}));

// Mock safeFetch to intercept all HTTP calls
const mockSafeFetch = vi.fn();
vi.mock("../../src/utils/HttpClient", () => ({
    safeFetch: (...args: any[]) => mockSafeFetch(...args),
}));

import * as GetWeather from "../../src/skills/core/GetWeather";

describe("GetWeather Skill", () => {
    beforeEach(() => {
        vi.clearAllMocks();
    });

    describe("metadata", () => {
        it("should have correct skill name", () => {
            expect(GetWeather.metadata.name).toBe("get_weather_forecast");
        });

        it("should not require any parameters", () => {
            expect(GetWeather.metadata.parameters.required).toEqual([]);
        });
    });

    describe("execute — Known locations (hardcoded coords)", () => {
        it("should return weather for 'Hà Nội'", async () => {
            // Mock weather API response
            mockSafeFetch.mockResolvedValueOnce({
                json: async () => ({
                    current: {
                        temperature_2m: 32,
                        relative_humidity_2m: 75,
                        weather_code: 2,
                    },
                }),
            });

            const result = await GetWeather.execute({ location: "Hà Nội" });
            expect(result).toContain("Hà Nội");
            expect(result).toContain("32");
            expect(result).toContain("75");
            expect(result).toContain("Partly cloudy");
        });

        it("should return weather for 'HCM'", async () => {
            mockSafeFetch.mockResolvedValueOnce({
                json: async () => ({
                    current: {
                        temperature_2m: 35,
                        relative_humidity_2m: 60,
                        weather_code: 0,
                    },
                }),
            });

            const result = await GetWeather.execute({ location: "hcm" });
            expect(result).toContain("35");
            expect(result).toContain("Clear sky");
        });
    });

    describe("execute — Mars easter egg", () => {
        it("should return fun message for Mars", async () => {
            const result = await GetWeather.execute({ location: "mars" });
            expect(result).toContain("Mars");
            expect(result).toContain("-125°C");
        });

        it("should handle Vietnamese 'sao hỏa'", async () => {
            const result = await GetWeather.execute({ location: "sao hỏa" });
            expect(result).toContain("Mars");
        });
    });

    describe("execute — Unknown location with geocoding", () => {
        it("should use geocoding API for unknown locations", async () => {
            // Mock geocoding response
            mockSafeFetch.mockResolvedValueOnce({
                json: async () => ({
                    results: [{
                        name: "Paris",
                        country: "France",
                        latitude: 48.8566,
                        longitude: 2.3522,
                    }],
                }),
            });

            // Mock weather response
            mockSafeFetch.mockResolvedValueOnce({
                json: async () => ({
                    current: {
                        temperature_2m: 18,
                        relative_humidity_2m: 65,
                        weather_code: 3,
                    },
                }),
            });

            const result = await GetWeather.execute({ location: "Paris" });
            expect(result).toContain("Paris");
            expect(result).toContain("18");
        });

        it("should handle geocoding not found", async () => {
            mockSafeFetch.mockResolvedValueOnce({
                json: async () => ({ results: [] }),
            });

            const result = await GetWeather.execute({ location: "XyzNotAPlace" });
            expect(result).toContain("Unable to geolocate");
        });
    });

    describe("execute — Auto-detect location via IP", () => {
        it("should auto-detect when no location provided", async () => {
            // Mock IP geolocation
            mockSafeFetch.mockResolvedValueOnce({
                json: async () => ({
                    status: "success",
                    city: "Ho Chi Minh City",
                    regionName: "Ho Chi Minh",
                    lat: 10.8231,
                    lon: 106.6297,
                }),
            });

            // Mock weather API
            mockSafeFetch.mockResolvedValueOnce({
                json: async () => ({
                    current: {
                        temperature_2m: 33,
                        relative_humidity_2m: 70,
                        weather_code: 61,
                    },
                }),
            });

            const result = await GetWeather.execute({});
            expect(result).toContain("Ho Chi Minh City");
            expect(result).toContain("33");
            expect(result).toContain("Slight rain");
        });

        it("should handle IP detection failure gracefully", async () => {
            // IP API fails
            mockSafeFetch.mockRejectedValueOnce(new Error("Network error"));

            const result = await GetWeather.execute({});
            expect(result).toContain("Unable to auto-detect");
        });
    });

    describe("execute — Error handling", () => {
        it("should handle weather API failure", async () => {
            // Weather API throws
            mockSafeFetch.mockRejectedValueOnce(new Error("API timeout"));

            const result = await GetWeather.execute({ location: "Hà Nội" });
            expect(result).toContain("failed");
            expect(result).toContain("API timeout");
        });

        it("should handle unknown weather codes", async () => {
            mockSafeFetch.mockResolvedValueOnce({
                json: async () => ({
                    current: {
                        temperature_2m: 25,
                        relative_humidity_2m: 50,
                        weather_code: 999,
                    },
                }),
            });

            const result = await GetWeather.execute({ location: "Đà Nẵng" });
            expect(result).toContain("Weather code: 999");
        });
    });

    describe("execute — Multi-day forecast", () => {
        it("should return forecast for tomorrow when days=2", async () => {
            mockSafeFetch.mockResolvedValueOnce({
                json: async () => ({
                    current: {
                        temperature_2m: 30,
                        relative_humidity_2m: 70,
                        weather_code: 2,
                    },
                    daily: {
                        time: ["2026-05-10", "2026-05-11"],
                        temperature_2m_max: [33, 34],
                        temperature_2m_min: [25, 26],
                        weather_code: [2, 61],
                        precipitation_probability_max: [10, 65],
                    },
                }),
            });

            const result = await GetWeather.execute({ location: "Hà Nội", days: 2 });
            expect(result).toContain("forecast");
            expect(result).toContain("Today");
            expect(result).toContain("Tomorrow");
            expect(result).toContain("2026-05-11");
            expect(result).toContain("34");
            expect(result).toContain("Rain: 65%");
        });

        it("should return 3-day forecast", async () => {
            mockSafeFetch.mockResolvedValueOnce({
                json: async () => ({
                    current: {
                        temperature_2m: 28,
                        relative_humidity_2m: 80,
                        weather_code: 3,
                    },
                    daily: {
                        time: ["2026-05-10", "2026-05-11", "2026-05-12"],
                        temperature_2m_max: [31, 32, 29],
                        temperature_2m_min: [24, 25, 23],
                        weather_code: [3, 61, 95],
                        precipitation_probability_max: [20, 70, 90],
                    },
                }),
            });

            const result = await GetWeather.execute({ location: "hcm", days: 3 });
            expect(result).toContain("3 days");
            expect(result).toContain("Today");
            expect(result).toContain("Tomorrow");
            expect(result).toContain("2026-05-12");
            expect(result).toContain("Thunderstorm");
        });

        it("should clamp days to maximum 7", async () => {
            mockSafeFetch.mockResolvedValueOnce({
                json: async () => ({
                    current: {
                        temperature_2m: 30,
                        relative_humidity_2m: 70,
                        weather_code: 0,
                    },
                    daily: {
                        time: ["2026-05-10"],
                        temperature_2m_max: [33],
                        temperature_2m_min: [25],
                        weather_code: [0],
                        precipitation_probability_max: [0],
                    },
                }),
            });

            // days=100 should be clamped to 7
            const result = await GetWeather.execute({ location: "Hà Nội", days: 100 });
            // Should not crash — internally capped to 7
            expect(result).toContain("forecast");
        });
    });
});
