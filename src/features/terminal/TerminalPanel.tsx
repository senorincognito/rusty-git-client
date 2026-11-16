import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { Terminal as XTerm } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import "@xterm/xterm/css/xterm.css";
import { getLocalBranches } from "@/api/branches";
import { getTags } from "@/api/graph";
import { getRemotes } from "@/api/remotes";
import { termResize, termStart, termStop, termWrite } from "@/api/terminal";
import { useTerminalAutocomplete } from "@/hooks/useSetting";
import { t as text } from "@/i18n";
import { completionContext, insertion, LineTracker, rankSuggestions } from "./completion";
import "./TerminalPanel.scss";

const MIN_H = 120;
let nextSessionId = Date.now();

/** The suggestion list, placed in viewport pixels: above the cursor line when there is room, else below it. */
interface Popup {
  items: string[];
  index: number;
  /** The typed part of the word the choice replaces. */
  prefix: string;
  left: number;
  top: number | null;
  bottom: number | null;
}

/**
 * Bottom panel with a real shell in the repository folder. It stays mounted while hidden so
 * the session (and its scrollback) survives toggling; it restarts when the repo changes.
 */
export default function TerminalPanel({
  path,
  open,
  onClose,
  paste,
  refsKey,
}: {
  path: string;
  open: boolean;
  onClose: () => void;
  /** Text to type into the shell (without Enter) once it runs; a new `id` is a new request. */
  paste?: { id: number; text: string } | null;
  /** Changes whenever the repository's branches or tags may have (the suggestions are loaded again). */
  refsKey: number;
}) {
  const host = useRef<HTMLDivElement>(null);
  const term = useRef<XTerm | null>(null);
  const fit = useRef<FitAddon | null>(null);
  const session = useRef(0);
  const started = useRef(false);
  const exited = useRef(false);
  const sessionReady = useRef(false);
  const queued = useRef<string | null>(null);
  const pathRef = useRef(path);
  const openRef = useRef(open);
  const [ready, setReady] = useState(false);
  const [height, setHeight] = useState(280);
  const [autocomplete] = useTerminalAutocomplete();
  const autocompleteRef = useRef(autocomplete);
  const tracker = useRef(new LineTracker());
  const names = useRef<string[] | null>(null); // branches and tags; null until loaded for the current repo
  const popupRef = useRef<Popup | null>(null);
  const acceptRef = useRef<(p: Popup, index: number) => void>(() => {});
  const [popup, setPopupState] = useState<Popup | null>(null);

  pathRef.current = path;
  openRef.current = open;
  autocompleteRef.current = autocomplete;

  const setPopup = useCallback((p: Popup | null) => {
    popupRef.current = p;
    setPopupState(p);
  }, []);

  // xterm's paste() brackets the text when the program asked for it, so a TUI takes it as pasted, not typed.
  const flush = useCallback(() => {
    const t = term.current;
    if (queued.current === null || !t || !sessionReady.current) return;
    t.paste(queued.current);
    queued.current = null;
    t.focus();
  }, []);

  const start = useCallback(async () => {
    const t = term.current;
    if (!t) return;
    exited.current = false;
    sessionReady.current = false;
    tracker.current.reset();
    const id = ++nextSessionId;
    session.current = id;
    t.reset();
    try {
      fit.current?.fit();
    } catch {
      /* host not measurable yet */
    }
    try {
      await termStart(pathRef.current, id, t.cols, t.rows);
      sessionReady.current = true;
      flush();
    } catch (e) {
      exited.current = true;
      queued.current = null;
      t.write(text.terminal.startFailed(String(e)));
    }
  }, [flush]);

  // Create the emulator once and connect it to the backend.
  useEffect(() => {
    const t = new XTerm({
      fontFamily: 'Consolas, "Cascadia Mono", Menlo, monospace',
      fontSize: 13,
      cursorBlink: true,
      scrollback: 5000,
      theme: {
        background: "#1b1d23",
        foreground: "#e4e6eb",
        cursor: "#3fa7a0",
        selectionBackground: "#3fa7a055",
        // The terminal draws its own scrollbar: same colours as the rest of the app (--muted #8b90a0).
        scrollbarSliderBackground: "#8b90a059",
        scrollbarSliderHoverBackground: "#8b90a0a6",
        scrollbarSliderActiveBackground: "#8b90a0a6",
      },
    });
    const f = new FitAddon();
    t.loadAddon(f);
    t.open(host.current!);
    term.current = t;
    fit.current = f;

    // Viewport position of the suggestions for a word whose first `prefixLen` characters are before the cursor.
    const anchor = (prefixLen: number) => {
      const screen = host.current?.querySelector(".xterm-screen");
      if (!screen) return null;
      const r = screen.getBoundingClientRect();
      const cellW = r.width / t.cols;
      const cellH = r.height / t.rows;
      const cursor = t.buffer.active;
      const left = Math.max(r.left, r.left + (cursor.cursorX - prefixLen) * cellW);
      const y = r.top + cursor.cursorY * cellH;
      return y > 200 ? { left, top: null, bottom: window.innerHeight - y } : { left, top: y + cellH, bottom: null };
    };

    let loading: Promise<void> | null = null;
    const loadNames = () => {
      if (loading) return loading;
      const forPath = pathRef.current;
      loading = Promise.all([getLocalBranches(forPath), getRemotes(forPath), getTags(forPath)])
        .then(([local, remotes, tags]) => {
          if (pathRef.current !== forPath) return;
          names.current = [
            ...local.map((b) => b.name),
            ...remotes.flatMap((r) => r.branches.map((b) => `${r.name}/${b}`)),
            ...tags.map((x) => x.name),
          ];
        })
        .catch(() => {
          if (pathRef.current === forPath) names.current = []; // no retry until the next change of refs
        })
        .finally(() => {
          loading = null;
        });
      return loading;
    };

    // The shadow line is only trusted at a prompt: a full-screen program (less, vim) reads the keys itself.
    const refresh = () => {
      const ctx =
        autocompleteRef.current && tracker.current.valid && t.buffer.active.type !== "alternate"
          ? completionContext(tracker.current.line)
          : null;
      if (!ctx) return setPopup(null);
      if (!names.current) {
        loadNames().then(refresh);
        return;
      }
      const items = rankSuggestions(ctx.prefix, names.current);
      const pos = anchor(ctx.prefix.length);
      setPopup(items.length && pos ? { items, index: 0, prefix: ctx.prefix, ...pos } : null);
    };

    acceptRef.current = (p, index) => {
      const choice = p.items[index];
      termWrite(session.current, insertion(p.prefix, choice)).catch(() => {});
      const tr = tracker.current;
      tr.line = tr.line.slice(0, tr.line.length - p.prefix.length) + choice;
      setPopup(null);
    };

    t.onCursorMove(() => {
      const p = popupRef.current;
      const pos = p && anchor(p.prefix.length);
      if (p && pos) setPopup({ ...p, ...pos });
    });
    t.textarea?.addEventListener("blur", () => setPopup(null));

    t.attachCustomKeyEventHandler((e) => {
      if (e.type !== "keydown") return true;
      // Let the app handle the toggle shortcut.
      if (e.ctrlKey && e.code === "Backquote") return false;
      if ((e.ctrlKey || e.metaKey) && e.key === ",") return false; // Settings
      // While suggestions show, these keys drive them instead of the shell.
      const p = popupRef.current;
      if (p && !e.ctrlKey && !e.altKey && !e.metaKey && !e.shiftKey) {
        if (e.key === "Tab") {
          e.preventDefault(); // else the webview moves focus out of the terminal
          acceptRef.current(p, p.index);
          return false;
        }
        if (e.key === "ArrowDown" || e.key === "ArrowUp") {
          e.preventDefault();
          const step = e.key === "ArrowDown" ? 1 : p.items.length - 1;
          setPopup({ ...p, index: (p.index + step) % p.items.length });
          return false;
        }
        if (e.key === "Escape") {
          setPopup(null);
          return false;
        }
      }
      // Ctrl+C copies when text is selected; otherwise it is the usual interrupt.
      if (e.ctrlKey && !e.shiftKey && e.code === "KeyC" && t.hasSelection()) {
        navigator.clipboard?.writeText(t.getSelection()).catch(() => {});
        t.clearSelection();
        return false;
      }
      return true;
    });

    t.onData((data) => {
      if (exited.current) {
        start();
      } else {
        termWrite(session.current, data).catch(() => {});
        tracker.current.feed(data);
        refresh();
      }
    });

    let disposed = false;
    const unlisten: (() => void)[] = [];
    Promise.all([
      listen<{ id: number; data: string }>("term-data", (e) => {
        if (e.payload.id === session.current) t.write(e.payload.data);
      }),
      listen<number>("term-exit", (e) => {
        if (e.payload === session.current) {
          exited.current = true;
          t.write("\r\n\x1b[2m[Process exited. Press any key to restart]\x1b[0m\r\n");
        }
      }),
    ]).then((fns) => {
      if (disposed) fns.forEach((fn) => fn());
      else {
        unlisten.push(...fns);
        setReady(true);
      }
    });

    const ro = new ResizeObserver(() => {
      const h = host.current;
      if (!h || h.clientHeight === 0 || !openRef.current) return; // hidden: nothing to measure
      try {
        f.fit();
        if (started.current) termResize(session.current, t.cols, t.rows).catch(() => {});
      } catch {
        /* ignore transient measuring errors */
      }
    });
    ro.observe(host.current!);

    return () => {
      disposed = true;
      ro.disconnect();
      unlisten.forEach((fn) => fn());
      t.dispose();
      term.current = null;
      fit.current = null;
      started.current = false;
      setReady(false);
      termStop().catch(() => {});
    };
  }, [start, setPopup]);

  // A different repository or a moved ref means the names are stale; the list closes with the panel or the setting.
  useEffect(() => {
    names.current = null;
  }, [path, refsKey]);
  useEffect(() => {
    if (!open || !autocomplete) setPopup(null);
  }, [open, autocomplete, setPopup]);

  // First time the panel is shown, start the shell; afterwards just refit and focus.
  useEffect(() => {
    if (!open || !ready) return;
    if (!started.current) {
      started.current = true;
      start();
    } else {
      try {
        fit.current?.fit();
        if (term.current) termResize(session.current, term.current.cols, term.current.rows);
      } catch {
        /* ignore */
      }
    }
    term.current?.focus();
  }, [open, ready, start]);

  // A paste request waits for the shell (a closed panel is started by opening it); an exited shell is restarted.
  useEffect(() => {
    if (!paste) return;
    queued.current = paste.text;
    if (exited.current) start();
    else flush();
  }, [paste, start, flush]);

  // A different repository gets a fresh shell in its folder.
  useEffect(() => {
    if (started.current) start();
  }, [path, start]);

  const startDrag = (e: React.MouseEvent) => {
    e.preventDefault();
    const startY = e.clientY;
    const startH = height;
    const onMove = (m: MouseEvent) =>
      setHeight(
        Math.min(Math.max(startH + (startY - m.clientY), MIN_H), window.innerHeight * 0.8),
      );
    const onUp = () => {
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
    };
    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
  };

  return (
    <section className="termpanel" style={{ height, display: open ? "flex" : "none" }}>
      <div className="termresize" onMouseDown={startDrag} />
      <header className="termhead">
        <span>{text.terminal.title}</span>
        <span className="termpath" title={path}>
          {path}
        </span>
        <button className="ghost" onClick={onClose} title={text.terminal.hide}>
          ×
        </button>
      </header>
      <div className="termhost" ref={host} />
      {popup && (
        <ul
          className="termcomplete"
          role="listbox"
          style={{ left: popup.left, top: popup.top ?? undefined, bottom: popup.bottom ?? undefined }}
        >
          {popup.items.map((item, i) => (
            <li
              key={item}
              role="option"
              aria-selected={i === popup.index}
              className={i === popup.index ? "active" : undefined}
              onMouseDown={(e) => {
                e.preventDefault(); // keep the terminal focused
                acceptRef.current(popup, i);
              }}
            >
              {item}
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
