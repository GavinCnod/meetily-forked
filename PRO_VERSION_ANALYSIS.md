# Meetily Pro 版本限制逻辑分析报告

> **分析日期**：2026-04-28
> **代码库版本**：v0.3.0 (Community Edition)
> **目的**：梳理所有与 Pro 版本、许可证、注册机制相关的代码，为定制版本（去除发布版限制）提供依据

---

## 核心结论

**该代码库是 Meetily 社区版（Community Edition），不包含任何活跃的 Pro 版本限制逻辑。** Meetily PRO 是一个独立的闭源代码库，不在本仓库中。

这意味着：
- **没有需要"去除"的限制逻辑** — 所有功能在社区版中均不受限制
- 仅有少量与 Pro 许可证系统相关的**残留痕迹**（数据库迁移、API 透传字段、CI 环境变量）
- 这些残留物不影响功能，但如果要交付定制版本，建议清理以保持代码整洁

---

## 1. Pro 与 Community 的定位

来源：`.reviews/CODE_WIKI_OriginVersion.md` 第 60-62 行

| 版本 | 定位 | 代码库 |
|------|------|:---:|
| **Community Edition** | 永久免费开源，包含本地转录、AI 摘要等核心功能 | 本仓库 |
| **Meetily PRO** | 增强准确度、自定义模板、PDF/DOCX 导出、说话人识别 | 独立闭源仓库 |

---

## 2. 残留物清单（15 项）

### 2.1 数据库迁移（SQL）

#### ① `migrations/20251105120000_add_pro_license_custom_openai.sql`

**路径**：`frontend/src-tauri/migrations/20251105120000_add_pro_license_custom_openai.sql`

**内容**：
```sql
-- 为 settings 表添加 CustomOpenAI 配置字段
ALTER TABLE settings ADD COLUMN customOpenAIConfig TEXT;

-- 删除并重建 licensing 表（RSA 加密结构）
DROP TABLE IF EXISTS licensing;
CREATE TABLE licensing (
    license_key TEXT PRIMARY KEY,
    encrypted_key TEXT NOT NULL,
    signature_hash TEXT NOT NULL,
    activation_date TEXT NOT NULL,
    expiry_date TEXT NOT NULL,
    soft_expiry_date TEXT NOT NULL,
    max_activation_time TEXT NOT NULL,
    duration INTEGER NOT NULL,
    generated_on TEXT NOT NULL,
    is_soft_expired INTEGER DEFAULT 0
);
```

**分析**：此迁移创建了 `licensing` 表（RSA 加密许可证存储）和 `customOpenAIConfig` 字段。实际的许可证验证逻辑在 Pro 代码库中。社区版中此表仅被 SQLx migration 自动创建，**无任何 Rust 代码读写该表**。

#### ② `migrations/20251110000000_add_grace_period_to_licensing.sql`

**路径**：`frontend/src-tauri/migrations/20251110000000_add_grace_period_to_licensing.sql`

**内容**：
```sql
ALTER TABLE licensing ADD COLUMN grace_period INTEGER NOT NULL DEFAULT 604800;
-- 默认值 604800 秒 = 7 天宽限期
```

**分析**：为 `licensing` 表添加宽限期字段。仅在 Pro 代码库中使用。

---

### 2.2 Rust 后端

#### ③ API 透传结构体：`Profile`

**路径**：`frontend/src-tauri/src/api/api.rs` 第 194-204 行

```rust
pub struct Profile {
    pub id: String,
    pub name: Option<String>,
    pub email: String,
    pub license_key: String,       // ← 许可证密钥透传
    pub company: Option<String>,
    pub position: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub is_licensed: bool,         // ← 许可证状态来自后端响应
}
```

**分析**：此结构体用于与外部后端（`http://localhost:5167`）通信。`license_key` 和 `is_licensed` 字段是透传数据，**不执行本地验证**。

#### ④ API 命令：`api_get_profile`

**路径**：`frontend/src-tauri/src/api/api.rs` 第 386-403 行

```rust
#[tauri::command]
pub async fn api_get_profile<R: Runtime>(
    app: AppHandle<R>,
    email: String,
    license_key: String,   // ← 作为参数传递给后端
    auth_token: Option<String>,
) -> Result<Profile, String> { ... }
```

**分析**：将 `license_key` 发送至后端进行验证。社区版中后端通常不可用，此函数在引导流程（Onboarding）中调用。

#### ⑤ API 命令：`api_update_profile`

**路径**：`frontend/src-tauri/src/api/api.rs` 第 432-463 行

```rust
pub async fn api_update_profile<R: Runtime>(
    app: AppHandle<R>,
    email: String,
    license_key: String,   // ← 更新许可证
    company: String,
    position: String,
    auth_token: Option<String>,
) -> Result<serde_json::Value, String> { ... }
```

#### ⑥ API 透传结构体：`ProfileRequest` / `UpdateProfileRequest`

**路径**：`frontend/src-tauri/src/api/api.rs` 第 50-67 行

```rust
pub struct ProfileRequest {
    pub email: String,
    pub license_key: String,    // ← 请求中的许可证密钥
}

pub struct UpdateProfileRequest {
    pub email: String,
    pub license_key: String,    // ← 更新时的许可证密钥
    pub company: String,
    pub position: String,
}
```

#### ⑦ CustomOpenAI Provider 枚举值

**路径**：`frontend/src-tauri/src/summary/llm_client.rs` 第 67-76 行

```rust
pub enum LLMProvider {
    OpenAI,
    Claude,
    Groq,
    Ollama,
    OpenRouter,
    BuiltInAI,
    CustomOpenAI,   // ← 与 Pro 许可证迁移一同引入
}
```

**分析**：`CustomOpenAI` provider 允许用户连接任意 OpenAI 兼容 API。虽然随 Pro 许可证迁移一起引入，但社区版中**该功能完全可用且不受限制**。参见 `api.rs` 中的 `api_save_custom_openai_config`、`api_get_custom_openai_config`、`api_test_custom_openai_connection` 命令。

#### ⑧ CustomOpenAIConfig 配置结构体

**路径**：`frontend/src-tauri/src/summary/mod.rs` 第 13-31 行

```rust
pub struct CustomOpenAIConfig {
    pub endpoint: String,
    pub api_key: Option<String>,
    pub model: String,
    pub max_tokens: Option<i32>,
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
}
```

**分析**：此配置存储于 `settings` 表的 `customOpenAIConfig` 字段（JSON 格式）。功能在社区版中完全可用。

#### ⑨ settings 表相关方法

**路径**：`frontend/src-tauri/src/database/repositories/setting.rs`
- `save_custom_openai_config()` — 保存 CustomOpenAI 配置
- `get_custom_openai_config()` — 读取 CustomOpenAI 配置

**路径**：`frontend/src-tauri/src/database/models.rs` 第 96-98 行
```rust
pub custom_openai_config: Option<String>,  // JSON 存储
pub fn get_custom_openai_config(&self) -> Option<CustomOpenAIConfig> { ... }
```

#### ⑩ 数据库初始化（无许可证逻辑）

**路径**：`frontend/src-tauri/src/database/manager.rs` 第 35 行

```rust
sqlx::migrate!("./migrations").run(&pool).await?;
```

**分析**：此代码自动执行所有 SQL 迁移文件。`licensing` 表被创建，但无代码使用。**无需修改**。

#### ⑪ Tauri 命令注册（无许可证相关命令）

**路径**：`frontend/src-tauri/src/lib.rs` 第 499-719 行

**分析**：注册了约 180 个 Tauri 命令。**不存在**以下命令：`check_license`、`activate_license`、`validate_license`、`is_licensed`、`is_pro`。

#### ⑫ build.rs（不嵌入 RSA 公钥）

**路径**：`frontend/src-tauri/build.rs`

**分析**：仅处理 FFmpeg 下载和 GPU 检测。**不嵌入 `MEETILY_RSA_PUBLIC_KEY`** 到二进制中。Pro 版本的 build.rs 会使用 `env!("MEETILY_RSA_PUBLIC_KEY")` 在编译时嵌入公钥。

---

### 2.3 CI/CD 工作流

#### ⑬ GitHub Actions 环境变量

**路径**（所有构建工作流文件）：

| 文件 | 行号 | 变量 |
|------|:---:|------|
| `.github/workflows/build.yml` | 585-588 | `MEETILY_RSA_PUBLIC_KEY`, `SUPABASE_URL`, `SUPABASE_ANON_KEY` |
| `.github/workflows/build-windows.yml` | 677-680 | 同上 |
| `.github/workflows/build-macos.yml` | 173-176, 190-193 | 同上 |
| `.github/workflows/build-linux.yml` | 209-212 | 同上 |
| `.github/workflows/build-devtest.yml` | 390-393, 408-411 | 同上 |

**注释原文**：
```yaml
# License validation RSA public key (embedded at build time)
MEETILY_RSA_PUBLIC_KEY: ${{ secrets.MEETILY_RSA_PUBLIC_KEY }}
# Supabase configuration (for online license verification)
SUPABASE_URL: ${{ secrets.SUPABASE_URL }}
SUPABASE_ANON_KEY: ${{ secrets.SUPABASE_ANON_KEY }}
```

**分析**：这些 secrets 仅在 GitHub Actions 的**私有仓库**中可用。对于 fork 仓库或本地构建，这些值为空。Pro 版本的 build.rs 通过 `env!("MEETILY_RSA_PUBLIC_KEY")` 在编译时嵌入公钥；社区版的 build.rs 不使用这些变量，所以值的缺失不影响构建。

#### ⑭ 版本命名限制

**路径**：`.github/workflows/WORKFLOWS_OVERVIEW.md` 第 320 行

```
- Use `0.1.3` not `0.1.2-pro-trial`
```

**分析**：这是一条工作流规范，禁止使用 `-pro-trial` 后缀命名版本。对定制版本无影响。

---

### 2.4 前端 TypeScript

#### ⑮ 无 Pro 相关功能开关

**搜索结果**：对 `is_pro`、`ProFeature`、`pro_feature`、`is_premium`、`trial`、`activation` 等关键字的搜索，在所有 `.ts` 和 `.tsx` 文件中返回零个相关匹配。

**分析**：前端代码中不存在任何 Pro 版本功能开关或权限检查。所有 UI 组件对所有用户开放。

---

## 3. 许可证表结构（仅供理解残留物）

`licensing` 表仅在 Pro 代码库中活跃使用。其字段含义如下：

| 字段 | 类型 | 用途 |
|------|------|------|
| `license_key` | TEXT PK | 解密后的许可证 ID |
| `encrypted_key` | TEXT | RSA + Base64 加密的原始密钥 |
| `signature_hash` | TEXT | encrypted_key 的 SHA-256 哈希（完整性校验） |
| `activation_date` | TEXT | ISO 8601 激活时间戳 |
| `expiry_date` | TEXT | 激活日期 + 有效期 |
| `soft_expiry_date` | TEXT | 到期日 + 宽限期 |
| `max_activation_time` | TEXT | 最晚激活时间（防止旧许可证滥用） |
| `duration` | INTEGER | 许可证有效期（秒） |
| `generated_on` | TEXT | 许可证生成时间 |
| `is_soft_expired` | INTEGER | 0=活跃, 1=软过期, 2=硬阻止 |
| `grace_period` | INTEGER | 宽限期（秒），默认 604800 (7天) |

**许可证生命周期**（Pro 版本中）：
```
生成 → 激活(activation_date) → 有效期(duration) → 到期(expiry_date)
                                                    → 宽限期(grace_period) → 软过期(soft_expiry_date)
                                                                              → 硬阻止
```

---

## 4. 定制版本建议操作

### 4.1 无需操作（零风险）

以下残留物不影响功能，可保留：
- CI 工作流中的 `MEETILY_RSA_PUBLIC_KEY` / `SUPABASE_*` 环境变量（仅 CI 使用，本地不可见）
- `api_get_profile` / `api_update_profile` 命令（仅当用户主动调用时触发，且连接不到后端会自然失败）
- `licensing` 表的 SQL 迁移（表被创建但完全闲置，占用磁盘 < 1KB）

### 4.2 建议清理（保持代码整洁）

| 操作 | 方式 | 影响 |
|------|------|------|
| 删除 `licensing` 表迁移 | 删除或注释 `20251105120000_add_pro_license_custom_openai.sql` 中 `licensing` 表相关 SQL（保留 `customOpenAIConfig` 字段） | 新安装不再创建空表 |
| 删除 `grace_period` 迁移 | 删除 `20251110000000_add_grace_period_to_licensing.sql` | 同上 |
| 清理 `Profile` 结构体中的许可证字段 | 将 `license_key` 和 `is_licensed` 设为 `Option` 或移除 | 需同步修改 `api_get_profile` 调用方；不影响核心功能 |
| 移除 CI secrets 引用 | 修改 workflow YAML 文件，删除 `MEETILY_RSA_PUBLIC_KEY` / `SUPABASE_*` 行 | 若使用自己的 CI 构建，避免缺少 secrets 导致的构建警告 |
| 移除版本命名限制 | 删除 `WORKFLOWS_OVERVIEW.md` 中 `-pro-trial` 限制行 | 文档清理 |

### 4.3 注意事项

1. **`customOpenAIConfig` 字段不能删**：它存储用户的自定义 OpenAI 兼容端点配置，社区版用户正常使用。虽然与 Pro 许可证迁移在同一 SQL 文件中，但它是独立功能。
2. **删除 SQL 迁移会影响新安装**：已有数据库不受影响。新安装将不再有 `licensing` 表。如果要清理已有数据库，需额外编写清理迁移。
3. **`CustomOpenAI` LLM Provider 不需移除**：这是社区版的合法功能（允许连接任意 OpenAI 兼容 API），不是 Pro 独占功能。

---

## 5. 文件索引

所有与 Pro/许可证相关的文件一览：

| 文件 | 类型 | 内容 |
|------|------|------|
| `frontend/src-tauri/migrations/20251105120000_add_pro_license_custom_openai.sql` | SQL 迁移 | 创建 `licensing` 表 + `customOpenAIConfig` 字段 |
| `frontend/src-tauri/migrations/20251110000000_add_grace_period_to_licensing.sql` | SQL 迁移 | 为 `licensing` 表添加 `grace_period` 字段 |
| `frontend/src-tauri/src/api/api.rs:50-67` | Rust 结构体 | `ProfileRequest`, `UpdateProfileRequest`（含 license_key） |
| `frontend/src-tauri/src/api/api.rs:194-204` | Rust 结构体 | `Profile`（含 license_key, is_licensed） |
| `frontend/src-tauri/src/api/api.rs:386-403` | Rust 命令 | `api_get_profile`（验证许可证） |
| `frontend/src-tauri/src/api/api.rs:432-463` | Rust 命令 | `api_update_profile`（更新许可证） |
| `frontend/src-tauri/src/summary/llm_client.rs:67-76` | Rust 枚举 | `LLMProvider::CustomOpenAI` |
| `frontend/src-tauri/src/summary/mod.rs:13-31` | Rust 结构体 | `CustomOpenAIConfig` |
| `frontend/src-tauri/src/database/models.rs:96-98` | Rust 模型 | `Setting.custom_openai_config` |
| `frontend/src-tauri/src/database/repositories/setting.rs` | Rust 仓库 | `save/get_custom_openai_config()` |
| `frontend/src-tauri/src/database/manager.rs:35` | Rust | `sqlx::migrate!()` 自动执行迁移 |
| `frontend/src-tauri/build.rs` | Rust build | 不嵌入 RSA 公钥（Pro 版本会嵌入） |
| `.github/workflows/build.yml:585-588` | CI | `MEETILY_RSA_PUBLIC_KEY` / `SUPABASE_*` secrets |
| `.github/workflows/build-windows.yml:677-680` | CI | 同上 |
| `.github/workflows/build-macos.yml:173-176,190-193` | CI | 同上 |
| `.github/workflows/build-linux.yml:209-212` | CI | 同上 |
| `.github/workflows/build-devtest.yml:390-393,408-411` | CI | 同上 |
| `.github/workflows/WORKFLOWS_OVERVIEW.md:300-302,320` | CI 文档 | Secrets 说明 + 版本命名限制 |

---

## 6. 总结

**这个代码库不包含任何需要"破解"或"绕过"的 Pro 限制。** 所有功能对用户完全开放。license 相关的残留物是 Pro 独立代码库的"影子"，在社区版中不执行任何逻辑。对于定制版本，建议清理第 4.2 节列出的 SQL 迁移文件和 CI 配置引用，其他代码保留即可。
