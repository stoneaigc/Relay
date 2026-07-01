import * as React from "react";
import { cn } from "@/lib/utils";

type Div = React.HTMLAttributes<HTMLDivElement>;

export const Card = React.forwardRef<HTMLDivElement, Div>(({ className, ...props }, ref) => (
  <div ref={ref} className={cn("rounded-xl border bg-card text-card-foreground shadow-sm", className)} {...props} />
));
Card.displayName = "Card";

export const CardHeader = ({ className, ...props }: Div) => (
  <div className={cn("flex flex-col space-y-1.5 p-5", className)} {...props} />
);
export const CardTitle = ({ className, ...props }: Div) => (
  <div className={cn("font-semibold leading-none tracking-tight", className)} {...props} />
);
export const CardDescription = ({ className, ...props }: Div) => (
  <div className={cn("text-sm text-muted-foreground", className)} {...props} />
);
export const CardContent = ({ className, ...props }: Div) => (
  <div className={cn("p-5 pt-0", className)} {...props} />
);
export const CardFooter = ({ className, ...props }: Div) => (
  <div className={cn("flex items-center p-5 pt-0", className)} {...props} />
);
