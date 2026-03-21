# MarkFlow

MarkFlow 是一个基于 `Rust + Vue 3` 的轻量文档系统，核心结构为：`用户 -> 项目 -> 文档树`。

它支持项目卡片管理、目录/文档树编辑、Markdown 实时编辑预览、受控分享（密码/过期时间）、账号安全（验证码 + 2FA）以及前后端一体化部署。
## 演示图
<img width="2553" height="1253" alt="5e45d32745dacac82f5fe439fd18dd0c" src="https://github.com/user-attachments/assets/59b44848-e240-4d75-8075-1fc8b925a683" />
<img width="2529" height="1250" alt="8756b3f4daf40354b2762204ec651a79" src="https://github.com/user-attachments/assets/e79c00eb-9ff0-42f4-9258-deed4ce7b2e6" />

## 当前版本

当前工作区目标版本：`v1.0.9`

以下版本说明基于 git 实际提交与当前工作区待发布改动整理。

### v1.0.9

基于 `v1.0.8..当前工作区` 的改动，`v1.0.9` 重点补齐编辑器目录能力与 AI 助手 MCP 管理链路：

- 编辑器预览区新增文档目录浮层，支持根据当前文档标题动态生成目录、点击目录项跳转到对应标题，并在预览重新渲染后同步刷新目录内容
- 调整编辑器目录交互与布局：目录按钮改为悬浮显示，不再挤压预览正文区域；展开面板宽度、留白和滚动表现也进一步向分享页样式靠齐
- AI 助手配置新增独立的 `MCP 配置` 入口，不再混在供应商配置弹窗内部；MCP 管理改为单独弹窗，并优化为左右分栏独立滚动、底部操作区固定可见
- 后端正式接入 MCP runtime，支持 `sse`、`streamable-http` 与 `stdio` 三种 transport，补齐配置持久化、连接测试、能力刷新、工具/资源/提示快照与聊天时的实际 MCP 注入
- MCP HTTP 配置支持多种认证模式与自定义 Headers，`stdio` 配置支持命令、参数、环境变量，并通过后端 allowlist 与开关控制是否展示和可用
- MCP 测试连接与刷新能力改为基于当前表单草稿执行，不必先保存才能验证配置；同时支持新增/复制时直接创建一条默认未启用的后端草稿记录，关闭弹窗后配置不会丢失
- 收紧 MCP transport 校验与诊断：切换到 `streamable-http` 时会拒绝继续使用 legacy `/sse` 端点；legacy SSE 心跳空消息会被忽略，不再刷无意义解析警告
- 后端配置与文档补充 `MCP_STDIO_ENABLED / MCP_STDIO_ALLOWED_COMMANDS`，用于控制 `stdio` 是否开启以及允许拉起的命令白名单

### v1.0.8

基于 `v1.0.7..当前工作区` 的改动，`v1.0.8` 重点优化智能体在文档树操作、对话收尾和编辑预览中的稳定性与可读性：

- 收紧智能体系统提示词约束：当任务涉及创建项目、创建/移动目录或文档、编辑正文与保存时，要求先读取项目列表、项目树、目标节点或编辑器状态，再执行写入与结构调整，减少凭空猜测路径和父节点导致的误操作
- 调整 `create_tree_node` 工具参数说明，明确“创建项目根目录节点”时不要传 `null`、空字符串、`0` 或其他占位值，避免模型把根目录错误当作一个实际父节点
- 调整 `move_tree_node` 工具参数说明，明确移动到项目根目录时必须显式传 `to_root=true`，且不要再混传 `target_parent_*` 字段，减少根目录移动语义歧义
- 优化前端创建节点工具实现，兼容把 `parent_id=0` 视作根目录别名的场景，并在存在真实父目录定位参数时再执行目录解析，降低根目录创建失败概率
- 优化前端移动节点工具实现，统一解析 `target_parent_* / parent_*` 参数，并兼容“根目录别名”输入，减少智能体在根目录移动和跨层级整理时的参数漂移问题
- 调整对话面板轮次摘要，去掉“仅打开了某个节点”的冗余提示，摘要更聚焦于创建、写入等真正影响结果的动作
- 优化多轮执行结束时的最终消息拼接逻辑：当本轮已经生成执行摘要或文档写入结果时，最终完成说明会在新的 assistant 消息中收尾，避免与过程消息互相覆盖或重复展示
- Markdown 编辑器预览改为使用独立的 `previewDraft` 节流同步，输入时不再每次都立即触发预览重渲染，在分栏/预览模式下能兼顾实时性与编辑流畅度
- 进一步收紧智能体面板宽度和高度布局，单栏聊天窗口在桌面端的阅读焦点更集中，也更贴近文档协作场景

### v1.0.7

基于 `v1.0.6..当前工作区` 的改动，`v1.0.7` 重点收敛智能体执行链路、模型接入配置和对话写作体验：

- 智能体主循环改为后端主导的 rig 风格 loop，后端统一维护多轮消息历史、工具结果回灌、continuation 与终止条件，不再依赖前端自行推进执行状态
- 新增前端工具回调桥，支持后端通过 `tool.request -> /api/agent/tool-callback` 请求前端执行工具，并将结果作为标准 tool result 回灌模型上下文
- 将 `[[ACTION:append]] / [[ACTION:replace]]` 文档动作协议纳入 loop 语义：ACTION 写入完成后会被视为一次 synthetic tool result，和普通工具调用一样参与后续续轮
- 补齐 ACTION 续写场景：当模型输出的 ACTION 块未完整闭合时，系统会自动发起 continuation 补齐剩余正文与 `[[/ACTION]]`，减少长文写作中途中断
- 修正多文档连续写入链路：同一轮中即使 ACTION 与保存/打开下一篇文档混合出现，也会正确记录写入完成结果并继续下一篇
- 收紧文档写作提示词：空文档首稿与文末续写统一优先 `[[ACTION:append]]...[[/ACTION]]`，整篇整体替换时使用 `[[ACTION:replace]]...[[/ACTION]]`
- 优化聊天面板消息展示：协议标记不再直接显示到聊天记录中；每轮只保留最新执行结果摘要；loop 完成时再单独输出最终总结块，避免覆盖过程消息
- 调整聊天窗口布局，去掉运行状态/工具状态侧栏，收紧为单栏聊天窗口，并补齐消息自动滚动到最新内容
- 新增 `AgentWorkbench`，支持按 provider kind 管理模型接入，区分 `openai / anthropic / gemini`，并为单模型配置 `modalities / thinking / tools / reasoning effort / additional params`
- 强化 OpenAI 兼容网关接入配置，补充 `provider_kind`、`model_configs`、模态与工具开关的持久化存储与数据库迁移
- 扩展聊天附件能力：除图片外，支持更多文本型附件，未知后缀但可解析为 UTF-8 文本的文件也可作为聊天文档附件提交给模型
- 强化附件与资料工具：支持按类型、名称、是否未引用筛选/批量删除附件，也支持通过已有图片附件设置头像
- 优化编辑器写入与局部改写链路，补充 Markdown 局部编辑辅助逻辑与更清晰的前端写入反馈

### v1.0.6

基于 `v1.0.5..当前工作区` 的改动，`v1.0.6` 重点重构智能体核心协议、执行状态与工具能力：

- 新增共享 agent 协议定义，统一前后端的正文动作、控制块、任务分析与页面路由枚举
- 重构智能体运行时，补充结构化任务分析、结构化计划、执行状态、上一轮完成记录与会话记忆
- 调整 AgentPanel 会话状态管理，新增 runtime plan、task analysis、tool events、artifacts 等一等状态
- 拆分“执行语义”和“传输协议”，支持在会话中显式切换 `auto / responses / chat` 三种交互模式
- 大幅减少依赖自然语言关键词的硬编码判断，改为以协议字段和结构化状态驱动多轮执行
- 优化多步计划执行、自动续轮、pending plan、last execution memory 的同步与恢复逻辑
- 强化正文协议约束，完善 `append / replace / rewrite_section / replace_block` 的共享协议定义、payload 规范与提示词说明
- 新增加强型局部写入失败处理，局部替换失败时直接报错并输出 console 调试信息，避免静默 no-op
- 优化编辑器快照与保存语义，区分实时编辑器、草稿缓存、已保存正文回退三种来源，并明确保存前状态
- 新增头像更新、附件筛选与批量删除的 function calling，支持按类型、名称和未引用状态管理附件
- 调整项目删除逻辑，删除项目时同步删除其下目录与文档，避免残留孤儿节点
- 更新 README 演示图，同步最新界面展示效果

### v1.0.5

基于 `v1.0.4..v1.0.5` 的提交，`v1.0.5` 重点修复多步任务确认与续执行状态漂移问题：

- 改进多步任务的确认链路，减少“已确认计划却再次进入确认态”的状态错乱
- 优化 pending plan 与续轮控制块的判断逻辑，降低模型换措辞后导致的前端失步风险
- 调整计划执行、继续处理与保存提示之间的状态同步，减少误续轮、漏续轮和错误保存提示
- 收紧多轮任务执行过程中的状态切换边界，为后续协议化重构打下基础

### v1.0.4

基于 `v1.0.3..HEAD` 的提交，`v1.0.4` 重点完善 AI 文档编写链路与编辑器草稿安全性：

- 文档生成链路升级为真实流式：后端直接透传模型增量内容，前端按流实时渲染，不再依赖“整段生成后再分片播放”
- 扩展文档协议动作，支持 `[[ACTION:append]]`、`[[ACTION:replace]]`、`[[ACTION:rewrite_section]]`、`[[ACTION:replace_block]]`，补齐局部重写与片段替换能力
- 明确协议边界：动作标记内内容写入 Markdown，标记外内容进入聊天面板；支持结束后继续输出保存状态与后续建议
- 补齐 function calling 能力：新增/强化 `read_editor_snapshot`、`save_current_document`、`update_tree_node_meta`、`update_project` 等工具
- 优化“未保存内容读取”策略：当文档存在未保存修改时，优先读取编辑器快照，再决定增量补写或重写
- 新增文档草稿缓存（本地草稿回填），修复切换文档后未保存内容丢失问题
- 调整提示词与编辑策略：先判定编辑意图和影响范围，再选择协议动作，减少无必要整篇重写

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

- 智能体与 AI 协作
- 页面内对话助手与独立 Agent Workbench
- provider kind / model config / modalities 管理
- 独立 MCP 管理与 `sse / streamable-http / stdio` 接入
- 前端工具调用桥接与后端主导多轮 loop
- `[[ACTION:append]] / [[ACTION:replace]]` 流式写文协议
- ACTION 未闭合时自动 continuation 补写
- 多文档连续写入与最终执行总结
- 聊天附件（图片 + 文本文档）与当前页面上下文感知

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
   ├─ pnpm-lock.yaml
   └─ src/
```

## 环境要求

- Rust stable（建议 1.93.1）
- Cargo
- Node.js 18+
- pnpm 9+（推荐）

## 本地开发

### 1) 启动后端

```bash
cd backend
cargo run
```

后端默认地址：`http://localhost:3000`

### 2) 启动前端

使用 pnpm：

```bash
cd frontend
pnpm install
pnpm dev
```

前端默认地址：`http://localhost:5173`

前端通过 Vite 代理把 `/api` 转发到 `http://localhost:3000`。

补充说明：

- 前端构建前会自动准备离线资源，把 `vditor/dist` 复制到 `frontend/public/vendor/vditor/dist`
- `Vditor` 编辑器和预览渲染都走本地 `/vendor/vditor` 资源，不依赖外网 CDN

## 后端配置

后端启动时会优先读取可执行文件同目录下的 `config.toml`，同时环境变量会覆盖同名配置。

### MCP stdio 开启方式

MCP 的 `stdio` 传输默认是关闭的。只有后端明确开启后，前端的 MCP 配置弹窗里才会显示 `stdio` 选项。

可用的配置项在 [backend/config.toml](/Volumes/Documents/projects/rust/markflow/backend/config.toml)：

```toml
mcp_stdio_enabled = false
mcp_stdio_allowed_commands = ["npx", "node", "uvx"]
```

含义是：

- `mcp_stdio_enabled`
  控制是否启用 MCP stdio 能力。设为 `true` 后，前端才会显示 `stdio` 配置。
- `mcp_stdio_allowed_commands`
  stdio 命令白名单。只有这里列出的命令才允许被 MCP 启动。

对应环境变量是：

- `MCP_STDIO_ENABLED`
- `MCP_STDIO_ALLOWED_COMMANDS`

示例：

```bash
export MCP_STDIO_ENABLED=true
export MCP_STDIO_ALLOWED_COMMANDS=npx,node,uvx
cd backend
cargo run
```

说明：

- `MCP_STDIO_ENABLED` 支持常见布尔值：`true/false`、`1/0`、`yes/no`、`on/off`
- `MCP_STDIO_ALLOWED_COMMANDS` 使用逗号分隔，例如 `npx,node,uvx`
- 环境变量优先级高于 `config.toml`
- 如果 `mcp_stdio_enabled=false` 或未设置，前端不会显示 `stdio`
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
