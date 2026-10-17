import * as React from "react";
import * as SheetPrimitive from "@radix-ui/react-dialog";
import { X } from "lucide-react";
import { cn } from "@/lib/utils";

// 右侧抽屉:基于 radix dialog 的侧边变体。
// 入场/退场动画由 index.css 中的 [data-sheet-panel] / [data-sheet-veil] 关键帧驱动
// (不依赖 tailwindcss-animate,避免工具类不存在导致动画失效)。

const Sheet = SheetPrimitive.Root;
const SheetTrigger = SheetPrimitive.Trigger;
const SheetClose = SheetPrimitive.Close;

const SheetContent = React.forwardRef<
  React.ElementRef<typeof SheetPrimitive.Content>,
  React.ComponentPropsWithoutRef<typeof SheetPrimitive.Content>
>(({ className, children, ...props }, ref) => (
  <SheetPrimitive.Portal>
    <SheetPrimitive.Overlay
      data-sheet-veil="true"
      className="fixed inset-0 z-50 bg-[#0b0d18]/40 backdrop-blur-[3px]"
    />
    <SheetPrimitive.Content
      ref={ref}
      data-sheet-panel="true"
      className={cn(
        "fixed inset-y-0 right-0 z-50 flex h-full w-full flex-col overflow-hidden border-l border-border bg-card shadow-drawer",
        "sm:w-[min(94vw,540px)]",
        className
      )}
      {...props}
    >
      {/* 顶部品牌色细条,作为抽屉的身份标识 */}
      <div className="brand-gradient h-[3px] w-full shrink-0" aria-hidden />
      {children}
      <SheetPrimitive.Close
        aria-label="关闭"
        className="absolute right-4 top-[15px] grid h-8 w-8 place-items-center rounded-lg text-muted-foreground outline-none transition-colors duration-150 hover:bg-muted hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring"
      >
        <X className="h-4 w-4" />
      </SheetPrimitive.Close>
    </SheetPrimitive.Content>
  </SheetPrimitive.Portal>
));
SheetContent.displayName = "SheetContent";

const SheetHeader = ({ className, ...props }: React.HTMLAttributes<HTMLDivElement>) => (
  <div
    className={cn("flex shrink-0 flex-col gap-1 border-b border-border px-6 pb-4 pr-14 pt-4", className)}
    {...props}
  />
);

const SheetTitle = React.forwardRef<
  React.ElementRef<typeof SheetPrimitive.Title>,
  React.ComponentPropsWithoutRef<typeof SheetPrimitive.Title>
>(({ className, ...props }, ref) => (
  <SheetPrimitive.Title
    ref={ref}
    className={cn("text-[17px] font-semibold leading-tight tracking-tight text-foreground", className)}
    {...props}
  />
));
SheetTitle.displayName = "SheetTitle";

const SheetDescription = React.forwardRef<
  React.ElementRef<typeof SheetPrimitive.Description>,
  React.ComponentPropsWithoutRef<typeof SheetPrimitive.Description>
>(({ className, ...props }, ref) => (
  <SheetPrimitive.Description
    ref={ref}
    className={cn("text-[13px] leading-relaxed text-muted-foreground", className)}
    {...props}
  />
));
SheetDescription.displayName = "SheetDescription";

// 抽屉滚动主体:内容子项以 stagger 方式依次上浮(见 index.css 的 [data-sheet-body])。
const SheetBody = ({ className, ...props }: React.HTMLAttributes<HTMLDivElement>) => (
  <div data-sheet-body="true" className={cn("flex-1 space-y-6 overflow-y-auto px-6 py-5", className)} {...props} />
);

// 抽屉底部操作条:粘底,与内容之间用分隔线隔开。
const SheetFooter = ({ className, ...props }: React.HTMLAttributes<HTMLDivElement>) => (
  <div
    className={cn(
      "flex shrink-0 items-center justify-end gap-2 border-t border-border bg-card px-6 py-3.5",
      className
    )}
    {...props}
  />
);

// 表单小节:品牌色竖条 + 标题 + 可选说明,统一抽屉内的分段排版。
const SheetSection = ({
  title,
  hint,
  children,
  className,
}: {
  title: string;
  hint?: string;
  children: React.ReactNode;
  className?: string;
}) => (
  <section className={cn("space-y-3.5", className)}>
    <div className="flex items-center gap-2">
      <span className="brand-gradient h-3.5 w-[3px] shrink-0 rounded-full" aria-hidden />
      <h4 className="text-[13px] font-semibold text-foreground">{title}</h4>
      {hint && <span className="ml-0.5 text-[11px] text-subtle-foreground">{hint}</span>}
    </div>
    {children}
  </section>
);

export {
  Sheet,
  SheetTrigger,
  SheetClose,
  SheetContent,
  SheetHeader,
  SheetTitle,
  SheetDescription,
  SheetBody,
  SheetFooter,
  SheetSection,
};
