import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";
import styles from "./Toast.module.css";

export type ToastKind = "ok" | "error";

type ToastItem = {
  id: number;
  text: string;
  kind: ToastKind;
};

type ToastApi = {
  showToast: (text: string, kind?: ToastKind) => void;
};

const ToastContext = createContext<ToastApi | null>(null);

export function ToastProvider({ children }: { children: ReactNode }) {
  const [items, setItems] = useState<ToastItem[]>([]);
  const timers = useRef(new Map<number, number>());
  const seq = useRef(0);

  const dismiss = useCallback((id: number) => {
    const timer = timers.current.get(id);
    if (timer) {
      window.clearTimeout(timer);
      timers.current.delete(id);
    }
    setItems((prev) => prev.filter((t) => t.id !== id));
  }, []);

  const showToast = useCallback(
    (text: string, kind: ToastKind = "ok") => {
      const id = ++seq.current;
      setItems((prev) => [...prev, { id, text, kind }]);
      const timer = window.setTimeout(
        () => dismiss(id),
        kind === "ok" ? 1500 : 3500,
      );
      timers.current.set(id, timer);
    },
    [dismiss],
  );

  useEffect(() => {
    return () => {
      for (const timer of timers.current.values()) {
        window.clearTimeout(timer);
      }
      timers.current.clear();
    };
  }, []);

  const api = useMemo(() => ({ showToast }), [showToast]);

  return (
    <ToastContext.Provider value={api}>
      {children}
      <div className={styles.viewport} aria-live="polite">
        {items.map((item) => (
          <div
            key={item.id}
            className={item.kind === "error" ? styles.toastError : styles.toast}
            role={item.kind === "error" ? "alert" : "status"}
          >
            {item.text}
          </div>
        ))}
      </div>
    </ToastContext.Provider>
  );
}

export function useToast(): ToastApi {
  const ctx = useContext(ToastContext);
  if (!ctx) {
    throw new Error("useToast must be used within ToastProvider");
  }
  return ctx;
}
