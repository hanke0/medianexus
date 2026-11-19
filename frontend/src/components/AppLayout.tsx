import { useEffect, useRef, useState } from "react";
import { NavLink, Outlet, ScrollRestoration, useNavigate } from "react-router-dom";
import { clearAuth, getUser, isAdmin } from "@/lib/auth";
import styles from "./AppLayout.module.css";

export function AppLayout() {
  const navigate = useNavigate();
  const user = getUser();
  const [menuOpen, setMenuOpen] = useState(false);
  const menuRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!menuOpen) return;

    const onPointerDown = (e: PointerEvent) => {
      if (!menuRef.current?.contains(e.target as Node)) {
        setMenuOpen(false);
      }
    };
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") setMenuOpen(false);
    };

    document.addEventListener("pointerdown", onPointerDown);
    document.addEventListener("keydown", onKeyDown);
    return () => {
      document.removeEventListener("pointerdown", onPointerDown);
      document.removeEventListener("keydown", onKeyDown);
    };
  }, [menuOpen]);

  const logout = () => {
    setMenuOpen(false);
    clearAuth();
    navigate("/login");
  };

  return (
    <div className={styles.shell}>
      <header className={styles.header}>
        <NavLink to="/" className={styles.brand}>
          <span className={styles.mark}>MN</span>
          <span className={styles.name}>MediaNexus</span>
        </NavLink>
        <nav className={styles.nav}>
          <NavLink to="/" end>
            发现
          </NavLink>
          <NavLink to="/tracks">追踪</NavLink>
          {isAdmin(user) && <NavLink to="/admin/settings">设置</NavLink>}
          {isAdmin(user) && <NavLink to="/admin/maintenance">维护</NavLink>}
          {isAdmin(user) && <NavLink to="/admin/users">用户</NavLink>}
        </nav>
        <div className={styles.user} ref={menuRef}>
          <button
            type="button"
            className={styles.userBtn}
            aria-haspopup="menu"
            aria-expanded={menuOpen}
            onClick={() => setMenuOpen((open) => !open)}
          >
            <span className={styles.userName}>{user?.username}</span>
            <span className={styles.chevron} aria-hidden>
              ▾
            </span>
          </button>
          {menuOpen && (
            <div className={styles.menu} role="menu">
              <button
                type="button"
                className={styles.menuItem}
                role="menuitem"
                onClick={logout}
              >
                退出登录
              </button>
            </div>
          )}
        </div>
      </header>
      <main className={styles.main}>
        <Outlet />
      </main>
      <ScrollRestoration />
    </div>
  );
}
