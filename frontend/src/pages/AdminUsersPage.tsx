import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { Navigate } from "react-router-dom";
import { Select } from "@/components/Select";
import { useToast } from "@/components/Toast";
import { ApiError, endpoints } from "@/lib/api";
import { getUser, isAdmin } from "@/lib/auth";
import styles from "./AdminUsersPage.module.css";

const ROLE_OPTIONS = [
  { value: "user", label: "普通用户" },
  { value: "admin", label: "管理员" },
];

export function AdminUsersPage() {
  const user = getUser();
  const allowed = isAdmin(user);
  const queryClient = useQueryClient();
  const { showToast } = useToast();
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [role, setRole] = useState("user");

  const usersQuery = useQuery({
    queryKey: ["users"],
    queryFn: endpoints.users,
    enabled: allowed,
  });

  const createMutation = useMutation({
    mutationFn: () => endpoints.createUser(username, password, role),
    onSuccess: () => {
      setUsername("");
      setPassword("");
      setRole("user");
      showToast("用户已创建");
      queryClient.invalidateQueries({ queryKey: ["users"] });
    },
    onError: (err: unknown) => {
      showToast(err instanceof ApiError ? err.message : "创建失败", "error");
    },
  });

  const deleteMutation = useMutation({
    mutationFn: (id: string) => endpoints.deleteUser(id),
    onSuccess: () => {
      showToast("用户已删除");
      queryClient.invalidateQueries({ queryKey: ["users"] });
    },
    onError: (err: unknown) => {
      showToast(err instanceof ApiError ? err.message : "删除失败", "error");
    },
  });

  if (!allowed) {
    return <Navigate to="/" replace />;
  }

  return (
    <div className={styles.page}>
      <form
        className={styles.form}
        onSubmit={(e) => {
          e.preventDefault();
          createMutation.mutate();
        }}
      >
        <label className="field">
          <span>用户名</span>
          <input
            value={username}
            onChange={(e) => setUsername(e.target.value)}
            required
          />
        </label>
        <label className="field">
          <span>密码</span>
          <input
            type="password"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            required
            minLength={4}
          />
        </label>
        <label className="field">
          <span>角色</span>
          <Select
            value={role}
            options={ROLE_OPTIONS}
            onChange={setRole}
            aria-label="角色"
          />
        </label>
        <button className="btn" type="submit" disabled={createMutation.isPending}>
          {createMutation.isPending ? "创建中…" : "新建用户"}
        </button>
      </form>

      <div className={styles.table}>
        <div className={styles.head}>
          <span>用户名</span>
          <span>角色</span>
          <span>创建时间</span>
          <span />
        </div>
        {(usersQuery.data ?? []).map((u) => (
          <div className={styles.row} key={u.id}>
            <span>{u.username}</span>
            <span className={`badge ${u.role === "admin" ? "warn" : ""}`}>
              {u.role === "admin" ? "管理员" : "用户"}
            </span>
            <span className={styles.muted}>{u.created_at}</span>
            <button
              type="button"
              className="btn ghost"
              disabled={u.id === user?.id || deleteMutation.isPending}
              onClick={() => deleteMutation.mutate(u.id)}
            >
              删除
            </button>
          </div>
        ))}
      </div>
    </div>
  );
}
