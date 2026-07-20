import type { ModelScore } from "../types/radar";

export function getModelDisplayName(
  model: Pick<ModelScore, "label" | "model" | "reasoningEffort">,
): string {
  const label = model.label.trim() || model.model.trim();
  const effort = model.reasoningEffort.trim();
  if (!effort) {
    return label;
  }

  const suffix = ` ${effort}`;
  return label.toLocaleLowerCase().endsWith(suffix.toLocaleLowerCase())
    ? label.slice(0, -suffix.length)
    : label;
}
