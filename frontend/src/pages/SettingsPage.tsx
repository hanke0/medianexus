import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useRef, useState, type ChangeEvent } from "react";
import { Navigate } from "react-router-dom";
import { Select } from "@/components/Select";
import { useToast } from "@/components/Toast";
import {
  ApiError,
  endpoints,
  type ConnectionTarget,
  type IntegrationSettings,
  type PlexLinkStatus,
} from "@/lib/api";
import { getUser, isAdmin } from "@/lib/auth";
import styles from "./SettingsPage.module.css";

const SAVE_DEBOUNCE_MS = 500;

const empty: IntegrationSettings = {
  tmdb_api_key: "",
  tmdb_language: "zh-CN",
  mdblist_api_key: "",
  hardlink_folder_template: "{{title}}-{{year}}",
  mteam_api_key: "",
  mteam_api_base: "https://api.m-team.cc",
  qb_url: "http://127.0.0.1:8081",
  qb_username: "admin",
  qb_password: "",
  qb_category: "medianexus",
  qb_save_path: "",
  plex_url: "",
  plex_linked: false,
  plex_token_expires_at: "",
  jellyfin_url: "",
  jellyfin_api_key: "",
  pt_primary_site: "",
  schedule_search_interval_secs: 10800,
  schedule_download_interval_secs: 60,
  schedule_library_scan_interval_secs: 86400,
  schedule_browse_warm_at: "00:01",
};

const PT_SITE_OPTIONS = [
  { value: "", label: "全部已配置站点" },
  { value: "mteam", label: "M-Team" },
];

const TMDB_LANGUAGE_OPTIONS = [
  { value: "zh-CN", label: "简体中文 (zh-CN)" },
  { value: "zh-TW", label: "繁体中文 (zh-TW)" },
  { value: "en-US", label: "English (en-US)" },
  { value: "ja-JP", label: "日本語 (ja-JP)" },
  { value: "ko-KR", label: "한국어 (ko-KR)" },
  { value: "fr-FR", label: "Français (fr-FR)" },
  { value: "de-DE", label: "Deutsch (de-DE)" },
  { value: "es-ES", label: "Español (es-ES)" },
  { value: "pt-BR", label: "Português (pt-BR)" },
  { value: "ru-RU", label: "Русский (ru-RU)" },
];

function tmdbLanguageOptions(current: string) {
  if (!current || TMDB_LANGUAGE_OPTIONS.some((o) => o.value === current)) {
    return TMDB_LANGUAGE_OPTIONS;
  }
  return [{ value: current, label: current }, ...TMDB_LANGUAGE_OPTIONS];
}

export function SettingsPage() {
  const user = getUser();
  const allowed = isAdmin(user);
  const queryClient = useQueryClient();
  const { showToast } = useToast();
  const [form, setForm] = useState<IntegrationSettings>(empty);
  const [plexStatus, setPlexStatus] = useState<PlexLinkStatus | null>(null);
  const [plexBusy, setPlexBusy] = useState(false);
  const [testing, setTesting] = useState<ConnectionTarget | null>(null);
  const formRef = useRef(form);
  const saveTimer = useRef<number | undefined>(undefined);
  formRef.current = form;

  const query = useQuery({
    queryKey: ["settings"],
    queryFn: endpoints.settings,
    enabled: allowed,
  });

  useEffect(() => {
    if (query.data) {
      setForm(query.data);
    }
  }, [query.data]);

  useEffect(() => {
    if (!allowed) return;
    let cancelled = false;
    let timer: number | undefined;

    const tick = async () => {
      try {
        const status = await endpoints.plexLinkStatus();
        if (cancelled) return;
        setPlexStatus(status);
        if (status.linked) {
          setForm((prev) => ({
            ...prev,
            plex_linked: true,
            plex_token_expires_at: status.expires_at
              ? String(status.expires_at)
              : prev.plex_token_expires_at,
          }));
        }
        if (status.pending) {
          timer = window.setTimeout(tick, 2000);
        }
      } catch {
        // Keep trying while a link attempt is in flight; expired PIN is cleared server-side.
        if (!cancelled && plexStatus?.pending) {
          timer = window.setTimeout(tick, 3000);
        }
      }
    };

    void tick();
    return () => {
      cancelled = true;
      if (timer) window.clearTimeout(timer);
    };
  }, [allowed, plexStatus?.pending]);

  useEffect(() => {
    return () => {
      if (saveTimer.current) window.clearTimeout(saveTimer.current);
    };
  }, []);

  const saveMutation = useMutation({
    mutationFn: (data: IntegrationSettings) => endpoints.updateSettings(data),
    onSuccess: (data) => {
      setForm(data);
      showToast("已保存");
      queryClient.setQueryData(["settings"], data);
    },
    onError: (err: unknown) => {
      showToast(err instanceof ApiError ? err.message : "保存失败", "error");
    },
  });

  if (!allowed) {
    return <Navigate to="/" replace />;
  }

  const scheduleSave = (next: IntegrationSettings) => {
    if (saveTimer.current) window.clearTimeout(saveTimer.current);
    saveTimer.current = window.setTimeout(() => {
      saveMutation.mutate(next);
    }, SAVE_DEBOUNCE_MS);
  };

  const patch = (partial: Partial<IntegrationSettings>) => {
    setForm((prev) => {
      const next = { ...prev, ...partial };
      scheduleSave(next);
      return next;
    });
  };

  const set =
    (key: keyof IntegrationSettings) =>
    (e: ChangeEvent<HTMLInputElement>) => {
      patch({ [key]: e.target.value } as Partial<IntegrationSettings>);
    };

  const setIntervalMinutes =
    (key: keyof IntegrationSettings) =>
    (e: ChangeEvent<HTMLInputElement>) => {
      const minutes = Math.max(1, Number(e.target.value) || 1);
      patch({ [key]: minutes * 60 } as Partial<IntegrationSettings>);
    };

  const flushSave = async () => {
    if (saveTimer.current) {
      window.clearTimeout(saveTimer.current);
      saveTimer.current = undefined;
    }
    return endpoints.updateSettings(formRef.current);
  };

  const runConnectionTest = async (target: ConnectionTarget) => {
    setTesting(target);
    try {
      const saved = await flushSave();
      setForm(saved);
      queryClient.setQueryData(["settings"], saved);
      const result = await endpoints.testConnection(target);
      showToast(result.message, result.ok ? "ok" : "error");
    } catch (err) {
      showToast(err instanceof ApiError ? err.message : "连接测试失败", "error");
    } finally {
      setTesting(null);
    }
  };

  return (
    <div className={styles.page}>
      {query.isLoading && <p className="notice">加载中…</p>}

      <div className={styles.form}>
        <fieldset>
          <legend>TMDB</legend>
          <label className="field">
            <span>API Key</span>
            <input
              type="password"
              value={form.tmdb_api_key}
              onChange={set("tmdb_api_key")}
              autoComplete="off"
            />
          </label>
          <label className="field">
            <span>语言</span>
            <Select
              value={form.tmdb_language || "zh-CN"}
              onChange={(v) => patch({ tmdb_language: v })}
              options={tmdbLanguageOptions(form.tmdb_language)}
              aria-label="TMDB 语言"
            />
          </label>
          <label className="field">
            <span className={styles.labelWithHint}>
              入库文件夹命名
              <span
                className={styles.hintMark}
                tabIndex={0}
                aria-label="可用占位符：{{title}}、{{year}}、{{original_title}}、{{tmdb_id}}"
                data-tip="{{title}}、{{year}}、{{original_title}}、{{tmdb_id}}"
              >
                !
              </span>
            </span>
            <input
              value={form.hardlink_folder_template}
              onChange={set("hardlink_folder_template")}
              placeholder="{{title}}-{{year}}"
            />
          </label>
        </fieldset>

        <fieldset>
          <legend>评分</legend>
          <label className="field">
            <span>MDBList API Key</span>
            <input
              type="password"
              value={form.mdblist_api_key}
              onChange={set("mdblist_api_key")}
              autoComplete="off"
            />
          </label>
          <p className={`notice ${styles.scheduleHint}`}>
            用于展示 IMDb 和烂番茄评分。在 mdblist.com 的偏好设置里生成免费 Key。
          </p>
        </fieldset>

        <fieldset>
          <legend>PT 站点</legend>
          <label className="field">
            <span>优先站点</span>
            <Select
              value={form.pt_primary_site}
              onChange={(v) => patch({ pt_primary_site: v })}
              options={PT_SITE_OPTIONS}
            />
          </label>
        </fieldset>

        <fieldset>
          <legend>M-Team</legend>
          <label className="field">
            <span>API Key</span>
            <input
              type="password"
              value={form.mteam_api_key}
              onChange={set("mteam_api_key")}
              autoComplete="off"
            />
          </label>
          <label className="field">
            <span>API Base</span>
            <input value={form.mteam_api_base} onChange={set("mteam_api_base")} />
          </label>
          <div className={styles.testRow}>
            <button
              type="button"
              className="btn ghost"
              disabled={testing !== null}
              onClick={() => void runConnectionTest("mteam")}
            >
              {testing === "mteam" ? "测试中…" : "测试连接"}
            </button>
          </div>
        </fieldset>

        <fieldset>
          <legend>qBittorrent</legend>
          <label className="field">
            <span>WebUI URL</span>
            <input value={form.qb_url} onChange={set("qb_url")} />
          </label>
          <label className="field">
            <span>用户名</span>
            <input value={form.qb_username} onChange={set("qb_username")} />
          </label>
          <label className="field">
            <span>密码</span>
            <input
              type="password"
              value={form.qb_password}
              onChange={set("qb_password")}
              autoComplete="new-password"
            />
          </label>
          <label className="field">
            <span>分类</span>
            <input value={form.qb_category} onChange={set("qb_category")} />
          </label>
          <label className="field">
            <span>保存路径</span>
            <input value={form.qb_save_path} onChange={set("qb_save_path")} />
          </label>
          <div className={styles.testRow}>
            <button
              type="button"
              className="btn ghost"
              disabled={testing !== null}
              onClick={() => void runConnectionTest("qbittorrent")}
            >
              {testing === "qbittorrent" ? "测试中…" : "测试连接"}
            </button>
          </div>
        </fieldset>

        <fieldset>
          <legend>Plex</legend>
          <label className="field">
            <span>URL</span>
            <input value={form.plex_url} onChange={set("plex_url")} placeholder="http://127.0.0.1:32400" />
          </label>
          <div className="field">
            <span>账号</span>
            {form.plex_linked ? (
              <button
                type="button"
                className="btn ghost"
                disabled={plexBusy}
                onClick={async () => {
                  setPlexBusy(true);
                  try {
                    const status = await endpoints.plexUnlink();
                    setPlexStatus(status);
                    setForm((prev) => ({
                      ...prev,
                      plex_linked: false,
                      plex_token_expires_at: "",
                    }));
                    showToast("已解除 Plex 关联");
                  } catch (err) {
                    showToast(
                      err instanceof ApiError ? err.message : "解除关联失败",
                      "error",
                    );
                  } finally {
                    setPlexBusy(false);
                  }
                }}
              >
                解除关联
              </button>
            ) : (
              <button
                type="button"
                className="btn"
                disabled={plexBusy}
                onClick={async () => {
                  setPlexBusy(true);
                  try {
                    // Always create a fresh PIN so an expired pending state cannot block login.
                    const forward = `${window.location.origin}/admin/settings`;
                    const started = await endpoints.plexLinkStart(forward);
                    setPlexStatus({
                      linked: false,
                      pending: true,
                      pin_code: started.pin_code,
                      auth_url: started.auth_url,
                    });
                    window.open(started.auth_url, "_blank", "noopener,noreferrer");
                  } catch (err) {
                    showToast(
                      err instanceof ApiError ? err.message : "发起 Plex 授权失败",
                      "error",
                    );
                  } finally {
                    setPlexBusy(false);
                  }
                }}
              >
                {plexStatus?.pending ? "等待授权中…" : "用 Plex 账号登录"}
              </button>
            )}
          </div>
          <div className={styles.testRow}>
            <button
              type="button"
              className="btn ghost"
              disabled={testing !== null}
              onClick={() => void runConnectionTest("plex")}
            >
              {testing === "plex" ? "测试中…" : "测试连接"}
            </button>
          </div>
        </fieldset>

        <fieldset>
          <legend>Jellyfin</legend>
          <label className="field">
            <span>URL</span>
            <input value={form.jellyfin_url} onChange={set("jellyfin_url")} placeholder="http://127.0.0.1:8096" />
          </label>
          <label className="field">
            <span>API Key</span>
            <input
              type="password"
              value={form.jellyfin_api_key}
              onChange={set("jellyfin_api_key")}
              autoComplete="off"
            />
          </label>
          <div className={styles.testRow}>
            <button
              type="button"
              className="btn ghost"
              disabled={testing !== null}
              onClick={() => void runConnectionTest("jellyfin")}
            >
              {testing === "jellyfin" ? "测试中…" : "测试连接"}
            </button>
          </div>
        </fieldset>

        <fieldset>
          <legend>定时任务</legend>
          <p className={`notice ${styles.scheduleHint}`}>
            间隔最少 1 分钟，任务从每天 0:00 起按间隔对齐执行。首页列表缓存每天在设定时间刷新；留空则不自动刷新，缓存在 0 点过期。
          </p>
          <label className="field">
            <span>搜种间隔（分钟）</span>
            <input
              type="number"
              min={1}
              value={Math.max(1, Math.round(form.schedule_search_interval_secs / 60))}
              onChange={setIntervalMinutes("schedule_search_interval_secs")}
            />
          </label>
          <label className="field">
            <span>下载状态间隔（分钟）</span>
            <input
              type="number"
              min={1}
              value={Math.max(1, Math.round(form.schedule_download_interval_secs / 60))}
              onChange={setIntervalMinutes("schedule_download_interval_secs")}
            />
          </label>
          <label className="field">
            <span>影视库扫描间隔（分钟）</span>
            <input
              type="number"
              min={1}
              value={Math.max(1, Math.round(form.schedule_library_scan_interval_secs / 60))}
              onChange={setIntervalMinutes("schedule_library_scan_interval_secs")}
            />
          </label>
          <label className="field">
            <span>首页缓存刷新时间（每天，留空关闭）</span>
            <input
              type="time"
              value={form.schedule_browse_warm_at}
              onChange={set("schedule_browse_warm_at")}
            />
          </label>
        </fieldset>
      </div>
    </div>
  );
}
