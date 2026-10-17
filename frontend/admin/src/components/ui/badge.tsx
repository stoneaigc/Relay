import * as React from "react";
import { cn } from "@/lib/utils";

// 徽标:统一的胶囊形状态标记,数字走等宽字形以便对齐。
type Variant = "default" | "muted" | "success" | "destructive" | "warning" | "outline";

const styles: Record<Variant, string> = {
  default: "bg-brand-50 text-brand-700 border-brand-200",
  muted: "bg-muted text-muted-foreground border-border",
  success: "bg-success/10 text-success-foreground border-success/25",
  destructive: "bg-destructive/10 text-destructive border-destructive/25",
  warning: "bg-warning/12 text-warning-foreground border-warning/30",
  outline: "bg-transparent text-muted-foreground border-border-strong",
};

export function Badge({
  className,
  variant = "default",
  ...props
}: React.HTMLAttributes<HTMLSpanElement> & { variant?: Variant }) {
  return (
    <span
      className={cn(
        "inline-flex items-center gap-1 rounded-full border px-2.5 py-0.5 text-xs font-medium tabular-nums",
        styles[variant],
        className
      )}
      {...props}
    />
  );
}
