import { resolveModelMarkSource } from "../lib/modelMark";

interface ModelMarkProps {
  className?: string;
  displayName?: string | null;
  model?: string | null;
}

export function ModelMark({ className, displayName, model }: ModelMarkProps) {
  const classes = className ? `model-mark ${className}` : "model-mark";

  return (
    <img
      alt=""
      aria-hidden="true"
      className={classes}
      draggable={false}
      src={resolveModelMarkSource(model, displayName)}
    />
  );
}

export default ModelMark;
