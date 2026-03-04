# MarkFlow

一个轻量、现代化的 Markdown 文档协作系统，包含文档树管理、实时编辑预览、受控分享、账号安全（验证码 + 2FA）与一体化打包部署能力。

## 功能概览

### 文档管理
- 文档树（目录 + 文档）结构化管理，支持无限层级。
- 新建/重命名/删除目录与文档。
- 文档与目录拖拽移动（含顺序与父级调整）。
- 目录统计信息展示（文档数、子目录数、最近更新时间等）。

### Markdown 编辑体验
- 基于 `@kangc/v-md-editor` 的 Markdown 编辑器。
- 编辑/分栏/预览模式切换。
- 代码块高亮（`highlight.js`）。
- 编辑页与预览页视觉风格统一，支持系统主题变量。
- 解决了常见输入中断/自动刷新导致无法继续编辑的问题。

### 分享能力
- 支持文档分享与目录分享。
- 支持分享密码保护（后端存储为哈希）。
- 支持有效期：永久、1天、7天、30天、自定义到具体时间。
- 分享内容过期后不可访问（后端与前端双重校验）。
- 密码分享支持“输入一次后浏览器缓存记住（localStorage）”，刷新后无需重复输入。
- 支持复制：
  - 纯链接
  - 链接 + 文档/目录名称 + 密码（便于 IM/邮件投递）

### 认证与安全
- 用户注册/登录。
- 登录验证码（算术验证码，5分钟有效）。
- JWT 鉴权（默认 7 天有效）。
- 两步验证（2FA / TOTP）：
  - 登录第一步仅校验用户名+密码+验证码。
  - 命中 2FA 用户后跳转独立 2FA 页面（挑战码 `challenge_id`，5分钟有效）。
- 个人中心支持启用/关闭 2FA。

### 个人资料
- 支持头像上传与更新。
- 头像以 Base64 DataURL 形式存储在 SQLite `users.avatar` 字段。

### 日志与运维
- 后端日志时间格式统一为 `yyyy-MM-dd HH:mm:ss`。
- 支持控制台日志 + 文件日志。
- 文件日志支持按大小/按天切割，并支持保留天数清理。
- 日志参数可由 `config.toml` 与环境变量配置（环境变量优先级更高）。

### 打包与部署
- 后端通过 `rust-embed` 嵌入前端 `dist` 静态资源，单进程提供 API + 前端页面。
- 适合本地离线部署和单文件分发场景（Windows 可编译 `markflow.exe`）。

---

## 技术栈

### 前端
- Vue 3 + TypeScript + Vite
- Pinia
- Vue Router
- Element Plus
- `@kangc/v-md-editor` + `highlight.js`
- Axios

### 后端
- Rust
- Axum
- SQLx + SQLite
- JWT（`jsonwebtoken`）
- BCrypt
- TOTP（`totp-rs`）
- Tracing + 自定义文件滚动日志

---

## 项目结构

```text
markflow/
├─ frontend/                # Vue 前端
│  ├─ src/
│  ├─ public/
│  └─ package.json
└─ backend/                 # Rust 后端
   ├─ src/
   ├─ config.toml
   └─ Cargo.toml
```

---

## 快速开始

## 环境要求
- Node.js 18+
- Rust stable（建议 1.75+）
- Cargo

## 1) 启动前端开发环境

```bash
cd frontend
npm install
npm run dev
```

也可使用 bun：

```bash
cd frontend
bun install
bun run dev
```

默认前端地址：`http://localhost:5173`

## 2) 启动后端开发环境

```bash
cd backend
cargo run
```

默认后端地址：`http://localhost:3000`

> 前后端联调时，前端通过 `/api` 调用后端接口。

---

## 一体化打包（推荐发布方式）

后端通过 `rust-embed` 读取 `../frontend/dist`，因此发布前先构建前端：

```bash
cd frontend
npm run build

cd ../backend
cargo build --release
```

产物：`backend/target/release/markflow(.exe)`

将 `config.toml` 放在可执行文件同目录即可生效。

---

## 配置说明

后端读取顺序：**环境变量 > `config.toml` > 默认值**。

`backend/config.toml` 示例：

```toml
port = "3000"
database_url = "sqlite:markflow.db"
jwt_secret = "change_me_to_a_long_random_string_in_production"
rust_log = "markflow=info,tower_http=warn"

log_to_file = true
log_dir = "logs"
log_file_name = "markflow.log"
log_rotate_size_mb = 50
log_rotate_days = 1
log_keep_days = 14
```

### 关键环境变量

- `PORT`
- `DATABASE_URL`
- `JWT_SECRET`
- `RUST_LOG`
- `LOG_TO_FILE`
- `LOG_DIR`
- `LOG_FILE_NAME`
- `LOG_ROTATE_SIZE_MB`
- `LOG_ROTATE_DAYS`
- `LOG_KEEP_DAYS`

---

## 数据存储说明

默认 SQLite 文件：`markflow.db`（由 `database_url` 决定）。

核心表：
- `users`
- `doc_nodes`
- `shares`

说明：
- 用户头像存储在 `users.avatar`（Base64 DataURL）。
- 分享密码仅存储哈希（`shares.password_hash`），不存明文。

---

## 主要 API（摘要）

### Auth
- `GET /api/auth/captcha`
- `POST /api/auth/register`
- `POST /api/auth/login`
- `POST /api/auth/login/2fa`
- `GET /api/auth/me`
- `PUT /api/auth/profile`
- `PUT /api/auth/password`
- `POST /api/auth/2fa/setup`
- `POST /api/auth/2fa/confirm`
- `POST /api/auth/2fa/disable`

### Docs
- `GET /api/docs`
- `POST /api/docs`
- `GET /api/docs/:id`
- `PUT /api/docs/:id`
- `DELETE /api/docs/:id`
- `PUT /api/docs/:id/move`

### Shares
- `POST /api/shares`
- `GET /api/shares/doc/:doc_id`
- `DELETE /api/shares/:id`
- `GET /api/s/:token`
- `POST /api/s/:token/verify`
- `GET /api/s/:token/content`

---

## 安全建议（生产）

- 必须修改 `JWT_SECRET` 为高强度随机值。
- 通过反向代理（Nginx/Caddy）启用 HTTPS。
- 关闭开发环境跨域放开策略，限制允许来源。
- 为数据库与日志目录配置备份与权限控制。

---

## 常见问题

### Q1：为什么目录页面显示“目录为空”？
表示该目录下当前没有直属子文档/子目录；可在该目录下新建文档或子目录。

### Q2：为什么分享链接第一次打开需要密码，刷新后还要再输？
当前实现已支持浏览器本地缓存密码（`localStorage`），在缓存未清理前可复用。

### Q3：链接已过期却还能打开？
后端会在 `/api/s/:token` 与 `/api/s/:token/content` 两处校验过期时间，若发现异常请确认服务端系统时间与时区配置。

---

## License

当前仓库未附带明确开源协议；如需开源发布，请补充 `LICENSE` 文件并声明授权条款。
