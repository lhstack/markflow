# MarkFlow

MarkFlow 是一个基于 `Rust + Vue 3` 的轻量文档系统，核心结构为：`用户 -> 项目 -> 文档树`。

Docker Hub 镜像仓库：

- `lhstack/markflow`

项目仓库：

- [https://github.com/lhstack/markflow](https://github.com/lhstack/markflow)

## 功能概览

- 项目、目录、文档树管理
- Markdown 编辑与预览
- 文档与目录分享
- 登录验证码与 2FA
- 附件上传与管理
- 超级管理员、系统配置、用户管理

## 支持架构

- `linux/amd64`
- `linux/arm64`

## 快速开始

拉取镜像：

```bash
docker pull lhstack/markflow:latest
```

说明：

- 镜像仅在发布 `v*` 版本标签时构建并推送
- `latest` 始终指向最近一次正式版本发布
- 同时也会发布对应版本标签，例如 `lhstack/markflow:v1.0.6`

运行容器：

```bash
docker run -d \
  --name markflow \
  --restart unless-stopped \
  -p 3000:3000 \
  -v $(pwd)/data:/app/data \
  -v $(pwd)/uploads:/app/uploads \
  -v $(pwd)/logs:/app/logs \
  -e JWT_SECRET=replace_with_your_secret \
  -e SHARE_PASSWORD_SECRET=replace_with_your_share_password_secret \
  -e REGISTRATION_ENABLED=true \
  -e UPLOAD_MAX_MB=20 \
  lhstack/markflow:latest
```

访问地址：

- `http://<你的服务器IP>:3000`

## Docker Compose

```yaml
services:
  markflow:
    image: lhstack/markflow:latest
    container_name: markflow
    restart: unless-stopped
    ports:
      - "3000:3000"
    environment:
      PORT: "3000"
      DATABASE_URL: "sqlite:data/markflow.db"
      JWT_SECRET: "change_me_to_a_long_random_string_in_production"
      SHARE_PASSWORD_SECRET: "change_me_to_a_long_random_string_for_share_password_encryption"
      RUST_LOG: "markflow=info,tower_http=warn"
      UPLOAD_DIR: "uploads"
      REGISTRATION_ENABLED: "true"
      UPLOAD_MAX_MB: "20"
      LOG_TO_FILE: "true"
      LOG_DIR: "logs"
      LOG_FILE_NAME: "markflow.log"
      LOG_ROTATE_SIZE_MB: "50"
      LOG_ROTATE_DAYS: "1"
      LOG_KEEP_DAYS: "14"
    volumes:
      - ./data:/app/data
      - ./uploads:/app/uploads
      - ./logs:/app/logs
```

启动：

```bash
docker compose up -d
```

## 数据卷说明

- `/app/data`：SQLite 数据目录
- `/app/uploads`：头像、项目背景图、附件等上传目录
- `/app/logs`：应用日志目录

建议持久化挂载这三个目录。

## 常用环境变量

| 环境变量 | 默认值 | 说明 |
| --- | --- | --- |
| `PORT` | `3000` | HTTP 监听端口 |
| `DATABASE_URL` | `sqlite:data/markflow.db` | 数据库连接串 |
| `JWT_SECRET` | `markflow_dev_secret_change_in_production` | JWT 密钥，生产必须替换 |
| `SHARE_PASSWORD_SECRET` | 同 `JWT_SECRET` 回退值 | 分享密码加密密钥，生产建议单独配置 |
| `RUST_LOG` | `markflow=info,tower_http=warn` | 日志级别 |
| `UPLOAD_DIR` | `uploads` | 上传文件目录 |
| `REGISTRATION_ENABLED` | `true` | 是否允许新用户注册 |
| `UPLOAD_MAX_MB` | `20` | 单文件上传大小限制（MB） |
| `LOG_TO_FILE` | `false` | 是否写入文件日志 |
| `LOG_DIR` | `logs` | 日志目录 |
| `LOG_FILE_NAME` | `markflow.log` | 当前日志文件名 |
| `LOG_ROTATE_SIZE_MB` | `50` | 日志滚动大小（MB） |
| `LOG_ROTATE_DAYS` | `1` | 日志滚动天数 |
| `LOG_KEEP_DAYS` | `14` | 历史日志保留天数 |

说明：

- 系统配置首次启动时会读取环境变量或 `config.toml`
- 一旦系统配置已写入数据库，后续运行会优先读取数据库中的配置
- 分享密码不会以明文直接落库，而是使用 `SHARE_PASSWORD_SECRET` 进行服务端加密后保存
- 如果未单独配置 `SHARE_PASSWORD_SECRET`，系统会回退使用 `JWT_SECRET`，但生产环境不建议这样做

## 首次启动

从 `1.0.1` 开始，系统启动时会自动检查超级管理员账号：

- 用户名固定为：`admin`
- 如果数据库中不存在该超级管理员，会自动初始化
- 初始密码随机生成，并只会在首次初始化时输出到日志

查看初始密码：

```bash
docker logs markflow | grep "Initialized super admin account"
```

如果环境里没有 `grep`，也可以直接查看最近日志：

```bash
docker logs --tail 200 markflow
```

建议首次登录后立即修改管理员密码。

## 1.0.6 版本更新

以下内容基于 `v1.0.5..当前工作区` 的实际改动整理：

- 新增共享 agent 协议定义，统一前后端的正文动作、控制块、任务分析与路由枚举
- 重构智能体运行时，补充结构化任务分析、结构化计划、执行状态、上一轮完成记录与会话记忆
- 支持在会话中显式切换 `auto / responses / chat` 三种传输模式，便于兼容不同模型与网关
- 优化多步计划执行、自动续轮、pending plan、last execution memory 的同步与恢复逻辑
- 强化正文协议约束，完善 `append / replace / rewrite_section / replace_block` 的 payload 规范与提示词规则
- 新增加强型局部写入失败处理，局部替换失败时直接报错并输出前端调试日志
- 新增头像更新、附件筛选与批量删除的 function calling，支持按类型、名称和未引用状态管理附件
- 调整项目删除逻辑，删除项目时同步删除其下目录与文档

## 1.0.5 版本更新

以下内容基于 `v1.0.4..v1.0.5` 的实际 git 提交整理：

- 改进多步任务确认链路，减少重复确认与状态漂移
- 优化 pending plan 与续轮状态判断，降低模型换措辞导致的前端失步风险
- 调整计划执行、继续处理与保存提示之间的状态同步，减少误续轮、漏续轮和错误保存提示
- 为后续协议化重构预留更清晰的状态边界

## 1.0.4 版本更新

以下内容基于 `v1.0.3..HEAD` 的实际 git 提交整理：

- AI 文档写作链路升级为真实流式透传，前端可边生成边渲染
- 新增文档协议动作 `append / replace / rewrite_section / replace_block`，支持局部重写与片段替换
- 协议内容与聊天内容分流：标记内写入编辑器，标记外在聊天面板展示
- 补齐函数调用能力：`read_editor_snapshot`、`save_current_document`、`update_tree_node_meta`、`update_project`
- 新增本地文档草稿缓存并自动恢复，避免切换文档导致未保存内容丢失
- 优化提示词与编辑决策逻辑，优先结合上下文判断是增量补充还是整体改写

## 1.0.3 版本更新

以下内容基于 `v1.0.2..HEAD` 的实际 git 提交整理：

- 新增 AI 助手后端接口与前端对话面板，支持在页面内直接进行智能问答和文档协作
- 补全 function calling，覆盖页面内容感知、页面跳转、项目管理、文档树查询、文档读写、浏览器与编辑器运行时
- 新增文档/目录移动工具，支持将节点移动到指定目录或项目根目录
- 优化工具 schema、参数描述与工具结果续轮逻辑，提升兼容 OpenAI 风格网关时的稳定性
- 为 `Responses` 链路补充工具调用回退处理，并增强当前会话上下文记忆
- 增加会话历史压缩能力，长对话场景下保留关键上下文并降低 token 消耗
- 支持 AI 以打字机效果写入 Markdown 文档，默认只写入未保存草稿，避免直接落盘
- 支持中止生成与快捷发送，交互方式调整为 `Ctrl+Enter / Cmd+Enter` 发送、`Enter` 换行
- 优化项目概览页空态与删除逻辑，允许删除最后一个项目并简化重复入口

## 1.0.2 版本更新

- 新增系统配置持久化能力
- 新增超级管理员初始化
- 新增系统配置与用户管理界面
- 支持注册开关和上传大小限制
- 新增分享密码服务端加密存储与可恢复复制
- 修复粘贴上传重复触发问题
- 登录、注册、2FA 错误提示改为中文
- 分享页右侧目录改为浮动定位，不再影响正文区域宽度

## 常用运维命令

查看日志：

```bash
docker logs -f markflow
```

重启容器：

```bash
docker restart markflow
```

更新镜像：

```bash
docker pull lhstack/markflow:latest
docker compose up -d
```
