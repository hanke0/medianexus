import { useEffect, useState, type ReactNode } from "react";
import { Navigate } from "react-router-dom";
import { endpoints } from "@/lib/api";
import { clearAuth, getToken } from "@/lib/auth";

export function ProtectedRoute({ children }: { children: ReactNode }) {
  const [state, setState] = useState<"checking" | "ok" | "fail">(() =>
    getToken() ? "checking" : "fail",
  );

  useEffect(() => {
    if (!getToken()) {
      setState("fail");
      return;
    }

    let cancelled = false;
    void endpoints
      .me()
      .then(() => {
        if (!cancelled) setState("ok");
      })
      .catch(() => {
        clearAuth();
        if (!cancelled) setState("fail");
      });

    return () => {
      cancelled = true;
    };
  }, []);

  if (state === "checking") {
    return null;
  }
  if (state === "fail") {
    return <Navigate to="/login" replace />;
  }
  return children;
}
