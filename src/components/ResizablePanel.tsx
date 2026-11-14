import { useRef, useState, type ReactNode } from "react";
import { t } from "@/i18n";
import "./ResizablePanel.scss";

const clamp = (w: number, min: number, max: number) =>
  Math.round(Math.min(Math.max(w, min), max));

function load(key: string, def: number, min: number, max: number): number {
  try {
    const v = Number(localStorage.getItem(key));
    return v > 0 ? clamp(v, min, max) : def;
  } catch {
    return def;
  }
}

function save(key: string, w: number) {
  try {
    localStorage.setItem(key, String(w));
  } catch {
    /* storage unavailable: the width just won't persist */
  }
}

/**
 * A side panel whose width the user can change. `edge` is the side that carries the drag
 * handle: "right" for a panel on the left of the window, "left" for one on the right.
 * Dragging, double-click (reset) and arrow keys all move the handle, and the width is remembered.
 */
export default function ResizablePanel({
  edge,
  storageKey,
  defaultWidth,
  min = 180,
  max = 640,
  hidden = false,
  children,
}: {
  edge: "left" | "right";
  storageKey: string;
  defaultWidth: number;
  min?: number;
  max?: number;
  /** Not shown, but still mounted, so what it holds (a half-typed message) survives. */
  hidden?: boolean;
  children: ReactNode;
}) {
  const [width, setWidth] = useState(() => load(storageKey, defaultWidth, min, max));
  const drag = useRef<{ startX: number; startW: number } | null>(null);
  // Moving the handle right grows a panel whose handle is on its right edge, and shrinks the other.
  const dir = edge === "right" ? 1 : -1;

  const set = (w: number) => {
    const next = clamp(w, min, max);
    setWidth(next);
    return next;
  };
  const widthAt = (clientX: number) =>
    drag.current ? drag.current.startW + dir * (clientX - drag.current.startX) : width;

  const onPointerDown = (e: React.PointerEvent<HTMLDivElement>) => {
    e.preventDefault();
    e.currentTarget.setPointerCapture(e.pointerId); // keep receiving moves outside the handle
    drag.current = { startX: e.clientX, startW: width };
    document.body.classList.add("resizing");
  };
  const onPointerMove = (e: React.PointerEvent<HTMLDivElement>) => {
    if (drag.current) set(widthAt(e.clientX));
  };
  const endDrag = (e: React.PointerEvent<HTMLDivElement>) => {
    if (!drag.current) return;
    const w = widthAt(e.clientX);
    drag.current = null;
    document.body.classList.remove("resizing");
    save(storageKey, set(w));
  };
  const onKeyDown = (e: React.KeyboardEvent<HTMLDivElement>) => {
    const step = e.shiftKey ? 48 : 16;
    if (e.key === "ArrowRight") save(storageKey, set(width + dir * step));
    else if (e.key === "ArrowLeft") save(storageKey, set(width - dir * step));
    else return;
    e.preventDefault();
  };

  return (
    <div className="resizable" style={{ width, minWidth: min }} hidden={hidden}>
      {children}
      <div
        className={`resize-handle edge-${edge}`}
        role="separator"
        aria-orientation="vertical"
        aria-label={t.resizePanel.label}
        aria-valuemin={min}
        aria-valuemax={max}
        aria-valuenow={width}
        tabIndex={0}
        title={t.resizePanel.hint}
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={endDrag}
        onPointerCancel={endDrag}
        onDoubleClick={() => save(storageKey, set(defaultWidth))}
        onKeyDown={onKeyDown}
      />
    </div>
  );
}
