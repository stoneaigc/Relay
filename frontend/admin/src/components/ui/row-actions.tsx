import * as React from "react";
import { MoreHorizontal } from "lucide-react";
import { cn } from "@/lib/utils";
import {
  DropdownMenu,
  DropdownMenuTrigger,
  DropdownMenuContent,
  DropdownMenuItem,
} from "@/components/ui/dropdown-menu";
import { Tooltip, TooltipTrigger, TooltipContent } from "@/components/ui/tooltip";

export interface RowAction {
  /** 操作名:作为图标按钮的悬停提示,以及下拉菜单里的文字 */
  label: string;
  /** 图标元素,如 <Pencil className="h-4 w-4" /> */
  icon?: React.ReactNode;
  onClick: () => void;
  /** 危险操作(如删除)悬停 / 菜单项显示为红色 */
  variant?: "default" | "destructive";
  disabled?: boolean;
}

interface RowActionsProps {
  actions: RowAction[];
  /** 外部最多直接展示几个按钮,超出收进 more,默认 2 */
  max?: number;
}

/**
 * 列表行操作:外面最多显示 `max` 个按钮(默认 2),
 * 超过 `max` 个时,多出来的收进末尾的「⋯」more 下拉菜单。
 */
export function RowActions({ actions, max = 2 }: RowActionsProps) {
  const valid = actions.filter(Boolean);
  // 不超过 max 个时全部内联;超过时,先内联 max 个,其余进下拉
  const inline = valid.length > max ? valid.slice(0, max) : valid;
  const overflow = valid.length > max ? valid.slice(max) : [];

  return (
    <div className="flex items-center justify-end gap-1">
      {inline.map((a, i) => (
        <Tooltip key={i}>
          <TooltipTrigger asChild>
            <button
              type="button"
              aria-label={a.label}
              disabled={a.disabled}
              onClick={a.onClick}
              className={cn(
                "inline-flex h-8 w-8 items-center justify-center rounded-md text-muted-foreground outline-none transition-colors disabled:pointer-events-none disabled:opacity-50",
                a.variant === "destructive"
                  ? "hover:bg-destructive/10 hover:text-destructive"
                  : "hover:bg-accent hover:text-foreground"
              )}
            >
              {a.icon}
            </button>
          </TooltipTrigger>
          <TooltipContent>{a.label}</TooltipContent>
        </Tooltip>
      ))}
      {overflow.length > 0 && (
        <DropdownMenu>
          <DropdownMenuTrigger className="inline-flex h-8 w-8 items-center justify-center rounded-md text-muted-foreground outline-none transition-colors hover:bg-accent hover:text-foreground">
            <MoreHorizontal className="h-4 w-4" />
          </DropdownMenuTrigger>
          <DropdownMenuContent>
            {overflow.map((a, i) => (
              <DropdownMenuItem
                key={i}
                disabled={a.disabled}
                onClick={a.onClick}
                className={a.variant === "destructive" ? "text-destructive focus:text-destructive" : undefined}
              >
                {a.icon}
                {a.label}
              </DropdownMenuItem>
            ))}
          </DropdownMenuContent>
        </DropdownMenu>
      )}
    </div>
  );
}
