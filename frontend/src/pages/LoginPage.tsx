import { useState, type FormEvent } from "react";
import { useNavigate } from "react-router-dom";
import { endpoints } from "@/lib/api";
import { setAuth } from "@/lib/auth";
import styles from "./LoginPage.module.css";

export function LoginPage() {
  const navigate = useNavigate();
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  const handleSubmit = async (e: FormEvent) => {
    e.preventDefault();
    setError(null);
    setLoading(true);
    try {
      const res = await endpoints.login(username, password);
      setAuth(res.token, res.user);
      navigate("/");
    } catch (err) {
      setError(err instanceof Error ? err.message : "登录失败");
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className={styles.page}>
      <div className={styles.hero}>
        <p className={styles.eyebrow}>TMDB · M-Team · qBittorrent</p>
        <h1>MediaNexus</h1>
        <p className={styles.sub}>一点追踪，自动搜种、下载、入库。</p>
      </div>
      <form className={styles.form} onSubmit={handleSubmit}>
        <label className="field">
          <span>用户名</span>
          <input
            value={username}
            onChange={(e) => setUsername(e.target.value)}
            autoComplete="username"
            required
          />
        </label>
        <label className="field">
          <span>密码</span>
          <input
            type="password"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            autoComplete="current-password"
            required
          />
        </label>
        {error && <p className={styles.error}>{error}</p>}
        <button type="submit" className="btn" disabled={loading}>
          {loading ? "登录中…" : "登录"}
        </button>
      </form>
    </div>
  );
}
