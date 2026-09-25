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
    ta.setAttribute("readonly", "");
    ta.style.position = "fixed";
    ta.style.top = "0";
    ta.style.left = "0";
    ta.style.opacity = "0";
    document.body.appendChild(ta);
    // Dialog 的焦点陷阱会干扰 focus/select,这里用 Range 选区 + setSelectionRange 双保险,并保存/恢复用户原有选区。
    const sel = window.getSelection();
    const saved = sel?.rangeCount ? sel.getRangeAt(0) : null;
    const range = document.createRange();
    range.selectNodeContents(ta);
    sel?.removeAllRanges();
    sel?.addRange(range);
    ta.setSelectionRange(0, t.length);
    ta.focus();
    const ok = document.execCommand("copy");
    sel?.removeAllRanges();
    if (saved && sel) sel.addRange(saved);
    document.body.removeChild(ta);
    return ok;
  } catch {
    return false;
  }
}
