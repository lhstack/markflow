# MarkFlow

MarkFlow 是一个基于 `Rust + Vue 3` 的轻量文档系统，核心结构为：`用户 -> 项目 -> 文档树`。

它支持项目卡片管理、目录/文档树编辑、Markdown 实时编辑预览、受控分享（密码/过期时间）、账号安全（验证码 + 2FA）以及前后端一体化部署。
## 演示图
<img width="2538" height="1128" alt="025d7c6f9412ff96b32115303e4c8883" src="https://github.com/user-attachments/assets/4d56cbb1-7a3c-4b9c-885f-e6a85613668f" />
<img width="2557" height="1042" alt="d7fcb8133c9393a19493270dee666756" src="https://github.com/user-attachments/assets/cc02966c-211d-4ab0-981a-90a6c7f313f4" />
<img width="2553" height="1247" alt="0c1c137b0c44893a5c46fd98981496d8" src="https://github.com/user-attachments/assets/af1e7d9f-4520-457c-afdd-7ceb1ca491e4" />

## 当前版本

当前版本：`v1.0.3`

以下版本说明基于 git 实际提交整理。

### v1.0.3

基于 `v1.0.2..HEAD` 的提交，`v1.0.3` 重点是智能体能力落地：

- 新增 AI 助手后端接口与前端对话面板，支持在页面内直接进行智能问答和文档协作
- 补全 function calling，覆盖页面状态读取、路由导航、项目管理、文档树操作、文档读写、浏览器与编辑器运行时
- 新增文档/目录移动能力，支持将节点移动到目标目录或项目根目录
- 优化工具 schema、参数描述与工具续轮逻辑，增强兼容 OpenAI 风格网关时的调用稳定性
- 为 `Responses` 失败场景补充 `Chat Completions` 工具回退能力，避免直接退化为纯聊天
- 增加当前会话上下文记忆与历史压缩策略，兼顾多轮对话连续性与 token 控制
- 支持 AI 打字机式写入 Markdown 文档，并默认写入未保存草稿，避免未经确认直接保存
- 暴露编辑器桥接对象与浏览器运行时，便于 AI 通过 JavaScript 执行表单填写、点击等前端操作
- 优化助手交互体验，支持中止生成、`Ctrl+Enter / Cmd+Enter` 发送、`Enter` 换行
- 调整项目概览页交互，允许删除最后一个项目，并移除空态中的重复新建入口

### v1.0.2

基于 `v1.0.1..v1.0.2` 的提交，`v1.0.2` 重点优化分享与文档加载链路：

- 新增 `SHARE_PASSWORD_SECRET / share_password_secret` 配置，使用服务端密钥加密保存分享密码密文
- 为分享记录补充可恢复密码字段，支持拥有者后续直接复制带密码的分享链接
- 调整分享弹窗密码复制逻辑，不再依赖浏览器 `prompt`，优先通过后端恢复密码
- 分享链接支持将密码写入 URL hash，并在分享页自动填充校验后清理地址栏密码片段
- 分享密码输入框支持随机生成 5 位字母数字混合密码，并改为明文可见输入
- 优化编辑区与分享页文档加载策略，树接口只返回结构信息，正文改为按选中文档懒加载
- 新增目录分享按节点加载正文接口，避免一次性下发整棵树的文档内容

### v1.0.1

基于 `v1.0.0..v1.0.1` 的提交，`v1.0.1` 重点补齐本地化资源、上传链路与后台管理能力：

- 移除 Google Fonts 外链，前端依赖资源改为本地化，编辑器与预览统一使用本地 `/vendor/vditor` 资源
- 新增离线资源准备脚本，构建与开发阶段自动复制 `vditor/dist` 到前端静态目录
- 优化 Markdown 编辑器缩进行为，改为使用 Vditor 原生缩进并插入四个空格
- 重构文件上传链路，补充附件上传、替换、删除与引用检查等附件管理能力
- 增加后台系统管理能力，包括系统配置持久化、用户管理、注册开关和上传大小限制
- 启动时自动初始化超级管理员 `admin`
- 调整登录、注册与 2FA 相关体验，完善中文错误提示
- 补充 Docker 构建与发布链路文档，完善一体化部署说明

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

补充说明：

- 前端构建前会自动准备离线资源，把 `vditor/dist` 复制到 `frontend/public/vendor/vditor/dist`
- `Vditor` 编辑器和预览渲染都走本地 `/vendor/vditor` 资源，不依赖外网 CDN
- `Element Plus`、`@element-plus/icons-vue` 等前端依赖通过 npm/bun 本地安装后直接参与打包
- 已移除 `Google Fonts` 外链，运行时不再请求 `fonts.googleapis.com` / `fonts.gstatic.com`

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
share_password_secret = "change_me_to_a_long_random_string_for_share_password_encryption"
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
- `SHARE_PASSWORD_SECRET`
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
- 分享密码使用两种形式保存：
  - `password_hash` 用于访问校验
  - `password_ciphertext` 用于拥有者后续再次复制分享链接密码
- 分享密码不会以明文直接落库，而是通过 `SHARE_PASSWORD_SECRET` / `share_password_secret` 进行服务端加密后保存

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
- `GET /api/shares/:id/password`
- `DELETE /api/shares/:id`
- `GET /api/s/:token`
- `POST /api/s/:token/verify`
- `GET /api/s/:token/content`
- `GET /api/s/:token/nodes/:node_id/content`

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

## 静态资源缓存问题
### nginx
```nginx
http {
   map $request_method $file_cache_control {
    default "";
    GET     "public, max-age=86400";
    HEAD    "public, max-age=86400";
   }
   
   location /uploads/files/ {
       proxy_pass http://backend; #你的反向代理地址00000000000000000000000000000000000000000000000000
   
       proxy_hide_header Cache-Control;
       proxy_hide_header Expires;
   
       expires 1d;
       add_header Cache-Control $file_cache_control always;
   }
}
```
### caddy
```caddy
你的域名 {
    @uploads path /uploads/files/*
    @uploads_cache {
        path /uploads/files/*
        method GET HEAD
    }

    header @uploads_cache {
        -Cache-Control
        -Expires
        Cache-Control "public, max-age=86400"
    }

    reverse_proxy @uploads 你的后端服务
}
```
## License
[LICENSE](./LICENSE)
