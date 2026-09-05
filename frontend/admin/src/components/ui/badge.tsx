import * as React from "react";
import { cn } from "@/lib/utils";

type Variant = "default" | "muted" | "success" | "destructive";

const styles: Record<Variant, string> = {
  default: "bg-primary/15 text-primary border-primary/25",
  muted: "bg-muted text-muted-foreground border-border",
  success: "bg-success/15 text-success border-success/30",
  destructive: "bg-destructive/15 text-destructive border-destructive/30",
};

export function Badge({
  className,
  variant = "default",
  ...props
}: React.HTMLAttributes<HTMLSpanElement> & { variant?: Variant }) {
  return (
    <span
      className={cn(
        "inline-flex items-center rounded-full border px-2.5 py-0.5 text-xs font-medium",
        styles[variant],
        className
      )}
      {...props}
    />
  );
}
