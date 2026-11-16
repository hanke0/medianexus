# MediaNexus

影视自动化追踪：TMDB 浏览 → M-Team 搜种 → qBittorrent 下载 → hardlink 入库 → Plex/Jellyfin 扫描。

架构参考 shelfie：Rust (Axum + SQLite) 后端 + React 前端。

## 功能

1. 从 TMDB 拉取最新/热门电影与电视剧并展示
2. 用户点击「追踪」后自动在 M-Team 搜种并下载 `.torrent`
3. 上传到 qBittorrent（分类可配置，默认 `medianexus`），后台轮询进度
4. 下载完成后 hardlink 到媒体库目录（电影 → `library/movies/`，其余默认 → `library/tv/`），并触发 Plex / Jellyfin 扫描
5. SQLite 用户系统；管理员可在网页配置外部服务与新建用户
6. 可同时追踪多部内容；同一标题仍不可重复追踪

## 快速开始

```bash
# 后端（默认监听 127.0.0.1 与局域网私网地址，不监听公网；库文件 ./medianexus.db，入库目录 ./nexusmedia）
cargo run -p medianexus-backend

# 前端
npm install
npm run dev:frontend
```

登录后打开 **设置** 页面配置 TMDB / M-Team / qB / Plex / Jellyfin。

- 前端：http://127.0.0.1:5173  
- Swagger：http://127.0.0.1:8080/swagger-ui  
- 默认管理员：`admin` / `admin123`

## CLI / 环境变量

优先级：CLI 参数 > 进程环境变量 > 默认值。不会自动读取 `.env` 文件。

```bash
cargo run -p medianexus-backend -- \
  --host lan \
  --port 8080 \
  --database-url 'sqlite:medianexus.db?mode=rwc' \
  --library-dir nexusmedia \
  --jwt-secret 'your-secret' \
  --admin-username admin \
  --admin-password admin123
```

| 项 | CLI | Env | 默认 |
|----|-----|-----|------|
| 监听地址 | `--host` | `HOST` | `lan` |
| 端口 | `--port` | `PORT` | `8080` |
| 数据库 | `--database-url` / `--db` | `DATABASE_URL` | `sqlite:medianexus.db?mode=rwc` |
| 入库目录 | `--library-dir` / `--library` | `LIBRARY_DIR` | `./nexusmedia`（自动创建 `movies/`、`tv/`） |
| JWT | `--jwt-secret` | `JWT_SECRET` | **未设置则每次启动随机生成** |
| 初始管理员 | `--admin-username` | `ADMIN_USERNAME` | `admin` |
| 初始密码 | `--admin-password` | `ADMIN_PASSWORD` | `admin123` |
| 版本 | `-V` / `--version` | — | 打印 `medianexus-backend <version>` 后退出 |

`lan` 会同时绑定 `127.0.0.1`、网卡上已有的 `::1`，以及私网地址（`10.0.0.0/8`、`172.16.0.0/12`、`192.168.0.0/16`、IPv6 ULA `fc00::/7`），不包含公网网卡。要连公网一起听，用 `--host 0.0.0.0`（全部 IPv4）或 `--host ::`（全部 IPv6），也可以指定某个公网地址。

开发时 Vite 只监听 `127.0.0.1:5173`，`/api` 由开发机代理到本机 `127.0.0.1:8080`。

外部服务（TMDB / M-Team / qB / Plex / Jellyfin）保存在 SQLite `settings` 表，由网页管理。

```bash
cargo run -p medianexus-backend -- -V
# medianexus-backend 0.1.0
```

CI（`.github/workflows/ci.yml`）在 `main` / PR 上跑 fmt、clippy、cargo test、前端 typecheck；`main` 推送与 `v*` tag 会构建嵌入前端的 release 二进制，tag 发布时上传到 GitHub Release。

## 自动化流水线

`pending → searching → torrent_found → downloading → hardlinking → scanning → completed`

## 安全提醒

请勿把密钥提交到 git。若曾在聊天中暴露，请尽快轮换。
