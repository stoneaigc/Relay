import * as React from "react";
import { cn } from "@/lib/utils";

// 表格:表头用极浅冷灰底 + 更小更硬的字重,行悬浮给明确的品牌色反馈;
// 单元格默认等宽数字,避免金额/计数在小数位上跳动。
export const Table = ({ className, ...props }: React.HTMLAttributes<HTMLTableElement>) => (
  <div className="w-full overflow-auto">
    <table className={cn("w-full caption-bottom text-sm", className)} {...props} />
  </div>
);
export const TableHeader = ({ className, ...props }: React.HTMLAttributes<HTMLTableSectionElement>) => (
  <thead className={cn("bg-muted/60 [&_tr]:border-b [&_tr]:hover:bg-transparent", className)} {...props} />
);
export const TableBody = ({ className, ...props }: React.HTMLAttributes<HTMLTableSectionElement>) => (
  <tbody className={cn("[&_tr:last-child]:border-0", className)} {...props} />
);
export const TableRow = ({ className, ...props }: React.HTMLAttributes<HTMLTableRowElement>) => (
  <tr
    className={cn("border-b border-border transition-colors duration-150 hover:bg-brand-50/60", className)}
    {...props}
  />
);
export const TableHead = ({ className, ...props }: React.ThHTMLAttributes<HTMLTableCellElement>) => (
  <th
    className={cn(
      "h-11 whitespace-nowrap px-3 text-left align-middle text-xs font-semibold text-muted-foreground",
      className
    )}
    {...props}
  />
);
export const TableCell = ({ className, ...props }: React.TdHTMLAttributes<HTMLTableCellElement>) => (
  <td className={cn("px-3 py-3 align-middle tabular-nums", className)} {...props} />
);
