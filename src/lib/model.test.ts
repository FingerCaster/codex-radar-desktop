import { describe, expect, it } from "vitest";

import { getModelDisplayName } from "./model";

describe("getModelDisplayName", () => {
  it("keeps the reasoning effort in its own visual field", () => {
    expect(
      getModelDisplayName({
        label: "GPT-5.6 Sol max",
        model: "gpt-5.6-sol",
        reasoningEffort: "max",
      }),
    ).toBe("GPT-5.6 Sol");
  });

  it("does not remove unrelated suffix text", () => {
    expect(
      getModelDisplayName({
        label: "Custom Solver",
        model: "custom-solver",
        reasoningEffort: "high",
      }),
    ).toBe("Custom Solver");
  });
});
