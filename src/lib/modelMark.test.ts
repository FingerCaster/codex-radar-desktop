import { describe, expect, it } from "vitest";

import { resolveModelMarkKind } from "./modelMark";

describe("resolveModelMarkKind", () => {
  it.each([
    ["gpt-5.6-sol", "sol"],
    ["gpt-5.6-terra", "terra"],
    ["gpt-5.6-luna", "luna"],
  ] as const)("maps the stable %s identifier to %s", (model, expected) => {
    expect(resolveModelMarkKind(model)).toBe(expected);
  });

  it.each([
    "gpt-5.5",
    "gpt-5.5-codex-max",
    "gpt-5.4",
    "gpt-5.4-mini",
    "gpt-5.6-solaris",
    "unknown-model",
  ])("keeps %s on the Codex fallback", (model) => {
    expect(resolveModelMarkKind(model)).toBe("codex");
  });

  it.each([
    ["GPT-5.6 Sol", "sol"],
    ["GPT-5.6 Terra max", "terra"],
    ["GPT-5.6-Luna (high)", "luna"],
  ] as const)(
    "uses the bounded taskbar display token in %s",
    (displayName, expected) => {
      expect(resolveModelMarkKind(undefined, displayName)).toBe(expected);
    },
  );

  it.each([
    "GPT-5.5 Sol",
    "GPT-5.4 Terra",
    "GPT-5.6 Solaris",
    "GPT-5.60 Luna",
    "Sol",
  ])("does not infer a family from the ambiguous display name %s", (displayName) => {
    expect(resolveModelMarkKind(undefined, displayName)).toBe("codex");
  });

  it("does not override an unknown stable identifier with display text", () => {
    expect(resolveModelMarkKind("gpt-5.5", "GPT-5.6 Sol")).toBe("codex");
  });
});
