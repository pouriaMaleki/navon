import { describe, expect, it, vi } from "vitest";

import { persistSpeedUnit, readStoredSpeedUnit } from "./wasmProgram";

describe("wasm speed unit persistence", () => {
  it("reads a stored speed unit when present", () => {
    const storage = {
      getItem: vi.fn(() => "mph"),
    };

    expect(readStoredSpeedUnit(storage)).toBe("mph");
  });

  it("ignores invalid stored speed unit values", () => {
    const storage = {
      getItem: vi.fn(() => "knots"),
    };

    expect(readStoredSpeedUnit(storage)).toBeUndefined();
  });

  it("persists the selected speed unit", () => {
    const storage = {
      setItem: vi.fn(),
    };

    persistSpeedUnit(storage, "kph");

    expect(storage.setItem).toHaveBeenCalledWith("esp32-minimap.speed-unit", "kph");
  });
});
