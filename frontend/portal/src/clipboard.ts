/** 复制文本到剪贴板。优先 Clipboard API(仅 HTTPS/localhost 可用),HTTP 环境回退 execCommand,保证非安全上下文也能复制。返回是否成功。 */
export async function copyText(t: string): Promise<boolean> {
  try {
    if (navigator.clipboard?.writeText) {
      await navigator.clipboard.writeText(t);
      return true;
    }
  } catch { /* 非安全上下文或权限拒绝,回退 execCommand */ }
  try {
    const ta = document.createElement("textarea");
    ta.value = t;
    ta.style.position = "fixed";
    ta.style.opacity = "0";
    document.body.appendChild(ta);
    ta.focus();
    ta.select();
    const ok = document.execCommand("copy");
    document.body.removeChild(ta);
    return ok;
  } catch {
    return false;
  }
}
