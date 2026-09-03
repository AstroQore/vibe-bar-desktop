import { useEffect, useRef, useState, type ReactNode } from "react";

/** A toolbar menu: the native `menuLabel` (icon · TITLE · detail) that drops a
 *  panel below itself. Closes on outside click or Escape. */
export function Menu({
  icon,
  title,
  detail,
  ariaLabel,
  width = 220,
  caps = true,
  children,
}: {
  icon: ReactNode;
  title: string;
  detail: string;
  ariaLabel: string;
  width?: number;
  /** Caps title + detail (the native menuLabel) or one plain label. */
  caps?: boolean;
  children: (close: () => void) => ReactNode;
}) {
  const [open, setOpen] = useState(false);
  // Anchor the panel to whichever edge of the button keeps it inside the
  // window: right-anchored under a button near the left edge would hang off it.
  const [anchor, setAnchor] = useState<"left" | "right">("right");
  const root = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!open) return;
    const onDown = (event: MouseEvent) => {
      if (!root.current?.contains(event.target as Node)) setOpen(false);
    };
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") setOpen(false);
    };
    document.addEventListener("mousedown", onDown);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onDown);
      document.removeEventListener("keydown", onKey);
    };
  }, [open]);
  return (
    <div className="us-menu" ref={root}>
      <button
        type="button"
        className={`us-menulabel${open ? " open" : ""}`}
        aria-label={ariaLabel}
        aria-expanded={open}
        onClick={() => {
          const rect = root.current?.getBoundingClientRect();
          if (rect) setAnchor(rect.left + width > window.innerWidth - 12 ? "right" : "left");
          setOpen((value) => !value);
        }}
      >
        <span className="us-menulabel-icon">{icon}</span>
        <span className={caps ? "us-menulabel-title" : "us-menulabel-detail"}>{title}</span>
        {detail ? <span className="us-menulabel-detail">{detail}</span> : null}
      </button>
      {open ? (
        <div className="us-menu-panel" style={{ width, ...(anchor === "left" ? { left: 0, right: "auto" } : { right: 0, left: "auto" }) }} role="menu">
          {children(() => setOpen(false))}
        </div>
      ) : null}
    </div>
  );
}

export function MenuItem({
  checked,
  onSelect,
  children,
  disabled,
}: {
  checked?: boolean;
  onSelect: () => void;
  children: ReactNode;
  disabled?: boolean;
}) {
  return (
    <button type="button" role="menuitemcheckbox" aria-checked={checked} className="us-menu-item" onClick={onSelect} disabled={disabled}>
      <span className="us-menu-check">{checked ? "✓" : ""}</span>
      <span className="us-menu-text">{children}</span>
    </button>
  );
}
