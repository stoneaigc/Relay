import * as React from "react";
import { cn } from "@/lib/utils";

// 输入框:白底 + 冷调描边,聚焦时品牌色描边 + 柔和光晕环(替代原先几乎不可见的灰环)。
const Input = React.forwardRef<HTMLInputElement, React.InputHTMLAttributes<HTMLInputElement>>(
  ({ className, ...props }, ref) => (
    <input
      ref={ref}
      className={cn(
        "flex h-9 w-full rounded-lg border border-input bg-card px-3 py-1 text-sm text-foreground shadow-card " +
          "transition-[border-color,box-shadow] duration-150 " +
          "placeholder:text-subtle-foreground hover:border-border-strong " +
          "focus-visible:border-brand-400 focus-visible:outline-none focus-visible:ring-4 focus-visible:ring-brand-500/15 " +
          "disabled:cursor-not-allowed disabled:bg-muted disabled:opacity-60",
        className
      )}
      {...props}
    />
  )
);
Input.displayName = "Input";

export { Input };
