import type { ButtonHTMLAttributes } from "react";
import type { LucideIcon } from "lucide-react";

export interface IconButtonProps
  extends Omit<ButtonHTMLAttributes<HTMLButtonElement>, "children"> {
  icon: LucideIcon;
  label: string;
  iconSize?: number;
}

export function IconButton({
  icon: Icon,
  label,
  iconSize = 16,
  className,
  type = "button",
  ...buttonProps
}: IconButtonProps) {
  const classes = ["icon-button", className].filter(Boolean).join(" ");

  return (
    <button
      {...buttonProps}
      aria-label={label}
      className={classes}
      title={label}
      type={type}
    >
      <Icon aria-hidden="true" size={iconSize} strokeWidth={1.9} />
    </button>
  );
}

export default IconButton;
