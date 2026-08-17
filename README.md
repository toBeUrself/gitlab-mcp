# GitLab MCP

面向公司内部自建 GitLab 的本机 MCP Server。服务通过标准输入输出（stdio）与 MCP 客户端通信，并通过 GitLab REST API v4 完成实际操作。

当前提供 28 个工具，覆盖项目、Merge Request、Issue、代码文件、分支/提交比较、Tag、Pipeline 和 Job 日志等高频研发场景。写操作默认关闭，不提供合并 MR、删除资源、修改仓库文件等高风险能力。

方案与安全边界详见 [DESIGN.md](./DESIGN.md)。

## 快速开始

### 1. 准备访问令牌

推荐为 MCP 单独创建最小权限的 Project Access Token 或 Personal Access Token：

- 只使用查询工具：授予 `read_api` scope；
- 需要创建分支/Tag、Issue、MR、修改 MR 或评论：授予 `api` scope，并通过 `GITLAB_ALLOW_WRITE=true` 显式开启写工具；
- 不要把 Token 写进仓库或通过命令行参数传递。

### 2. 构建

```bash
cd /path/to/gitlab-mcp
sh scripts/build-release.sh
```

构建产物位于：

```text
dist/gitlab-mcp
```

匿名化构建脚本会在系统临时目录中编译，重映射 Rust 和 Cargo 的本机源码路径，只把经过隐私检查的最终二进制复制到 `dist/`。临时构建目录会自动删除，避免在项目内留下包含用户名或个人目录结构的中间产物。分发或截图前应使用该脚本，不要直接分发普通 `cargo build` 产生的二进制或 `target/` 目录。

将二进制安装到 MCP 客户端 `PATH` 中的目录后，配置里只需使用命令名。例如：

```bash
install -m 755 dist/gitlab-mcp /path/in/PATH/gitlab-mcp
```

### 3. 配置 MCP 客户端

把下面配置加入支持 stdio MCP 的客户端。不同客户端的外层配置文件位置可能不同，但 server 定义中的 `command` 和 `env` 内容相同。

只读模式（推荐先使用）：

```json
{
  "mcpServers": {
    "company-gitlab": {
      "command": "gitlab-mcp",
      "env": {
        "GITLAB_URL": "https://gitlab.example.internal",
        "GITLAB_TOKEN": "replace-with-your-token",
        "GITLAB_TOKEN_TYPE": "private",
        "GITLAB_ALLOW_WRITE": "false"
      }
    }
  }
}
```

允许上述受限写操作：

```json
{
  "mcpServers": {
    "company-gitlab": {
      "command": "gitlab-mcp",
      "env": {
        "GITLAB_URL": "https://gitlab.example.internal",
        "GITLAB_TOKEN": "replace-with-an-api-scope-token",
        "GITLAB_TOKEN_TYPE": "private",
        "GITLAB_ALLOW_WRITE": "true"
      }
    }
  }
}
```

保存配置并重启 MCP 客户端。客户端成功连接后应能发现下文列出的 28 个工具。

### 4. 开始使用

可以直接用自然语言让客户端选择工具，例如：

```text
列出我最近活跃的 GitLab 项目。
查看 team/payment 项目当前打开的 MR。
比较 team/payment 的 qa2 和 main 是否完全同步。
确认 Commit abc123 是否已进入 team/payment 的 release 分支。
读取 team/payment 的 main 分支上 src/main.rs。
从 team/payment 的 main 创建 feature/retry-payment 分支。
检查 team/payment 最近失败的 Pipeline、对应 Job 和日志。
在 team/payment 创建一个标题为“修复支付回调超时”的 Issue。
给 team/payment 的 MR !128 评论“已完成本地验证”。
```

涉及写操作时，建议在提示中同时给出项目、标题/正文等完整信息，并在执行前检查客户端展示的工具参数。

## 环境变量

| 变量 | 必填 | 默认值 | 说明 |
| --- | --- | --- | --- |
| `GITLAB_URL` | 是 | 无 | GitLab 实例根地址，例如 `https://gitlab.example.internal`；也可直接填写以 `/api/v4` 结尾的地址 |
| `GITLAB_TOKEN` | 是 | 无 | GitLab 访问令牌，仅作为 HTTP Header 发送 |
| `GITLAB_TOKEN_TYPE` | 否 | `private` | `private`/`pat` 使用 `PRIVATE-TOKEN`，`oauth`/`bearer` 使用 Bearer Token，`job` 使用 `JOB-TOKEN` |
| `GITLAB_ALLOW_WRITE` | 否 | `false` | `1`、`true` 或 `yes` 时允许七个受限写工具，其余值均视为关闭 |
| `GITLAB_INSECURE` | 否 | `false` | `true` 时接受无效 TLS 证书，仅用于确有需要且风险可控的内网环境 |
| `GITLAB_MAX_RESPONSE_BYTES` | 否 | `1048576` | 单次 GitLab 响应最大字节数，必须大于 0；用于限制 MCP 上下文消耗 |

环境变量修改后需要重启 MCP Server。stdio 模式下通常意味着重启承载它的 MCP 客户端。

## 参数约定

- `project`：可传 GitLab 数字项目 ID，也可传完整命名空间路径，例如 `platform/order-service`。路径会由服务安全编码。
- `iid`：Issue 或 MR 在所属项目内的编号，例如页面显示的 `#25` 或 `!128` 中的数字；不是 GitLab 数据库全局 ID。
- `git_ref`：分支、Tag 或 Commit SHA。
- `page`：页码，从 1 开始，默认 1。
- `per_page`：每页数量，范围 1～100，默认 30。
- 所有工具返回 GitLab API 的 JSON；`get_repository_file` 的 `content` 字段为 Base64 编码。
- GitLab 权限仍由所配置 Token 决定。MCP 暴露了某个工具，不代表该 Token 一定有权执行。

## 工具详情

### 用户与项目

#### `get_current_user`

返回当前 Token 对应的 GitLab 用户，用于验证地址、Token 和鉴权方式是否正确。

- 参数：无。
- GitLab API：`GET /user`。
- 典型用途：完成配置后首先调用；确认当前操作身份。

#### `list_projects`

列出当前用户可见的项目，结果固定按最近活动时间倒序排列。

| 参数 | 必填 | 类型 | 默认值 | 说明 |
| --- | --- | --- | --- | --- |
| `search` | 否 | string | 无 | 按项目名称或路径搜索 |
| `membership` | 否 | boolean | 无 | `true` 时只返回当前用户所属项目 |
| `page` | 否 | integer | `1` | 页码 |
| `per_page` | 否 | integer | `30` | 每页数量，1～100 |

- GitLab API：`GET /projects`。
- 示例提示：`搜索名称包含 quotation 的项目，只看我加入的项目。`

#### `get_project`

读取单个项目的详细信息，包括默认分支、描述、可见性和 Web 地址等 GitLab 返回字段。

| 参数 | 必填 | 类型 | 说明 |
| --- | --- | --- | --- |
| `project` | 是 | string | 数字项目 ID 或完整命名空间路径 |

- GitLab API：`GET /projects/:id`。
- 示例提示：`查看 platform/order-service 的项目详情。`

#### `list_group_projects`

列出指定 GitLab 群组下的项目，固定按最近活动时间倒序排列。

| 参数 | 必填 | 类型 | 默认值 | 说明 |
| --- | --- | --- | --- | --- |
| `group` | 是 | string | 无 | 数字群组 ID 或完整群组路径，例如 `platform/backend` |
| `search` | 否 | string | 无 | 按项目名称或路径搜索 |
| `include_subgroups` | 否 | boolean | `false` | 是否包含子群组项目 |
| `with_shared` | 否 | boolean | `true` | 是否包含共享给该群组的项目 |
| `archived` | 否 | boolean | 无 | 按归档状态过滤 |
| `page` | 否 | integer | `1` | 页码 |
| `per_page` | 否 | integer | `30` | 每页数量，1～100 |

- GitLab API：`GET /groups/:id/projects`。
- 示例提示：`列出 platform 群组及其子群组中最近活跃的项目。`

### Merge Request

#### `list_merge_requests`

列出指定项目的 MR，查询范围固定为当前 Token 可见的全部 MR。

| 参数 | 必填 | 类型 | 默认值 | 说明 |
| --- | --- | --- | --- | --- |
| `project` | 是 | string | 无 | 项目 ID 或完整路径 |
| `state` | 否 | string | 无 | `opened`、`closed`、`merged` 或 `all` |
| `page` | 否 | integer | `1` | 页码 |
| `per_page` | 否 | integer | `30` | 每页数量，1～100 |

- GitLab API：`GET /projects/:id/merge_requests`。
- 示例提示：`列出 platform/order-service 当前打开的 MR。`

#### `get_merge_request`

获取一个 MR 的标题、描述、源/目标分支、作者、状态、Pipeline 和合并状态等信息。

| 参数 | 必填 | 类型 | 说明 |
| --- | --- | --- | --- |
| `project` | 是 | string | 项目 ID 或完整路径 |
| `iid` | 是 | integer | 项目内 MR 编号，例如 `!128` 传 `128` |

- GitLab API：`GET /projects/:id/merge_requests/:iid`。
- 示例提示：`查看 platform/order-service 的 MR !128。`

#### `list_merge_request_diffs`

列出一个 MR 的文件级 diff。该工具固定请求最多 100 条 diff；大 MR 仍可能受到 GitLab 实例限制和响应大小限制。

| 参数 | 必填 | 类型 | 说明 |
| --- | --- | --- | --- |
| `project` | 是 | string | 项目 ID 或完整路径 |
| `iid` | 是 | integer | 项目内 MR 编号 |

- GitLab API：`GET /projects/:id/merge_requests/:iid/diffs`。
- 示例提示：`检查 platform/order-service 的 MR !128 修改了哪些文件。`

#### `review_merge_request_context`

一次性整理评审一个 MR 所需的上下文。MR 基本信息为必需数据；diff、Commit、Discussion、关联 Issue、Pipeline 和审批状态并发获取，避免客户端连续调用多个基础工具。

| 参数 | 必填 | 类型 | 默认值 | 说明 |
| --- | --- | --- | --- | --- |
| `project` | 是 | string | 无 | 项目 ID 或完整路径 |
| `iid` | 是 | integer | 无 | 项目内 MR 编号 |
| `include_diffs` | 否 | boolean | `true` | 是否包含文件 diff；大型 MR 可关闭 |
| `include_commits` | 否 | boolean | `true` | 是否包含 Commit |
| `include_discussions` | 否 | boolean | `true` | 是否包含普通讨论和 diff 讨论 |
| `include_related_issues` | 否 | boolean | `true` | 是否包含从 MR 内容识别到的关联 Issue |
| `include_pipelines` | 否 | boolean | `true` | 是否包含 MR Pipeline |
| `include_approvals` | 否 | boolean | `true` | 是否包含审批状态 |
| `per_page` | 否 | integer | `30` | 每个集合最多请求的条数，1～100 |

返回对象包括：

- `merge_request`：MR 元数据，包括分支、作者、状态、合并状态等；
- 被启用且获取成功的 `diffs`、`commits`、`discussions`、`related_issues`、`pipelines`、`approvals`；
- `summary`：本次返回的文件、Commit、Discussion、未解决 Discussion、关联 Issue 和 Pipeline 数量，以及审批是否完成；
- `partial`：是否存在可选接口失败；
- `warnings`：可选接口失败原因，例如当前 GitLab 版本不支持 approvals，或 Token 权限不足。

MR 主接口失败时整个工具失败。可选接口失败时仍返回已有评审上下文，并通过 `partial=true` 提醒调用方。聚合结果仍受 `GITLAB_MAX_RESPONSE_BYTES` 限制；超限时可关闭不需要的 `include_*`、降低 `per_page`，或谨慎提高响应上限。

- GitLab API：聚合 MR、diff、Commit、Discussion、关联 Issue、MR Pipeline 和 approvals 等只读端点。
- 示例提示：`整理 platform/order-service 的 MR !128 的完整评审上下文，并重点检查未解决讨论。`
- 精简示例：`读取 MR !128 的评审上下文，但不要返回 diff 和历史 Pipeline。`

#### `create_merge_request`（写操作）

创建 MR。需要 `GITLAB_ALLOW_WRITE=true` 和具备写权限的 Token。

| 参数 | 必填 | 类型 | 默认值 | 说明 |
| --- | --- | --- | --- | --- |
| `project` | 是 | string | 无 | 项目 ID 或完整路径 |
| `source_branch` | 是 | string | 无 | 源分支 |
| `target_branch` | 是 | string | 无 | 目标分支 |
| `title` | 是 | string | 无 | MR 标题 |
| `description` | 否 | string | 无 | MR 描述 |
| `draft` | 否 | boolean | `false` | `true` 时自动给标题增加 `Draft:` 前缀 |
| `remove_source_branch` | 否 | boolean | GitLab 项目设置 | 合并后是否删除源分支 |

- GitLab API：`POST /projects/:id/merge_requests`。
- 示例提示：`从 feature/retry 合并到 main，创建一个草稿 MR，标题为“增加失败重试”。`

#### `update_merge_request`（写操作）

修改 MR 的标题、描述、Draft 状态、reviewer 或 assignee。需要 `GITLAB_ALLOW_WRITE=true`；至少要提供一个待修改字段。

| 参数 | 必填 | 类型 | 说明 |
| --- | --- | --- | --- |
| `project` | 是 | string | 项目 ID 或完整路径 |
| `iid` | 是 | integer | 项目内 MR 编号 |
| `title` | 否 | string | 新标题 |
| `description` | 否 | string | 新描述；传空字符串可清空 |
| `draft` | 否 | boolean | `true` 转为 Draft，`false` 取消 Draft |
| `reviewer_ids` | 否 | integer[] | GitLab 用户 ID 数组；空数组清空 reviewer |
| `assignee_ids` | 否 | integer[] | GitLab 用户 ID 数组；空数组清空 assignee |

GitLab REST 更新接口没有独立的 Draft 字段，工具通过标题前缀兼容处理。仅修改 `draft` 时会先读取当前标题，再增加或移除 `Draft:`/`WIP:` 等前缀。

- GitLab API：`PUT /projects/:id/merge_requests/:iid`。
- 示例提示：`把 platform/order-service 的 MR !128 转为非 Draft，并设置 reviewer ID 为 42 和 57。`

#### `add_merge_request_note`（写操作）

给 MR 添加普通评论，不支持行级 diff 评论。

| 参数 | 必填 | 类型 | 说明 |
| --- | --- | --- | --- |
| `project` | 是 | string | 项目 ID 或完整路径 |
| `iid` | 是 | integer | 项目内 MR 编号 |
| `body` | 是 | string | 评论正文 |

- GitLab API：`POST /projects/:id/merge_requests/:iid/notes`。
- 示例提示：`给 platform/order-service 的 MR !128 评论“CI 已通过，可以复查”。`

### Issue

#### `list_issues`

列出指定项目当前 Token 可见的 Issue。

| 参数 | 必填 | 类型 | 默认值 | 说明 |
| --- | --- | --- | --- | --- |
| `project` | 是 | string | 无 | 项目 ID 或完整路径 |
| `state` | 否 | string | 无 | `opened`、`closed` 或 `all` |
| `page` | 否 | integer | `1` | 页码 |
| `per_page` | 否 | integer | `30` | 每页数量，1～100 |

- GitLab API：`GET /projects/:id/issues`。
- 示例提示：`列出 platform/order-service 未关闭的 Issue。`

#### `get_issue`

获取一个 Issue 的标题、描述、标签、负责人、状态等信息。

| 参数 | 必填 | 类型 | 说明 |
| --- | --- | --- | --- |
| `project` | 是 | string | 项目 ID 或完整路径 |
| `iid` | 是 | integer | 项目内 Issue 编号，例如 `#25` 传 `25` |

- GitLab API：`GET /projects/:id/issues/:iid`。
- 示例提示：`查看 platform/order-service 的 Issue #25。`

#### `create_issue`（写操作）

创建 Issue。需要 `GITLAB_ALLOW_WRITE=true` 和具备写权限的 Token。

| 参数 | 必填 | 类型 | 说明 |
| --- | --- | --- | --- |
| `project` | 是 | string | 项目 ID 或完整路径 |
| `title` | 是 | string | Issue 标题 |
| `description` | 否 | string | Issue 描述 |
| `labels` | 否 | string | 逗号分隔的标签，例如 `bug,backend` |
| `assignee_ids` | 否 | integer[] | GitLab 用户 ID 数组，不是用户名 |

- GitLab API：`POST /projects/:id/issues`。
- 示例提示：`在 platform/order-service 创建一个 bug Issue，指派给用户 ID 42。`

#### `add_issue_note`（写操作）

给 Issue 添加普通评论。

| 参数 | 必填 | 类型 | 说明 |
| --- | --- | --- | --- |
| `project` | 是 | string | 项目 ID 或完整路径 |
| `iid` | 是 | integer | 项目内 Issue 编号 |
| `body` | 是 | string | 评论正文 |

- GitLab API：`POST /projects/:id/issues/:iid/notes`。
- 示例提示：`给 platform/order-service 的 Issue #25 评论“问题已在 qa2 验证”。`

### 仓库内容

#### `list_branches`

列出项目分支，可使用文本或 RE2 正则过滤，两种过滤方式不能同时使用。

| 参数 | 必填 | 类型 | 默认值 | 说明 |
| --- | --- | --- | --- | --- |
| `project` | 是 | string | 无 | 项目 ID 或完整路径 |
| `search` | 否 | string | 无 | 分支名文本，支持 `^prefix` 和 `suffix$` |
| `regex` | 否 | string | 无 | RE2 正则表达式 |
| `page` | 否 | integer | `1` | 页码 |
| `per_page` | 否 | integer | `30` | 每页数量，1～100 |

- GitLab API：`GET /projects/:id/repository/branches`。
- 示例提示：`列出 platform/order-service 中以 release/ 开头的分支。`

#### `get_branch`

获取单个分支的最新 Commit、保护状态和 Web 地址等信息。

| 参数 | 必填 | 类型 | 说明 |
| --- | --- | --- | --- |
| `project` | 是 | string | 项目 ID 或完整路径 |
| `branch` | 是 | string | 分支名，包含 `/` 时由服务自动编码 |

- GitLab API：`GET /projects/:id/repository/branches/:branch`。
- 示例提示：`查看 platform/order-service 的 qa2 分支当前指向哪个 Commit。`

#### `compare_refs`

比较两个分支、Tag 或 Commit，返回 Commit 和文件 diff。

| 参数 | 必填 | 类型 | 默认值 | 说明 |
| --- | --- | --- | --- | --- |
| `project` | 是 | string | 无 | 项目 ID 或完整路径 |
| `from` | 是 | string | 无 | 源分支、Tag 或 Commit SHA |
| `to` | 是 | string | 无 | 目标分支、Tag 或 Commit SHA |
| `straight` | 否 | boolean | `false` | `false` 按 merge-base 比较 `from...to`；`true` 直接比较 `from..to` |

- GitLab API：`GET /projects/:id/repository/compare`。
- 示例提示：`直接比较 platform/order-service 的 qa2 和 main，确认环境分支是否同步。`

#### `list_commits`

列出默认分支或指定 ref 的 Commit，可按路径和时间范围筛选。

| 参数 | 必填 | 类型 | 默认值 | 说明 |
| --- | --- | --- | --- | --- |
| `project` | 是 | string | 无 | 项目 ID 或完整路径 |
| `git_ref` | 否 | string | 默认分支 | 分支、Tag、SHA 或 revision range |
| `path` | 否 | string | 无 | 只返回影响该仓库路径的 Commit |
| `since` / `until` | 否 | string | 无 | ISO 8601 起止时间 |
| `first_parent` | 否 | boolean | `false` | 在合并 Commit 中只跟随第一父节点 |
| `with_stats` | 否 | boolean | `false` | 是否包含增删行统计 |
| `order` | 否 | string | `default` | `default` 或 `topo` |
| `page` | 否 | integer | `1` | 页码 |
| `per_page` | 否 | integer | `30` | 每页数量，1～100 |

- GitLab API：`GET /projects/:id/repository/commits`。
- 示例提示：`列出 platform/order-service 的 release 分支最近 20 个 Commit。`

#### `get_commit`

按 SHA、分支名或 Tag 名获取一个 Commit，适合确认指定 Commit 是否可从目标 ref 解析。

| 参数 | 必填 | 类型 | 默认值 | 说明 |
| --- | --- | --- | --- | --- |
| `project` | 是 | string | 无 | 项目 ID 或完整路径 |
| `sha` | 是 | string | 无 | Commit SHA、分支名或 Tag 名 |
| `stats` | 否 | boolean | GitLab 默认值 | 是否包含 Commit 统计 |

- GitLab API：`GET /projects/:id/repository/commits/:sha`。
- 示例提示：`查看 platform/order-service 的 Commit abc123 详情。`

#### `list_repository_tree`

列出仓库目录中的文件和子目录。

| 参数 | 必填 | 类型 | 默认值 | 说明 |
| --- | --- | --- | --- | --- |
| `project` | 是 | string | 无 | 项目 ID 或完整路径 |
| `path` | 否 | string | 仓库根目录 | 目录路径，例如 `src/api` |
| `git_ref` | 否 | string | 默认分支 | 分支、Tag 或 Commit SHA |
| `recursive` | 否 | boolean | `false` | 是否递归列出子目录 |
| `page` | 否 | integer | `1` | 页码 |
| `per_page` | 否 | integer | `30` | 每页数量，1～100 |

- GitLab API：`GET /projects/:id/repository/tree`。
- 示例提示：`递归列出 platform/order-service 的 main 分支下 src 目录。`

#### `get_repository_file`

读取指定版本的单个文件。返回对象中的 `content` 为 Base64，`encoding` 通常为 `base64`；MCP 客户端或模型需要解码后才能得到原文。

| 参数 | 必填 | 类型 | 说明 |
| --- | --- | --- | --- |
| `project` | 是 | string | 项目 ID 或完整路径 |
| `file_path` | 是 | string | 仓库相对路径，例如 `src/main.rs` |
| `git_ref` | 是 | string | 分支、Tag 或 Commit SHA |

- GitLab API：`GET /projects/:id/repository/files/:file_path`。
- 示例提示：`读取 platform/order-service 的 main 分支上 README.md，并解码内容。`

#### `create_branch`（写操作）

从已有分支、Tag 或 Commit SHA 创建新分支。需要 `GITLAB_ALLOW_WRITE=true` 和具备分支创建权限的 Token；不会覆盖已有分支，也不提供删除能力。Protected Branch 规则仍由 GitLab 强制执行。

| 参数 | 必填 | 类型 | 说明 |
| --- | --- | --- | --- |
| `project` | 是 | string | 项目 ID 或完整路径 |
| `branch` | 是 | string | 新分支名称，例如 `feature/retry-payment` |
| `git_ref` | 是 | string | 起点分支、Tag 或 Commit SHA，例如 `main` |

- GitLab API：`POST /projects/:id/repository/branches`。
- 示例提示：`从 platform/order-service 的 main 创建 feature/retry-payment 分支。`

#### `create_tag`（写操作）

从分支、Commit SHA 或已有 Tag 创建新 Tag。需要 `GITLAB_ALLOW_WRITE=true` 和对应仓库权限；Protected Tag 规则仍由 GitLab 强制执行。

| 参数 | 必填 | 类型 | 说明 |
| --- | --- | --- | --- |
| `project` | 是 | string | 项目 ID 或完整路径 |
| `tag_name` | 是 | string | 新 Tag 名，例如 `v2.3.0` |
| `git_ref` | 是 | string | 目标分支、Commit SHA 或已有 Tag |
| `message` | 否 | string | 提供时创建 annotated tag；省略时创建 lightweight tag |

- GitLab API：`POST /projects/:id/repository/tags`。
- 示例提示：`从 platform/order-service 的 Commit abc123 创建 v2.3.0 Tag，消息为“生产发布 2.3.0”。`

### CI/CD

#### `list_pipelines`

列出项目 Pipeline，可按分支/Tag 和状态过滤。

| 参数 | 必填 | 类型 | 默认值 | 说明 |
| --- | --- | --- | --- | --- |
| `project` | 是 | string | 无 | 项目 ID 或完整路径 |
| `git_ref` | 否 | string | 无 | 分支或 Tag |
| `status` | 否 | string | 无 | 例如 `running`、`pending`、`success`、`failed`、`canceled` |
| `page` | 否 | integer | `1` | 页码 |
| `per_page` | 否 | integer | `30` | 每页数量，1～100 |

- GitLab API：`GET /projects/:id/pipelines`。
- 示例提示：`查找 platform/order-service 的 main 分支最近失败的 Pipeline。`

#### `get_pipeline`

获取单个 Pipeline 的详细状态、ref、SHA、耗时和触发用户等 GitLab 返回字段。

| 参数 | 必填 | 类型 | 说明 |
| --- | --- | --- | --- |
| `project` | 是 | string | 项目 ID 或完整路径 |
| `pipeline_id` | 是 | integer | Pipeline ID |

- GitLab API：`GET /projects/:id/pipelines/:pipeline_id`。
- 示例提示：`查看 platform/order-service 的 Pipeline 98765 详情。`

#### `list_pipeline_jobs`

列出一个 Pipeline 中的 Job，用于继续定位 CI 失败阶段。

| 参数 | 必填 | 类型 | 默认值 | 说明 |
| --- | --- | --- | --- | --- |
| `project` | 是 | string | 无 | 项目 ID 或完整路径 |
| `pipeline_id` | 是 | integer | 无 | Pipeline 全局 ID，来自 `list_pipelines` 返回结果 |
| `page` | 否 | integer | `1` | 页码 |
| `per_page` | 否 | integer | `30` | 每页数量，1～100 |

- GitLab API：`GET /projects/:id/pipelines/:pipeline_id/jobs`。
- 示例提示：`查看 platform/order-service 的 Pipeline 98765 中失败的 Job。`

#### `get_job_trace`

读取一个 GitLab CI Job 的完整文本日志。日志仍受 `GITLAB_MAX_RESPONSE_BYTES` 限制，超限时工具会拒绝返回，避免大量日志直接占用模型上下文。

| 参数 | 必填 | 类型 | 说明 |
| --- | --- | --- | --- |
| `project` | 是 | string | 项目 ID 或完整路径 |
| `job_id` | 是 | integer | Job ID，来自 `list_pipeline_jobs` 返回结果 |

- GitLab API：`GET /projects/:id/jobs/:job_id/trace`。
- 示例提示：`读取 platform/order-service 的 Job 456789 日志并定位失败原因。`

## 推荐工作流

### 配置验证

1. 调用 `get_current_user`，确认连接身份。
2. 调用 `list_projects`，确认 Token 能看到预期项目。
3. 调用 `get_project`，确认完整项目路径可用。
4. 只在确有需要时开启 `GITLAB_ALLOW_WRITE`。

### MR 评审

1. 优先调用 `review_merge_request_context`，一次获取 MR 元数据、diff、Commit、讨论、关联 Issue、Pipeline 和审批状态。
2. 对大型 MR，先关闭 `include_diffs` 获取概览，再按需要调用 `list_merge_request_diffs`。
3. 必要时用 `get_repository_file` 获取目标分支上的完整文件上下文。
4. 用户确认后再用 `add_merge_request_note` 发表评论。

### 环境分支同步确认

1. 用 `get_branch` 读取两个环境分支当前的 Commit SHA。
2. 用 `compare_refs` 并设置 `straight=true` 直接比较两个 ref。
3. 查看返回的 `compare_same_ref`、Commit 和 diff，确认是否同步及差异方向。
4. 需要核对某个 Commit 时，先用 `get_commit` 确认 SHA，再用 `compare_refs` 将该 SHA 与目标分支比较；`list_commits` 可用于浏览目标分支的提交历史。

### CI 失败定位

1. `list_pipelines` 按分支和 `failed` 状态定位 Pipeline。
2. `get_pipeline` 查看 Pipeline 详情，`list_pipeline_jobs` 查找失败 Job。
3. `get_job_trace` 读取失败 Job 日志并定位错误；日志超过响应上限时改用 GitLab Web 页面查看。

## 安全建议

- 优先使用只覆盖目标项目、有效期较短、最小 scope 的令牌。
- 默认保持 `GITLAB_ALLOW_WRITE=false`；只在明确需要时开启。
- 不要把真实 Token 写入 README、脚本、Git 配置或提交记录。
- MCP 客户端配置文件包含 Token 时，应限制文件权限并避免同步到云盘或公开仓库。
- 仅当内部 GitLab 确实使用无法验证的证书时才开启 `GITLAB_INSECURE`；更好的做法是把公司 CA 加入系统信任链。
- 工具返回内容会进入模型上下文。机密项目应确认所使用 MCP 客户端及模型符合公司的数据安全要求。

## 常见问题

### 启动时报 `missing required environment variable`

MCP 客户端没有向子进程传递 `GITLAB_URL` 或 `GITLAB_TOKEN`。检查配置中的 `env` 字段并重启客户端。

### 返回 `401 Unauthorized`

检查 Token 是否过期、`GITLAB_TOKEN_TYPE` 是否匹配。Personal/Project Access Token 通常使用 `private`，OAuth Access Token 使用 `oauth`。

### 返回 `403 Forbidden`

Token 有效，但 scope 或项目角色不足。查询通常需要 `read_api`，写操作通常需要 `api` 以及相应项目权限。

### 返回 `404 Not Found`

确认 `project` 使用完整命名空间路径，例如 `group/subgroup/project`，并确认 Token 能访问该项目。Issue/MR 参数应使用项目内 `iid`。

### 写工具提示 `write tools are disabled`

在 MCP Server 的环境变量中设置 `GITLAB_ALLOW_WRITE=true`，然后重启 MCP 客户端。

### 返回内容超过限制

缩小查询范围、降低 `per_page` 或读取更具体的文件。确有需要时可提高 `GITLAB_MAX_RESPONSE_BYTES`，但这会增加模型上下文消耗。

### 内网证书校验失败

优先把公司 CA 加入系统信任链。临时排查可设置 `GITLAB_INSECURE=true`，但这会降低 TLS 安全性。

## 开发验证

```bash
cargo fmt --check
cargo test
cargo clippy --all-targets -- -D warnings
sh scripts/build-release.sh
```

单元测试使用本地 Mock Server，不会访问真实 GitLab，也不会读取本机 Token。
