# MarkFlow Docker Hub 使用与安装手册

本文档用于发布后给使用者参考，镜像仓库为 `lhstack/markflow`。

项目仓库地址：

- https://github.com/lhstack/markflow/

## 1. 版本与兼容性

- 当前版本：`1.0.1`
- 应用 Rust 版本：`1.93.1`（镜像构建阶段使用）
- 推荐 Docker Engine：`24.0+`
- 推荐 Docker Compose Plugin：`2.20+`

版本检查命令：

```bash
docker version
docker compose version
```

## 2. 安装 Docker 与 Compose

### 2.1 Ubuntu / Debian（命令行安装）

```bash
sudo apt-get update
sudo apt-get install -y ca-certificates curl gnupg
sudo install -m 0755 -d /etc/apt/keyrings
curl -fsSL https://download.docker.com/linux/ubuntu/gpg | sudo gpg --dearmor -o /etc/apt/keyrings/docker.gpg
sudo chmod a+r /etc/apt/keyrings/docker.gpg

echo \
  "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/docker.gpg] https://download.docker.com/linux/ubuntu \
  $(. /etc/os-release && echo \"$VERSION_CODENAME\") stable" | \
  sudo tee /etc/apt/sources.list.d/docker.list > /dev/null

sudo apt-get update
sudo apt-get install -y docker-ce docker-ce-cli containerd.io docker-buildx-plugin docker-compose-plugin
```

将当前用户加入 docker 组（可选，避免每次 sudo）：

```bash
sudo usermod -aG docker $USER
newgrp docker
```

### 2.2 macOS / Windows

安装 Docker Desktop（内置 `docker compose`），安装后执行：

```bash
docker version
docker compose version
```

## 3. 拉取镜像并快速运行

```bash
docker pull lhstack/markflow:latest
```

### 3.1 docker run 方式

```bash
docker run -d \
  --name markflow \
  --restart unless-stopped \
  -p 3000:3000 \
  --cpus=2 \
  --memory=256m \
  --log-driver=json-file \
  --log-opt max-size=16k \
  --log-opt max-file=2 \
  -v $(pwd)/data:/app/data \
  -v $(pwd)/uploads:/app/uploads \
  -v $(pwd)/logs:/app/logs \
  -e JWT_SECRET=replace_with_your_secret \
  -e REGISTRATION_ENABLED=true \
  -e UPLOAD_MAX_MB=20 \
  lhstack/markflow:latest
```

访问地址：`http://<你的服务器IP>:3000`

### 3.2 docker compose 方式

仓库内已提供 `docker-compose.yml`，直接运行：

```bash
docker compose up -d
```

## 4. 配置优先级

应用配置读取顺序：

1. 环境变量
2. `config.toml`
3. 程序默认值

容器内配置文件路径：`/app/config.toml`（与可执行文件同目录）。

## 5. 环境变量与配置参数（完整）

| 配置项 (`config.toml`) | 环境变量 | 默认值 | 说明 |
| --- | --- | --- | --- |
| `port` | `PORT` | `3000` | HTTP 监听端口 |
| `database_url` | `DATABASE_URL` | `sqlite:markflow.db` | 数据库连接串，容器建议 `sqlite:data/markflow.db` |
| `jwt_secret` | `JWT_SECRET` | `markflow_dev_secret_change_in_production` | JWT 密钥，生产必须替换 |
| `rust_log` | `RUST_LOG` | `markflow=info,tower_http=warn` | 日志级别过滤规则 |
| `upload_dir` | `UPLOAD_DIR` | `uploads` | 上传文件根目录，容器建议挂载到 `/app/uploads` |
| `registration_enabled` | `REGISTRATION_ENABLED` | `true` | 是否允许新用户注册；数据库中已有系统配置时以数据库为准 |
| `upload_max_mb` | `UPLOAD_MAX_MB` | `20` | 单文件上传大小限制（MB）；数据库中已有系统配置时以数据库为准 |
| `log_to_file` | `LOG_TO_FILE` | `false` | 是否写文件日志（同时保留控制台日志） |
| `log_dir` | `LOG_DIR` | `logs` | 日志目录 |
| `log_file_name` | `LOG_FILE_NAME` | `markflow.log` | 当前活跃日志文件名 |
| `log_rotate_size_mb` | `LOG_ROTATE_SIZE_MB` | `50` | 按大小滚动阈值（MB） |
| `log_rotate_days` | `LOG_ROTATE_DAYS` | `1` | 按时间滚动阈值（天） |
| `log_keep_days` | `LOG_KEEP_DAYS` | `14` | 历史日志保留天数（`<=0` 表示不清理） |

## 6. `config.toml` 完整示例

```toml
port = "3000"
database_url = "sqlite:data/markflow.db"
jwt_secret = "replace_with_a_long_random_string"
rust_log = "markflow=info,tower_http=warn"
upload_dir = "uploads"
registration_enabled = true
upload_max_mb = 20

log_to_file = true
log_dir = "logs"
log_file_name = "markflow.log"
log_rotate_size_mb = 50
log_rotate_days = 1
log_keep_days = 14
```

## 7. 数据与日志目录说明

- `/app/data`：SQLite 数据目录（建议挂载持久化卷）
- `/app/uploads`：上传文件目录（头像、项目背景图、附件等，建议挂载持久化卷）
- `/app/logs`：应用文件日志目录（建议挂载持久化卷）

建议备份：

- `data/markflow.db`
- `uploads/`
- `logs/`（按需保留）

## 8. 首次启动与超级管理员

从 `1.0.1` 开始，应用启动时会自动检查超级管理员账号：

- 如果数据库中不存在超级管理员，则自动初始化一个固定用户名的账号：`admin`
- 初始密码为随机生成，只会在首次初始化时打印到容器日志
- 该密码不会再次显示，建议首次登录后立即修改

查看首次启动日志中的管理员密码：

```bash
docker logs markflow | grep "Initialized super admin account"
```

如果没有 grep，也可以直接查看最近日志：

```bash
docker logs --tail 200 markflow
```

说明：

- 超级管理员可以在“系统配置”中开启或关闭用户注册
- 超级管理员可以在“用户管理”中管理普通用户状态、密码与 2FA
- 一旦系统配置已写入数据库，后续会优先读取数据库，不再以环境变量覆盖运行中的配置值

## 9. 1.0.1 功能补充

`1.0.1` 相比之前版本新增或完善了以下能力：

- 系统配置持久化到数据库，支持后台动态切换注册开关和上传大小限制
- 超级管理员启动自动初始化
- 用户管理后台：新增用户、删除用户、启用/停用、重置密码、开关 2FA、导出用户数据
- 上传链路统一，支持按钮上传、拖拽上传、粘贴上传
- 修复粘贴上传重复触发导致附件重复创建的问题
- 分享页增强文档目录定位体验，右侧浮动目录不会再影响正文区域宽度
- 登录、注册、2FA 页面错误提示改为中文

## 10. 常用运维命令

查看容器日志：

```bash
docker logs -f markflow
```

重启容器：

```bash
docker restart markflow
```

更新镜像并重建（compose）：

```bash
docker compose pull
docker compose up -d
```

## 11. 发布者（维护者）推送示例

```bash
docker build -t lhstack/markflow:latest .
docker push lhstack/markflow:latest
```

版本标签（例如 `v1.2.3`）：

```bash
docker tag lhstack/markflow:latest lhstack/markflow:v1.2.3
docker push lhstack/markflow:v1.2.3
```
