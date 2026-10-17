import * as React from "react";
import { Slot } from "@radix-ui/react-slot";
import { cva, type VariantProps } from "class-variance-authority";
import { cn } from "@/lib/utils";

// 按钮基元:主按钮走品牌渐变 + 品牌投影,其余变体走冷调中性面;
// 统一 150ms 颜色/阴影过渡 + 按下微缩放,给交互以触感。
const buttonVariants = cva(
  "inline-flex select-none items-center justify-center gap-2 whitespace-nowrap rounded-lg text-sm font-medium " +
    "transition-[color,background-color,border-color,box-shadow,transform,filter] duration-150 ease-out " +
    "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-1 focus-visible:ring-offset-background " +
    "active:scale-[0.985] disabled:pointer-events-none disabled:opacity-50 disabled:active:scale-100",
  {
    variants: {
      variant: {
        default:
          "brand-gradient text-primary-foreground shadow-brand hover:brightness-[1.07] active:brightness-[0.96]",
        outline:
          "border border-border-strong bg-card text-foreground shadow-card hover:border-brand-300 hover:bg-accent hover:text-accent-foreground",
        ghost: "text-foreground/80 hover:bg-accent hover:text-accent-foreground",
        destructive:
          "bg-destructive text-destructive-foreground shadow-card hover:brightness-[1.06] active:brightness-[0.96]",
        secondary:
          "bg-secondary text-secondary-foreground hover:bg-brand-100 hover:text-brand-700",
      },
      size: {
        default: "h-9 px-4 py-2",
        sm: "h-8 rounded-md px-3 text-xs",
        lg: "h-11 rounded-xl px-6 text-[15px]",
        icon: "h-9 w-9",
      },
    },
    defaultVariants: { variant: "default", size: "default" },
  }
);

export interface ButtonProps
  extends React.ButtonHTMLAttributes<HTMLButtonElement>,
    VariantProps<typeof buttonVariants> {
  asChild?: boolean;
}

const Button = React.forwardRef<HTMLButtonElement, ButtonProps>(
  ({ className, variant, size, asChild = false, ...props }, ref) => {
    const Comp = asChild ? Slot : "button";
    return <Comp className={cn(buttonVariants({ variant, size, className }))} ref={ref} {...props} />;
  }
);
Button.displayName = "Button";

export { Button, buttonVariants };
