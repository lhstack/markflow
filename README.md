# MarkFlow

MarkFlow 是一个基于 `Rust + Vue 3` 的轻量文档系统，核心结构为：`用户 -> 项目 -> 文档树`。

它支持项目卡片管理、目录/文档树编辑、Markdown 实时编辑预览、受控分享（密码/过期时间）、账号安全（验证码 + 2FA）以及前后端一体化部署。

## 核心能力

- 项目层级管理
- 项目概览卡片页（分页网格、背景图上传、编辑/删除）
- 项目名称重名校验（新增与编辑，前后端双重校验）
- 创建项目后停留在概览页（不再自动跳入项目）

- 文档树管理
- 目录与文档的新增、重命名、删除
- 拖拽排序（同级重排）
- 拖拽跨目录移动（目录内/目录外/根级）
- 树区域空白处右键菜单

- 编辑与预览
- Markdown 编辑、分栏预览、纯预览
- 代码高亮（`highlight.js`）
- 目录节点统计展示

- 分享能力
- 文档分享与目录分享
- 分享密码校验（哈希存储）
- 分享有效期控制
- 分享页目录可展开/收起
- 分享页文档目录浮层定位与标题定位跳转
- 可直接预览的附件在新窗口打开

- 状态缓存（刷新不重置）
- 首页侧边栏展开状态缓存
- 文档树目录展开状态缓存（按项目隔离）
- 分享页侧栏/目录展开状态缓存（按 token 隔离）
- 首页支持恢复上次项目与文档上下文

- 认证与安全
- 注册/登录
- 注册开关（支持后台动态启停）
- 登录验证码
- JWT 鉴权
- 2FA（TOTP）
- 头像上传
- 登录/注册/2FA 中文错误提示

- 系统管理
- 启动时自动初始化超级管理员 `admin`
- 系统配置持久化到数据库
- 上传大小限制可配置
- 用户管理（启用/停用、重置密码、开关 2FA、新增/删除）

- 附件与上传
- 统一上传链路（按钮/拖拽/粘贴）
- 粘贴上传去重，避免重复创建附件
- 附件管理（替换、删除、引用检查）

## 技术栈

- 前端
- Vue 3 + TypeScript + Vite
- Pinia + Vue Router
- Element Plus
- `@kangc/v-md-editor`

- 后端
- Rust + Axum
- SQLx + SQLite
- JWT + BCrypt + TOTP
- Tracing 日志（支持滚动文件日志）
- `rust-embed`（嵌入前端 dist）

## 项目结构

```text
markflow/
├─ README.md
├─ backend/
│  ├─ Cargo.toml
│  ├─ config.toml
│  └─ src/
└─ frontend/
   ├─ package.json
   ├─ bun.lock
   └─ src/
```

## 环境要求

- Rust stable（建议 1.93.1）
- Cargo
- Node.js 18+（若使用 npm）
- Bun 1.3.9（推荐）

## 本地开发

### 1) 启动后端

```bash
cd backend
cargo run
```

后端默认地址：`http://localhost:3000`

### 2) 启动前端

使用 Bun：

```bash
cd frontend
bun install
bun run dev
```

或使用 npm：

```bash
cd frontend
npm install
npm run dev
```

前端默认地址：`http://localhost:5173`

前端通过 Vite 代理把 `/api` 转发到 `http://localhost:3000`。

## 一体化发布（单进程）

发布前先构建前端：

```bash
cd frontend
bun run build
```

再构建后端：

```bash
cd backend
cargo build --release
```

产物：

- macOS / Linux: `backend/target/release/markflow`
- Windows: `backend/target/release/markflow.exe`

运行时会从可执行文件同目录读取 `config.toml`。

## 配置说明

配置优先级：

- 环境变量
- `config.toml`
- 默认值

示例（`backend/config.toml`）：

```toml
port = "3000"
database_url = "sqlite:markflow.db"
jwt_secret = "change_me_to_a_long_random_string_in_production"
rust_log = "markflow=info,tower_http=warn"
upload_dir = "uploads"

log_to_file = true
log_dir = "logs"
log_file_name = "markflow.log"
log_rotate_size_mb = 50
log_rotate_days = 1
log_keep_days = 14
registration_enabled = true
upload_max_mb = 20
```

对应环境变量：

- `PORT`
- `DATABASE_URL`
- `JWT_SECRET`
- `RUST_LOG`
- `UPLOAD_DIR`
- `LOG_TO_FILE`
- `LOG_DIR`
- `LOG_FILE_NAME`
- `LOG_ROTATE_SIZE_MB`
- `LOG_ROTATE_DAYS`
- `LOG_KEEP_DAYS`
- `REGISTRATION_ENABLED`
- `UPLOAD_MAX_MB`

## 数据存储

默认数据库：SQLite（`sqlite:markflow.db`）

核心数据表：

- `users`
- `projects`
- `doc_nodes`
- `shares`

说明：

- 文档根节点归属项目（`doc_nodes.project_id`）
- 上传文件默认保存在 `uploads/<user_id>/<yyyyMMdd>/`
- 头像、项目背景图使用上传接口保存文件并在表中存 URL
- 分享密码仅存哈希，不存明文

## API 摘要（均以 `/api` 开头）

Auth：

- `GET /api/auth/captcha`
- `GET /api/auth/public-settings`
- `POST /api/auth/register`
- `POST /api/auth/login`
- `POST /api/auth/login/2fa`
- `GET /api/auth/me`
- `PUT /api/auth/profile`
- `PUT /api/auth/password`
- `POST /api/auth/2fa/setup`
- `POST /api/auth/2fa/confirm`
- `POST /api/auth/2fa/disable`

Projects：

- `GET /api/projects`
- `POST /api/projects`
- `PUT /api/projects/:id`
- `DELETE /api/projects/:id`

Docs：

- `GET /api/docs`
- `POST /api/docs`
- `GET /api/docs/:id`
- `PUT /api/docs/:id`
- `DELETE /api/docs/:id`
- `PUT /api/docs/:id/move`

Shares：

- `POST /api/shares`
- `GET /api/shares/doc/:doc_id`
- `DELETE /api/shares/:id`
- `GET /api/s/:token`
- `POST /api/s/:token/verify`
- `GET /api/s/:token/content`

Admin：

- `GET /api/admin/system-settings`
- `PUT /api/admin/system-settings`
- `GET /api/admin/users`
- `POST /api/admin/users`
- `DELETE /api/admin/users/:id`
- `PUT /api/admin/users/:id/status`
- `PUT /api/admin/users/:id/password`
- `PUT /api/admin/users/:id/2fa`
- `GET /api/admin/users/:id/export`

前端页面路由中，分享页访问路径是 `/s/:token`（由 SPA 承载）。

## 开发校验命令

前端：

```bash
cd frontend
bun run build
```

后端：

```bash
cd backend
cargo check
```

## 本地未推送 / 未提交变更说明

以下内容已存在于当前本地代码中，其中一部分已经提交但尚未 push，另一部分仍处于未提交状态：

- 文件上传链路完成重构，新增统一上传工具、上传进度管理和附件管理能力。
- Markdown 编辑器补齐按钮上传、拖拽上传、粘贴上传，并修复粘贴触发两次上传的问题。
- 预览区和分享页增强了标题目录识别、浮动目录面板、目录状态缓存，以及附件预览在新窗口打开的行为。
- 后端新增系统配置持久化，首次启动若不存在超级管理员会自动创建固定用户名 `admin` 并在日志中输出随机初始密码。
- 新增超级管理员菜单：系统配置与用户管理；支持注册开关、上传大小限制、普通用户启用/停用、密码重置、2FA 开关、新增/删除用户与导出用户数据。
- 登录、注册、2FA 页面补充了中文错误提示映射。
- 分享页右侧文档目录已调整为浮动定位，不再影响中间正文区域宽度。
- 文档树“新建文档/目录”补充了前端防重复提交保护，避免重复点击或重复回车导致连续创建。
- 当前尚未提交的配置更新包含：`backend/config.toml` 新增 `registration_enabled` 与 `upload_max_mb` 示例配置。

## 常见问题

### 1) 刷新后为什么回到项目概览/树被重置？

已支持状态缓存与恢复：

- 首页会恢复最近项目与文档上下文
- 文档树展开状态按项目缓存
- 分享页侧栏与目录展开按分享 token 缓存

### 2) 项目名为什么不能重名？

系统已开启重名约束（忽略大小写），新增和编辑都不允许与同账号现有项目名冲突。

### 3) 为什么创建项目后没有自动进入？

当前交互设计是创建后停留在项目概览页，避免打断连续创建/管理流程。

### 4) macOS 提示“Apple 无法验证 markflow”怎么办？

这是未签名二进制在 macOS 上的常见提示。你可以在终端执行：

```bash
xattr -dr com.apple.quarantine /path/to/markflow
chmod +x /path/to/markflow
/path/to/markflow
```

请把 `/path/to/markflow` 替换成你的实际二进制路径。

## 生产建议

- 必须替换 `JWT_SECRET`
- 建议通过 Nginx/Caddy 反向代理并启用 HTTPS
- 限制 CORS 来源（当前默认开发友好配置）
- 对数据库与日志目录做备份与权限控制

## License

当前仓库未包含明确开源协议。如需开源发布，请补充 `LICENSE` 文件并声明授权条款。
