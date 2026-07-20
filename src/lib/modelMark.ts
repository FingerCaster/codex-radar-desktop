import codexLogo from "../assets/codex-logo.svg";
import lunaLogo from "../assets/luna-transparent.png";
import solLogo from "../assets/sol-transparent.png";
import terraLogo from "../assets/terra-transparent.png";

export type ModelMarkKind = "codex" | "luna" | "sol" | "terra";

const MODEL_MARK_SOURCES: Record<ModelMarkKind, string> = {
  codex: codexLogo,
  luna: lunaLogo,
  sol: solLogo,
  terra: terraLogo,
};

const MODEL_MARK_BY_IDENTIFIER: Readonly<Record<string, ModelMarkKind>> = {
  "gpt-5.6-luna": "luna",
  "gpt-5.6-sol": "sol",
  "gpt-5.6-terra": "terra",
};

const TASKBAR_DISPLAY_NAME_PATTERN =
  /(?:^|[^a-z0-9])gpt-5\.6[^a-z0-9]+(luna|sol|terra)(?=$|[^a-z0-9])/i;

export function resolveModelMarkKind(
  model?: string | null,
  displayName?: string | null,
): ModelMarkKind {
  const identifier = model?.trim().toLowerCase();
  if (identifier) {
    return MODEL_MARK_BY_IDENTIFIER[identifier] ?? "codex";
  }

  const displayToken = displayName
    ?.match(TASKBAR_DISPLAY_NAME_PATTERN)?.[1]
    ?.toLowerCase();
  return displayToken === "luna" ||
    displayToken === "sol" ||
    displayToken === "terra"
    ? displayToken
    : "codex";
}

export function resolveModelMarkSource(
  model?: string | null,
  displayName?: string | null,
): string {
  return MODEL_MARK_SOURCES[resolveModelMarkKind(model, displayName)];
}
