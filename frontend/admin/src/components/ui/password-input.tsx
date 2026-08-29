import * as React from "react";
import { cn } from "@/lib/utils";
import { Input } from "./input";
import { Eye, EyeOff } from "lucide-react";

/** 带"小眼睛"的密码输入框:点击眼睛可切换明文/密文。 */
export function PasswordInput({
  className,
  value,
  onChange,
  ...props
}: React.InputHTMLAttributes<HTMLInputElement>) {
  const [show, setShow] = React.useState(false);
  return (
    <div className="relative">
      <Input
        type={show ? "text" : "password"}
        className={cn("pr-10", className)}
        value={value}
        onChange={onChange}
        {...props}
      />
      <button
        type="button"
        onClick={() => setShow((s) => !s)}
        aria-label={show ? "隐藏密码" : "显示密码"}
        className="absolute inset-y-0 right-0 flex items-center pr-3 text-muted-foreground transition-colors hover:text-foreground"
        tabIndex={-1}
      >
        {show ? <EyeOff className="size-4" /> : <Eye className="size-4" />}
      </button>
    </div>
  );
}
