import { describe, expect, it } from "vitest";
import { vendorChunkName } from "../vite.config";

describe("vendorChunkName", () => {
  it("keeps every Pixi package in the lazy vendor-pixi chunk", () => {
    expect(vendorChunkName("C:/repo/node_modules/@pixi/core/dist/esm/core.mjs")).toBe(
      "vendor-pixi",
    );
    expect(vendorChunkName("C:/repo/node_modules/pixi.js/dist/esm/pixi.mjs")).toBe(
      "vendor-pixi",
    );
  });

  it("preserves the other vendor boundaries", () => {
    expect(vendorChunkName("C:/repo/node_modules/three/build/three.module.js")).toBe(
      "vendor-three",
    );
    expect(vendorChunkName("C:/repo/node_modules/msgpackr/pack.js")).toBe("vendor");
    expect(vendorChunkName("C:/repo/src/WidgetApp.vue")).toBeUndefined();
  });
});
