# PRD：转录专业术语定制化功能

> **文档状态**：V4.0（基于 GPT 对 V3.3 的审查后修订）
> **创建日期**：2026-04-27
> **修订日期**：2026-04-27
> **关联项目**：Meetily v0.3.0
> **目标行业**：危险化学品制造业（日系企业）
> **支持语言**：日本語 / 中文 / English
>
> **版本里程碑**：本版本已完成所有已知结构级问题的修复，可交付研发进入技术详细设计和 POC 开发。

---

## 目录

1. [需求背景](#1-需求背景)
2. [核心问题分析](#2-核心问题分析)
3. [总体架构设计](#3-总体架构设计)
4. [审计与证据链设计](#4-审计与证据链设计)
5. [Phase 0：基线测量与方案验证](#5-phase-0基线测量与方案验证)
6. [第一级：Whisper initial_prompt 软引导](#6-第一级whisper-initial_prompt-软引导)
7. [第二级：正则实时校正通道](#7-第二级正则实时校正通道)
8. [第三级：LLM 深度校正建议](#8-第三级llm-深度校正建议)
9. [与现有录音停止链路的集成](#9-与现有录音停止链路的集成)
10. [TranscriptUpdate 事件扩展](#10-transcriptupdate-事件扩展)
11. [迁移策略与受影响模块](#11-迁移策略与受影响模块)
12. [数据库设计](#12-数据库设计)
13. [后端实现规范](#13-后端实现规范)
14. [前端实现规范](#14-前端实现规范)
15. [配置与存储](#15-配置与存储)
16. [硬件要求与降级策略](#16-硬件要求与降级策略)
17. [实施计划与 MVP 策略](#17-实施计划与-mvp-策略)
18. [测试策略](#18-测试策略)
19. [风险与应对](#19-风险与应对)
20. [合规与法务审查](#20-合规与法务审查)
21. [回滚与功能淘汰](#21-回滚与功能淘汰)

---

## 1. 需求背景

### 1.1 业务场景

客户为在华日系危险化学品制造企业。日常会议特征：

| 特征 | 说明 |
|------|------|
| **多语言混合** | 会议中频繁切换日语、中文、英语 |
| **高度专业化** | 涉及 MSDS、CAS 编号、UN 危险货物编号、GHS 分类、IUPAC 命名等 |
| **合规要求严格** | 转录文本用于内部审计与合规存档，术语准确性直接影响法律风险 |
| **三方沟通** | 日方技术人员（日语）、中方操作人员（中文）、国际供应商/客户（英语）共同参会 |

### 1.2 问题描述

Meetily 使用 Whisper / Parakeet 进行本地语音识别（STT）。危化品行业日企场景的关键挑战：

1. **跨语言术语混乱**
2. **化学物质名称识别率极低**
3. **安全编码格式特殊**（CAS RN、UN No.、GHS 代码）
4. **片假名/汉字/罗马字混合**

### 1.3 目标

- **强制保留原始 STT 输出**，校正结果分层存储，证据链完整
- **多语言支持**：日语、中文、英语
- **三级校正管道**：模型内 `initial_prompt` → 正则精确替换 → LLM 校正**建议**
- **术语表可溯源**：术语来源可查，校正时记录术语快照版本
- 转录时实时应用 L1+L2，录音停止后通过**持久化串行队列**异步触发 L3
- L3 校正默认仅建议，需用户确认后生效；V1 仅纠错模式

---

## 2. 核心问题分析

### 2.1 STT 输出错误规律

#### 日语

| 错误类型 | 示例 | 根因 |
|----------|------|------|
| 片假名 → 汉字误转 | `ポリウレタン` → `保利売れたん` | token 覆盖不足 |
| 長音/促音丢失 | `メチルエチルケトン` → 长音被省略 | 特殊假名 token 不敏感 |
| 浊音/半浊音混淆 | `ポリ` → `ボリ` / `ホリ` | 噪声下辅音歧义 |
| 拗音分割 | `メチル` → `メ チル` | 拗音被拆分 |

#### 中文

| 错误类型 | 示例 | 根因 |
|----------|------|------|
| 同音字替换 | `甲苯二异氰酸酯` → `甲本二亿情酸纸` | 化学术语汉字组合生僻 |
| 数字+字母错位 | `H225` → `H二二五` | 中英混合 token 不稳定 |
| 多音字误读 | `重铬酸钾` → `重各酸钾` | 化学用字多音 |

#### 英语（化学语境）

| 错误类型 | 示例 | 根因 |
|----------|------|------|
| IUPAC 命名拆分 | `2,4,6-Trinitrotoluene` → `two four six...` | 数字序列 token 化不熟 |
| CAS 编号格式 | `CAS 108-88-3` → `k Ass one o eight...` | 连字符边界错误 |
| GHS 代码 | `H301` → `H three hundred one` | H+数字不在常见 token 表 |

#### 跨语言混合

| 错误类型 | 示例 |
|----------|------|
| 日语→中文误转 | `この物質は…` → `这个物质是…` |
| 代码混入自然语言 | `UN 1203` → `うん いちにーぜろさん` |

### 2.2 设计决策：三级校正管道

| 维度 | L1 | L2 | L3 |
|------|:---|:---|:---|
| **执行时机** | Whisper 推理时 | 每次转录输出后 | 录音停止后（持久化串行队列） |
| **支持引擎** | 仅 Whisper | 全部 | 全部 |
| **覆盖率（估计）** | ~15-30% | ~50-60% | ~10-20% |
| **确定性** | 否 | 是 | 否 |

---

## 3. 总体架构设计

```
┌──────────────────────────────────────────────────────────────────────────────┐
│                          前端 (Next.js + React)                               │
│                                                                              │
│  TranscriptContext (双轨 buffer, useRef + 间隔同步)                            │
│  useRecordingStop (停止编排: 保存 + L3 入队)                                   │
│  storageService / transcriptService / indexedDBService / useTranscriptRecovery│
│                                                                              │
│  TerminologyManager (含失效规则标记)     CorrectionDiffView (聚类 + 风险提示)    │
└──────────────────────────────────┬───────────────────────────────────────────┘
                                   │ invoke() / listen()
┌──────────────────────────────────┴───────────────────────────────────────────┐
│                      Tauri IPC 层 (Rust Backend)                              │
│                                                                              │
│  terminology/                           database/                             │
│  ├─ cache.rs: 原子刷新 + snapshot_hash    ├─ terminology 表 (含 package 字段)  │
│  ├─ corrector.rs: 混合引擎               ├─ l3_correction_jobs (任务状态)     │
│  ├─ queue.rs: 持久化队列 + 恢复           ├─ transcript_corrections (建议审核)  │
│  └─ commands.rs                        ├─ transcripts 表 (raw + hash)       │
│                                         └─ 幂等迁移                           │
│  转录管道: whisper_engine (L1) → worker.rs (L2) → emit transcript-update      │
│  后处理:   llm_client (L3) → 持久化队列 → 建议写入 DB → emit corrections-ready  │
└──────────────────────────────────────────────────────────────────────────────┘
```

---

## 4. 审计与证据链设计

### 4.1 四层文本模型

| 层 | 存储位置 | 可变性 | 用途 |
|---|---------|:---:|------|
| **L0** | `transcripts.raw_transcript` | **不可变** | 审计锚点 |
| **L1+L2** | `transcripts.transcript` | 可更新 | 实时显示 |
| **L3 建议** | `transcript_corrections` | status 可变 | 校正候选 |
| **Final** | `transcripts.transcript`（更新后） | 接受后写入 | 归档导出 |

### 4.2 raw_transcript 在 meeting details 中的可见性

| 环境 | 行为 |
|------|------|
| **普通用户（默认）** | `raw_transcript` **默认隐藏**。meeting details 仅展示 `transcript`（L1+L2 后） |
| **调试/审计模式** | 用户可通过设置中的"显示原始转录"开关启用。开启后在 meeting details 中出现"查看原始转录"折叠面板，面板标题注明"原始 STT 输出，未经术语校正" |
| **导出** | 合规导出时同时包含 raw 和 final 两列 |

### 4.3 术语快照版本

保存时记录术语表 SHA-256 摘要到 `transcripts.terminology_snapshot_hash`。

| 版本 | 能力 |
|------|------|
| **V1** | `snapshot_hash` — 快照指纹，可证明"规则集与现在不同" |
| **Future** | 术语表历史快照 — 可完整重建"当时有哪些条目" |

### 4.4 审计追溯能力

| 问题 | 答案来源 |
|------|----------|
| 原始模型输出了什么？ | `raw_transcript`（不可变） |
| L2 用了哪套规则？ | `terminology_snapshot_hash` |
| L3 任务何时执行、结果如何？ | `l3_correction_jobs` |
| L3 产生了哪些建议？ | `transcript_corrections` |
| 用户接受了哪些？ | `status = 'accepted'` + `reviewed_by` + `reviewed_at` |

---

## 5. Phase 0：基线测量与方案验证

### 5.1 验证清单

| # | 验证项 | 阻塞级别 | 若失败的回退 |
|---|--------|:---:|------|
| 1 | whisper-rs 是否暴露 `set_initial_prompt` | **P0** | 废弃 L1 |
| 2 | `fancy-regex` 在当前 Rust 版本下可编译且通过 TC-01~TC-06 | **P0** | 日/中全词匹配降级为子串匹配，**Phase 1A 验收标准中移除日/中 whole-word 相关承诺** |
| 3 | Whisper 实际使用的 tokenizer 类型确认 | **P1** | 回退保守估算 |
| 4 | `regex` 1.11 对 `\p{Katakana}`/`\p{Han}` 的实际行为 | **P1** | — |
| 5 | 当前 transcript-update 监听链路追踪 | **P1** | — |

### 5.2 预估工作量

1-2 人天。

---

## 6. 第一级：Whisper initial_prompt 软引导

### 6.1 实现原理

Whisper.cpp 的 `initial_prompt` 参数通过 decoder cross-attention 偏置 token 分布。软引导，非确定性。~224 token 硬限制。仅对 Whisper 引擎生效。

### 6.2 Token 计数方案

**方案 A（优先，需 Phase 0 验证）**：若确认 Whisper tokenizer 与 `tiktoken-rs` 兼容，使用精确 BPE 计数。

**方案 B（回退）**：若无法确认，使用保守估算（偏向高估，宁可少注入不超限）：

```rust
fn estimate_tokens_fallback(text: &str, lang: &str) -> usize {
    match lang {
        "ja" => (text.chars().count() as f64 * 0.8) as usize,
        "zh" => (text.chars().count() as f64 * 0.9) as usize,
        _    => (text.chars().count() as f64 * 0.4) as usize,
    }
}
```

### 6.3 Token 截断策略

按 `priority=high` → `updated_at` 降序 → 贪心拼接 → 超限截断。被排除术语 ID 通知前端。

### 6.4 L1 Prompt 审计存储

- 运行日志：仅记录术语数量和截断状态
- 完整 prompt 快照：写入 `transcripts.l1_prompt_snapshot`

### 6.5 代码集成位置

`whisper_engine.rs`，`FullParams` 构造后、`state.full()` 前。仅当 Phase 0 验证 API 可用时。

---

## 7. 第二级：正则实时校正通道

### 7.1 混合引擎方案

| 规则类型 | 引擎 |
|----------|------|
| `whole_word = false`（所有语言） | `regex` |
| `whole_word = true`，语言 = `en` | `regex` + `\b` |
| `whole_word = true`，语言 = `ja`/`zh` | **`fancy-regex`**（look-around） |

### 7.2 降级承诺

若 Phase 0 验证 `fancy-regex` 不可用：

- 日/中 `whole_word = true` 规则降级为 `whole_word = false`（子串匹配）
- **Phase 1A 验收标准中移除"日/中全词匹配不误替换"的承诺**
- UI 中标注"日语/中文全词匹配暂不可用"
- 预留 `fancy-regex` 依赖声明，后续版本可重新启用

### 7.3 编译失败优雅降级

`build_term_rule` 返回 `Err(RuleBuildError)` 时，该术语在 UI 标记为"⚠ 规则语法错误/已失效"，自动设置 `enabled = false`。

### 7.4 关键测试用例

```
TC-01: 日语连续术语（验证无消费效应）
TC-02: 日语行首/行尾正确匹配
TC-03: 中文短术语不命中长术语
TC-04: 中文同音字变体正确替换
TC-05: 日语浊音变体仅替换错误项
TC-06: 混合语言不误替换
```

### 7.5 性能基准

预估值，以 Phase 1 实测为准。熔断：> 100ms → 仅 high 规则 + UI 警告。

### 7.6 L2 可关闭性

全局 `terminology_enabled = 0`，或按 `package_id` 批量禁用。

---

## 8. 第三级：LLM 深度校正建议

### 8.1 模式定义：仅纠错，不扩写

| 操作 | V1 纠错模式 | Future 扩写模式 |
|------|:---:|:---:|
| 修正 STT 识别错误 | ✅ | ✅ |
| 统一术语表记 | ✅ | ✅ |
| 缩写展开为全称+括号 | ❌ | ✅ |
| 添加原文没有的解释 | ❌ | ❌ |

### 8.2 数据模型拆分：任务 vs 建议

> **V4.0 关键修正**：L3 任务队列状态与建议审核状态是两类不同对象，必须拆分存储。

#### 8.2.1 `l3_correction_jobs`（任务表 — 一张表一条任务）

```sql
CREATE TABLE IF NOT EXISTS l3_correction_jobs (
    id              TEXT PRIMARY KEY,
    meeting_id      TEXT NOT NULL UNIQUE,  -- 一场会议一条任务（幂等）
    status          TEXT NOT NULL DEFAULT 'queued',
    -- 状态: 'queued' | 'running' | 'done' | 'failed' | 'timeout'
    error_detail    TEXT,
    attempt_count   INTEGER NOT NULL DEFAULT 1,
    created_at      TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at      TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (meeting_id) REFERENCES meetings(id) ON DELETE CASCADE
);
```

#### 8.2.2 `transcript_corrections`（建议表 — 一条记录一条建议）

```sql
CREATE TABLE IF NOT EXISTS transcript_corrections (
    id                  TEXT PRIMARY KEY,
    meeting_id          TEXT NOT NULL,
    job_id              TEXT NOT NULL,       -- 关联 l3_correction_jobs.id
    original_span       TEXT NOT NULL,
    suggested_text      TEXT NOT NULL,
    occurrences_json    TEXT,                -- JSON: [{start, end}, ...]
    language            TEXT,
    correction_type     TEXT NOT NULL DEFAULT 'llm',
    reason              TEXT,
    source_snapshot_hash TEXT,
    status              TEXT NOT NULL DEFAULT 'pending',
    -- 建议审核状态: 'pending' | 'accepted' | 'rejected' | 'obsolete'
    reviewed_by         TEXT,
    reviewed_at         TEXT,
    created_at          TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (meeting_id) REFERENCES meetings(id) ON DELETE CASCADE,
    FOREIGN KEY (job_id) REFERENCES l3_correction_jobs(id) ON DELETE CASCADE
);
```

#### 8.2.3 两个状态机

```
l3_correction_jobs.status:          transcript_corrections.status:
                                   
  queued ──→ running                  pending ──→ accepted
    │          │                         │           (reviewed_by, reviewed_at)
    │          ├─→ done                  ├─→ rejected
    │          ├─→ failed                └─→ obsolete (版本冲突)
    │          └─→ timeout
    │                                   
    └─ (重试: attempt_count + 1)
```

**查询语义清晰**：
- "L3 任务是否完成" → `SELECT status FROM l3_correction_jobs WHERE meeting_id = ?`
- "有哪些待审核建议" → `SELECT * FROM transcript_corrections WHERE meeting_id = ? AND status = 'pending'`

### 8.3 L3 持久化任务队列

```rust
/// 入队 L3 校正任务
async fn enqueue_l3_correction(db: &Db, meeting_id: &str) -> Result<QueueStatus> {
    // 1. 幂等检查：l3_correction_jobs 中已有 queued/running 记录 → 不重复入队
    if let Some(job) = db.find_active_l3_job(meeting_id).await? {
        return Ok(QueueStatus::AlreadyQueued);
    }

    // 2. 创建任务记录（入队即写 DB，崩溃不丢任务）
    let job_id = uuid::Uuid::new_v4().to_string();
    db.insert_l3_job(&job_id, meeting_id, "queued").await?;

    // 3. 异步执行
    let db = db.clone();
    let jid = job_id.clone();
    let mid = meeting_id.to_string();
    tauri::async_runtime::spawn(async move {
        let _permit = LLM_CORRECTION_QUEUE.acquire().await;
        db.update_l3_job_status(&jid, "running").await.ok();

        match tokio::time::timeout(Duration::from_secs(60), run_llm_correction(&db, &mid)).await {
            Ok(Ok(suggestions)) => {
                // 写入建议到 transcript_corrections (status='pending')
                db.insert_l3_suggestions(&jid, &mid, &suggestions).await.ok();
                db.update_l3_job_status(&jid, "done").await.ok();
            }
            Ok(Err(e)) => {
                db.update_l3_job_status(&jid, "failed")
                  .set_error_detail(&e.to_string()).await.ok();
            }
            Err(_) => {
                db.update_l3_job_status(&jid, "timeout").await.ok();
            }
        }
    });

    Ok(QueueStatus::Queued)
}
```

**关键约束**：
- 入队即写 DB（`l3_correction_jobs`），不依赖内存
- 同一 meeting 幂等（UNIQUE 约束 + 逻辑检查）
- 超时/失败保留状态 + 错误详情，前端可触发重试（`attempt_count` + 1）
- App 重启恢复：查询 `status IN ('queued','running')` 重新入队

### 8.4 L3 建议数据结构

```rust
pub struct L3CorrectionSuggestion {
    pub id: String,
    pub meeting_id: String,
    pub job_id: String,
    pub original_span: String,
    pub suggested_text: String,
    pub occurrences: Vec<CharRange>,
    pub language: String,
    pub correction_type: String,
    pub reason: String,
    pub source_snapshot_hash: String,
    pub status: String,  // 'pending' | 'accepted' | 'rejected' | 'obsolete'
    pub reviewed_by: Option<String>,
    pub reviewed_at: Option<String>,
}

pub struct CharRange {
    pub start: usize,  // Unicode scalar value 索引
    pub end: usize,
}
```

### 8.5 Offset 语义定义

统一采用 **Unicode scalar value 计数**。Rust: `s.char_indices()`。JS: `[...str]` 展开后索引。**禁止使用 `String.length`（UTF-16 code unit）**。

### 8.6 L3 "全文 N 处统一接受"的限制与风险控制

> **受控简化策略**。以下约束为硬性规则，非可选建议。

| 约束 | 触发条件 | 行为 |
|------|----------|------|
| LLM Prompt 要求 | 始终 | `original_span` 至少包含 3-4 个字符 |
| 短子串边界检查 | `original_span` < 3 字符 | 逐条确认模式，禁用"一键接受全部" |
| 高频片段检测 | 匹配次数 > 全文字符数的 1/3 | **禁用"接受全部"**，仅允许逐条展开确认。标记警告："该片段在全文高频出现，可能存在误匹配" |
| 高风险术语 | `correction_type` = `ghs_code` / `cas_number` / `un_number` | 默认逐条展开确认，**不提供"一键接受全部"按钮** |
| 全文 N 处提示 | 始终 | UI 显示"以下建议基于全文同串匹配。如仅需修正部分出现，请点击[逐条展开]" |

### 8.7 LLM Provider 与降级链

默认 `qwen2.5:7b` → 降级 `3b` → 静默放弃。

### 8.8 前端交互：按术语聚类 + 风险分级

```
┌──────────────────────────────────────────────────────────────┐
│  L3 深度校正建议 (共 23 条, 涉及 5 个术语)    状态: ✅ 已完成  │
│  ⚠️ 以下建议基于全文同串匹配，如仅需修正部分出现请逐条展开       │
│                                                              │
│  ┌─ 🟡 化学名: ポリウレタン (全文 8 处) ────────────────┐   │
│  │  [一键接受全部 8 处]  [逐条查看上下文]  [全部拒绝]    │   │
│  └──────────────────────────────────────────────────────┘   │
│                                                              │
│  ┌─ 🔴 高风险代码: H225 (全文 5 处) ───────────────────┐   │
│  │  ⚠️ 安全代码类术语，需逐条确认上下文                     │   │
│  │  [逐条展开确认 (5处)]  [全部拒绝]                       │   │
│  └──────────────────────────────────────────────────────┘   │
│                                                              │
│  ┌─ ⚠️ 高频片段: 酸 → 酸性 (全文 47 处) ───────────────┐   │
│  │  ⚠️ 该片段在全文中高频出现，已禁用批量接受               │   │
│  │  [逐条展开确认 (47处)]  [全部拒绝]                      │   │
│  └──────────────────────────────────────────────────────┘   │
│                                                              │
│  [接受全部（仅允许批量接受的术语）]  [拒绝全部]               │
└──────────────────────────────────────────────────────────────┘
```

### 8.9 auto_accept 策略

字段保留，**V1 UI 不暴露**，默认强制 `false`。

### 8.10 L3 接受后摘要重新生成提示

用户接受 L3 校正后提示可手动触发重新生成摘要。

---

## 9. 与现有录音停止链路的集成

### 9.1 当前链路（基于实际代码）

```
recording_manager.rs: stop_recording() → emit("recording-stopped", { meeting_id })

transcriptService.ts (~line 57): listen<TranscriptUpdate>('transcript-update', ...)

TranscriptContext.tsx (~line 326): 聚合 → indexedDBService.saveTranscript(meetingId, update)

useRecordingStop.ts (~line 80): 编排停止 → storageService.saveMeeting(title, transcripts, folderPath)

storageService.ts (~line 44): invoke('api_save_transcript', { meetingTitle, transcripts, folderPath })

useTranscriptRecovery.ts: IndexedDB → 恢复未保存 transcript
```

### 9.2 术语校正集成后的链路

```
recording_manager.rs: stop_recording() → emit("recording-stopped", { meeting_id })

useRecordingStop.ts:
  │
  ├─► 保存（同步等待）
  │     从 TranscriptContext 读取双轨 buffer
  │     await storageService.saveMeeting(...) // 新命令
  │       // 'save_transcript_with_terminology' { raw, normalized, hash }
  │     // 旧 'api_save_transcript' 保留兼容
  │
  ├─► 异步 L3 入队
  │     await invoke("run_llm_terminology_correction", { meeting_id })
  │     // L3 任务写入 l3_correction_jobs (status='queued')
  │
  └─► 监听 L3 完成
        listen("llm-corrections-ready", ...)
```

### 9.3 职责划分

| 职责 | 模块 |
|------|------|
| 双轨状态管理 | `TranscriptContext` |
| 停止流程编排 | `useRecordingStop` |
| IPC 封装 | `storageService` |
| 本地容灾缓存 | `indexedDBService` |
| 崩溃恢复 | `useTranscriptRecovery` |

---

## 10. TranscriptUpdate 事件扩展

### 10.1 扩展后结构

```rust
pub struct TranscriptUpdate {
    // 现有字段（向后兼容）
    pub text: String,
    pub timestamp: String,
    pub source: String,
    pub sequence_id: u64,
    pub chunk_start_time: f64,
    pub is_partial: bool,
    pub confidence: f32,
    pub audio_start_time: f64,
    pub audio_end_time: f64,

    // 新增字段（可选）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub corrections_applied: Option<u32>,
}
```

### 10.2 worker.rs 集成位置

所有 STT 引擎分支中，`cleaned_text` 赋值后统一插入：

```rust
let raw_text = text.trim().to_string();
let corrected = apply_terminology_correction(&raw_text, &rules);
let display_text = corrected.into_owned();

let update = TranscriptUpdate {
    text: display_text,
    raw_text: Some(raw_text),
    corrections_applied: Some(count),
    // ... 其余不变
};
```

### 10.3 前端双轨聚合实现

**实现位置**：`TranscriptContext.tsx`（非 service 层）。

**内存优化**：使用 `useRef` 维护 mutable buffer，仅在渲染间隔同步到 State，避免长会议场景下数千次 React State 不可变数组重组的 GC 压力。

```typescript
// TranscriptContext.tsx
const rawBufferRef = useRef<string[]>([]);
const displayBufferRef = useRef<string[]>([]);

// 在 transcriptService 的 listen 回调中追加：
rawBufferRef.current.push(event.payload.raw_text ?? event.payload.text);
displayBufferRef.current.push(event.payload.text);

// 间隔同步（例如每 500ms 或每 10 个 chunk）
syncToState();
```

---

## 11. 迁移策略与受影响模块

### 11.1 现有命令兼容

| 命令 | 处理 |
|------|------|
| `api_save_transcript`（旧） | **保留兼容期**（Phase 1A-2），无新参数时 raw/hash 留空 |
| `save_transcript_with_terminology`（新） | Phase 1A 新增，接收 raw + hash |
| `api_get_meeting` | 扩展返回 raw 字段（可选） |
| transcript history | 旧数据 raw 为空，前端优雅降级 |

### 11.2 双轨扩展受影响模块

> **注意**：双轨改造是**恢复链路级别**的改造包，非简单"加两个字段"。涉及运行中缓冲、本地容灾缓存、重启恢复、停止保存、历史再入库五条路径。

| 模块 | 文件 | 改造内容 |
|------|------|------|
| **TranscriptUpdate 事件** | `worker.rs` | + raw_text / corrections_applied |
| **事件监听** | `transcriptService.ts` | 解析双轨 payload |
| **状态管理 + 双轨 buffer** | `TranscriptContext.tsx` | 维护 raw + display buffer（useRef + 间隔同步） |
| **本地容灾缓存** | `indexedDBService.ts` | saveTranscript 存储双轨 |
| **崩溃恢复** | `useTranscriptRecovery.ts` | 从 IndexedDB 恢复双轨 |
| **停止保存** | `useRecordingStop.ts` | 编排双轨保存 + L3 入队 |
| **IPC 封装** | `storageService.ts` | saveMeeting 适配新命令 |
| **命令层** | `database/commands.rs` | 新增 save_transcript_with_terminology |
| **DB 模型** | `database/models.rs` | Transcript + raw / hash 字段 |
| **meeting details** | `app/_components/` | raw 查看入口（默认隐藏，设置开关） |
| **摘要生成** | `summary/` | 依赖 normalized（V1 不变） |

### 11.3 分阶段迁移

| 阶段 | 内容 |
|------|------|
| Phase 1A | 新命令 + 新 DB 字段 + TranscriptUpdate 扩展 + 前端双轨 buffer |
| Phase 1A | 旧命令保留，无 raw 参数时 raw 留空 |
| Phase 1B-2 | 稳定后旧调用路径逐步迁移 |
| Phase 3 | 评估废弃旧命令 |

---

## 12. 数据库设计

### 12.1 新建表：`terminology`

```sql
CREATE TABLE IF NOT EXISTS terminology (
    id               TEXT PRIMARY KEY,
    original         TEXT NOT NULL,
    replacement      TEXT NOT NULL,
    language         TEXT NOT NULL DEFAULT 'auto',
    case_sensitive   INTEGER NOT NULL DEFAULT 0,
    whole_word       INTEGER NOT NULL DEFAULT 1,
    enabled          INTEGER NOT NULL DEFAULT 1,
    priority         TEXT NOT NULL DEFAULT 'normal',
    category         TEXT NOT NULL DEFAULT 'general',
    description      TEXT,
    source_type      TEXT NOT NULL DEFAULT 'manual',  -- 'preset' | 'imported' | 'manual'
    package_id       TEXT,
    package_name     TEXT,
    import_batch_id  TEXT,
    created_at       TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at       TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(original, language)
);
```

### 12.2 新建表：`l3_correction_jobs`（V4.0 新增）

```sql
-- L3 任务队列状态。一场会议最多一条 active job。
CREATE TABLE IF NOT EXISTS l3_correction_jobs (
    id              TEXT PRIMARY KEY,
    meeting_id      TEXT NOT NULL UNIQUE,
    status          TEXT NOT NULL DEFAULT 'queued',
    -- 'queued' | 'running' | 'done' | 'failed' | 'timeout'
    error_detail    TEXT,
    attempt_count   INTEGER NOT NULL DEFAULT 1,
    created_at      TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at      TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (meeting_id) REFERENCES meetings(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_l3_jobs_status ON l3_correction_jobs(status);
CREATE INDEX IF NOT EXISTS idx_l3_jobs_meeting ON l3_correction_jobs(meeting_id);
```

### 12.3 新建表：`transcript_corrections`

```sql
-- L3 校正建议。一条记录 = 一条建议。审核状态独立于任务状态。
CREATE TABLE IF NOT EXISTS transcript_corrections (
    id                  TEXT PRIMARY KEY,
    meeting_id          TEXT NOT NULL,
    job_id              TEXT NOT NULL,
    original_span       TEXT NOT NULL,
    suggested_text      TEXT NOT NULL,
    occurrences_json    TEXT,
    language            TEXT,
    correction_type     TEXT NOT NULL DEFAULT 'llm',
    reason              TEXT,
    source_snapshot_hash TEXT,
    status              TEXT NOT NULL DEFAULT 'pending',
    -- 'pending' | 'accepted' | 'rejected' | 'obsolete'
    reviewed_by         TEXT,
    reviewed_at         TEXT,
    created_at          TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (meeting_id) REFERENCES meetings(id) ON DELETE CASCADE,
    FOREIGN KEY (job_id) REFERENCES l3_correction_jobs(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_corrections_meeting ON transcript_corrections(meeting_id);
CREATE INDEX IF NOT EXISTS idx_corrections_status ON transcript_corrections(status);
```

### 12.4 现有表扩展（幂等迁移）

```sql
-- transcripts 表新增:
--   raw_transcript TEXT
--   terminology_snapshot_hash TEXT
--   l1_prompt_snapshot TEXT

-- settings 表新增:
--   terminology_enabled INTEGER DEFAULT 1
--   initial_prompt_enabled INTEGER DEFAULT 1
--   llm_correction_enabled INTEGER DEFAULT 1
--   llm_correction_auto_accept INTEGER DEFAULT 0  -- V1 UI 不暴露
```

所有 ALTER TABLE 通过 `add_column_if_not_exists()` 幂等执行。

### 12.5 数据模型（Rust）

```rust
// L3 任务
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct L3CorrectionJob {
    pub id: String,
    pub meeting_id: String,
    pub status: String,        // queued | running | done | failed | timeout
    pub error_detail: Option<String>,
    pub attempt_count: i64,
    pub created_at: String,
    pub updated_at: String,
}

// L3 建议
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct TranscriptCorrection {
    pub id: String,
    pub meeting_id: String,
    pub job_id: String,
    pub original_span: String,
    pub suggested_text: String,
    pub occurrences_json: Option<String>,
    pub language: Option<String>,
    pub correction_type: String,
    pub reason: Option<String>,
    pub source_snapshot_hash: Option<String>,
    pub status: String,        // pending | accepted | rejected | obsolete
    pub reviewed_by: Option<String>,
    pub reviewed_at: Option<String>,
    pub created_at: String,
}
```

---

## 13. 后端实现规范

### 13.1 模块组织结构

```
frontend/src-tauri/src/
├── terminology/
│   ├── mod.rs
│   ├── cache.rs                     # 统一缓存 + snapshot_hash
│   ├── commands.rs                  # Tauri 命令
│   ├── corrector.rs                 # 混合引擎 + RuleBuildError
│   ├── queue.rs                     # L3 持久化队列 + 恢复
│   └── snapshot.rs                  # 术语快照哈希
│
├── whisper_engine/whisper_engine.rs # L1 集成
├── audio/transcription/worker.rs   # L2 + TranscriptUpdate 扩展
├── summary/llm_client.rs           # L3: suggest_corrections()
│
├── database/
│   ├── models.rs
│   ├── repositories/
│   │   └── terminology.rs
│   └── setup.rs                    # 幂等迁移 + L3 任务恢复
│
├── lib.rs
└── migrations/
    └── 20260427000000_add_terminology.sql
```

### 13.2 Tauri 命令

```rust
// 术语 CRUD
terminology::commands::get_terminology_list,
terminology::commands::save_terminology_entry,
terminology::commands::delete_terminology_entry,
terminology::commands::import_terminology_csv,
terminology::commands::export_terminology_csv,

// 术语包管理
terminology::commands::enable_package,
terminology::commands::disable_package,
terminology::commands::rollback_import_batch,

// 缓存与快照
terminology::commands::refresh_all_terminology_caches,
terminology::commands::compute_terminology_snapshot_hash,

// L3 持久化队列
terminology::commands::run_llm_terminology_correction,   // 入队→写 l3_correction_jobs
terminology::commands::get_l3_job_status,                 // 查询任务状态
terminology::commands::retry_l3_correction,               // 超时/失败重试

// L3 建议审核
terminology::commands::get_corrections_for_meeting,
terminology::commands::accept_correction,
terminology::commands::accept_correction_for_term,
terminology::commands::reject_correction,

// 保存
database::commands::save_transcript_with_terminology,

// 设置
terminology::commands::get_terminology_settings,
terminology::commands::set_terminology_settings,
```

### 13.3 启动初始化

```rust
let db = app.state::<state::AppState>().db_manager.clone();
tauri::async_runtime::spawn(async move {
    // 1. 术语缓存
    if let Err(e) = terminology::cache::refresh_all_caches(&db).await {
        log::warn!("Terminology caches init failed: {}", e);
    }
    // 2. L3 任务恢复：查询 l3_correction_jobs WHERE status IN ('queued','running')
    if let Err(e) = terminology::queue::recover_pending_tasks(&db).await {
        log::warn!("L3 task recovery failed: {}", e);
    }
});
```

---

## 14. 前端实现规范

### 14.1 新增组件

| 组件 | 路径 | 说明 |
|------|------|------|
| `TerminologyManager` | `components/TerminologyManager/index.tsx` | 术语管理（含包筛选 + 失效规则标记） |
| `TerminologyImportDialog` | `components/TerminologyManager/ImportDialog.tsx` | CSV 导入（冲突预览） |
| `CorrectionDiffView` | `components/CorrectionDiff/DiffView.tsx` | 按术语聚类 + 风险分级 |
| `L3QueueStatus` | `components/CorrectionDiff/QueueStatus.tsx` | L3 任务状态 + 重试按钮 |
| `RawTranscriptToggle` | `app/_components/RawTranscriptToggle.tsx` | raw 查看开关（默认隐藏） |

### 14.2 TypeScript 类型

```typescript
export interface TranscriptUpdate {
  // 现有
  text: string; timestamp: string; source: string; sequence_id: number;
  chunk_start_time: number; is_partial: boolean; confidence: number;
  audio_start_time: number; audio_end_time: number;
  // 新增
  raw_text?: string;
  corrections_applied?: number;
}

export interface CharRange { start: number; end: number; }  // Unicode scalar value

export type L3JobStatus = 'queued' | 'running' | 'done' | 'failed' | 'timeout';
export type CorrectionStatus = 'pending' | 'accepted' | 'rejected' | 'obsolete';
```

---

## 15. 配置与存储

`auto_accept` 字段保留但 V1 UI 不暴露。CSV 导入：MVP 仅 UTF-8 BOM。预置术语包通过 `package_id` 级管理。


### 15.1 配置项

| 配置项 | 默认值 | V1 UI 暴露 | 说明 |
|--------|:---:|:---:|------|
| `terminology_enabled` | 1 | ✅ | 总开关（含 L2） |
| `initial_prompt_enabled` | 1 | ✅ | L1 开关（仅对 Whisper） |
| `llm_correction_enabled` | 1 | ✅ | L3 建议生成开关 |
| `llm_correction_auto_accept` | **0** | **❌** | V1 不暴露，强制为 false |

### 15.2 CSV 导入/导出

**编码支持分阶段**：
- MVP：仅 UTF-8 BOM
- Phase 1B：Shift-JIS 自动检测（前端预览前 5 行）

**冲突键**：`(original, language)` 联合唯一。导入时 upsert。导入预览显示："将新增 X 条，覆盖 Y 条"。

**导入批次追踪**：每次 CSV 导入生成 `import_batch_id`，可在 `terminology` 表中按批次查询和回滚。

### 15.3 内置预置术语包

```
preset_terminology/
├── chemical_ja.csv       # source_type=preset, package_id="meetily-chemical-ja-v1"
├── chemical_zh.csv
├── chemical_en.csv
├── ghs_codes.csv         # package_id="meetily-ghs-codes-v1"
└── README.md
```

预置包可通过 `package_id` 级批量启用/禁用。

---

## 16. 硬件要求与降级策略

V1 降级策略为推荐策略（用户手动配置）。自动硬件检测后续版本完善。

### 16.1 最低配置

| 组件 | 最低配置 | 推荐配置 |
|------|----------|----------|
| RAM | 8GB (L1+L2) | 16GB+ (含 L3) |
| 磁盘 | 500MB | 3-5GB (含 LLM 模型) |

### 16.2 降级策略（推荐策略，用户手动配置）

| 条件 | L1 | L2 | L3 |
|------|:---:|:---:|:---:|
| RAM < 8GB | ❌ | ✅ | ❌ |
| RAM 8-12GB | ✅ | ✅ | 尝试 3B |
| RAM > 12GB | ✅ | ✅ | ✅ (默认 7B) |
| 电池供电 | ✅ | ✅ | ❌ (可手动开启) |

> 自动硬件检测和电源状态感知在后续版本中完善。第一版以设置页面中的推荐策略形式呈现。

---

## 17. 实施计划与 MVP 策略

| 阶段 | 内容 | 工作量 |
|------|------|:---:|
| **Phase 0** | 基线测量 + 5 项技术验证（含 tokenizer + fancy-regex） | 1-2 人天 |
| **Phase 1A (MVP)** | L2 混合引擎 + DB（含 l3_correction_jobs 拆分）+ TranscriptUpdate 双轨 + 基础 UI + 迁移 | 3-4 人天 |
| **Phase 1B** | L1（API 验证后）+ CSV 导入（UTF-8 BOM）+ token 计数 | 2 人天 |
| **Phase 2** | L3 持久化队列 + 恢复 + 聚类 UI + 风险分级 | 3-4 人天 |
| **Phase 3** | 完善：Shift-JIS、审计报告、摘要重生成 | 2-3 人天 |

**总计：11-15 人天**

> **注意**：Phase 1A 中的双轨改造是**恢复链路级别**的改造包（涉及 11 个模块，参见第 11.2 节），非前端简单改动。排期时应为前端部分预留足够的集成测试时间。

### 17.1 MVP（Phase 1A）核心范围

1. L2 混合引擎 + `worker.rs` 集成
2. `terminology` 表 + `l3_correction_jobs` 表 + `transcript_corrections` 表 + 幂等迁移
3. `raw_transcript` + `terminology_snapshot_hash` + `l1_prompt_snapshot` 字段
4. TranscriptUpdate 双轨扩展 + 前端双轨 buffer（TranscriptContext）
5. 基础术语管理 UI（含失效规则标记 + 包级禁用）
6. 预置术语包（精简版 50 条）
7. 旧 `api_save_transcript` 兼容保留

---

## 18. 测试策略

### 18.1 关键测试用例

```
✅ TC-01: 日语连续术语（验证无消费效应）
✅ TC-02: 日语行首/行尾正确匹配
✅ TC-03: 中文短术语不命中长术语
✅ TC-04: 中文同音字变体正确替换
✅ TC-05: 日语浊音变体仅替换错误项
✅ TC-06: 混合语言不误替换
✅ TC-07: fancy-regex 编译失败 → UI 标记"已失效"
✅ TC-08: Cow<str> 无匹配时不分配
✅ TC-09: TranscriptUpdate raw_text 和 text 分别正确
✅ TC-10: L3 任务写入 l3_correction_jobs (status='queued')
✅ TC-11: L3 任务执行 → status 流转: queued→running→done
✅ TC-12: L3 建议写入 transcript_corrections (status='pending')
✅ TC-13: L3 并发 2 任务 → Semaphore 串行，不 OOM
✅ TC-14: App 崩溃重启 → l3_correction_jobs 中 queued/running 任务被恢复
✅ TC-15: 同一 meeting 不可重复入队（幂等）
✅ TC-16: L3 超时 → status='timeout' → 前端重试按钮可见
✅ TC-17: L3 短 original_span (<3字符) → 禁用接受全部
✅ TC-18: L3 高频片段(>全文1/3) → 禁用接受全部
✅ TC-19: L3 高风险术语(ghs/cas/un) → 不提供接受全部按钮
✅ TC-20: 术语快照哈希修改后正确变化
✅ TC-21: 幂等迁移重复执行不报错
✅ TC-22: Offset Rust char_indices ↔ JS [...str] 一致
✅ TC-23: 前端 useRef buffer 2 小时会议无内存抖动
✅ TC-24: raw_transcript 默认隐藏，设置开关后可见
✅ TC-25: 旧 api_save_transcript 兼容运行，raw 字段留空
```

### 18.2 验收标准

**核心功能**：
- [ ] raw_transcript 强制保留且不可变
- [ ] `terminology_snapshot_hash` 随术语表变更而正确变化
- [ ] L2 日语连续术语无消费效应（TC-01）
- [ ] fancy-regex 编译失败规则 UI 标记"已失效"

**若 fancy-regex Phase 0 验证失败**：
- [ ] 日/中全词匹配相关验收项从 Phase 1A 移除
- [ ] UI 标注"日语/中文全词匹配暂不可用"

**L3 数据模型**：
- [ ] `l3_correction_jobs` 与 `transcript_corrections` 独立存储
- [ ] 任务状态和建议审核状态使用各自独立的状态机
- [ ] 持久化队列：入队即写 DB、Semaphore(1)、重启恢复、幂等、超时+重试

**L3 风险控制**：
- [ ] 短子串 (<3字符) 禁用接受全部
- [ ] 高频片段 (>全文1/3) 禁用接受全部
- [ ] 高风险术语 (ghs/cas/un) 不提供接受全部按钮
- [ ] 全文 N 处提示文案正确显示

**迁移与兼容**：
- [ ] 旧 `api_save_transcript` 兼容运行
- [ ] 双轨扩展覆盖 IndexedDB / transcript history / reload recovery
- [ ] raw_transcript 默认隐藏，设置开关后可见

**性能**：
- [ ] 前端 useRef buffer 2 小时会议无内存抖动
- [ ] 幂等迁移重复执行不报错
- [ ] 校正审计日志完整可追溯

---

## 19. 风险与应对

| 风险 | 影响 | 概率 | 应对 |
|------|------|:---:|------|
| whisper-rs 未暴露 `set_initial_prompt` | L1 无法实现 | 中 | Phase 0 验证，废弃 L1 |
| `fancy-regex` 不可用 | L2 日/中全词不可用 | 低 | Phase 0 预研，降级子串匹配，验收标准同步移除 |
| Whisper tokenizer 与 `cl100k_base` 不一致 | L1 token 计数不准 | 中 | Phase 0 验证，回退保守估算 |
| L3 全文同串在不同上下文误匹配 | 部分不需要修正处被建议修正 | 中 | 全文 N 处提示 + 短子串/高频/高风险三类强制约束 |
| L3 任务持久化未正确实现 | App 崩溃任务丢失 | 低 | `l3_correction_jobs` 独立表 + 入队即写 DB + TC-14 |
| 双轨恢复链路改造规模被低估 | Phase 1A 前端排期不足 | 中 | 11.2 节显式标注为"恢复链路级别改造包"，预留集成测试时间 |
| 危化品合规风险 | 错误校正导致安全信息错误 | **高** | raw 不可变 + L3 仅建议 + auto_accept 隐藏 + 审计日志 |

---

## 20. 合规与法务审查

| 节点 | 时机 | 内容 |
|------|------|------|
| Gate A | Phase 1A 后 | raw 保留 + 默认隐藏策略 + L2 审计兼容 |
| Gate B | Phase 2 上线前 | L3 建议模式（全文 N 处提示 + 高风险强制逐条） |
| Gate C | Phase 3 后 | 审计报告格式 |

核心原则：raw 不可变、L2 可审计、L3 仅建议、auto_accept 隐藏。

---

## 21. 回滚与功能淘汰

- 总开关：`terminology_enabled = 0`
- L1 / L3 可独立关闭
- 术语包 `import_batch_id` 级回滚
- L3 任务：超时/失败可重试（`attempt_count` + 1）
- raw_transcript 永远不变（最终回滚锚点）
- 旧 `api_save_transcript` 兼容期保留

---

## 附录 A：成功指标体系

| 指标 | 方法 | 目标 |
|------|------|:---:|
| L2 误替换率 | 人工审核 100 条 | < 1% |
| L3 建议接受率 | 用户接受/拒绝比 | > 70% |
| 用户复核时长 | 对比有无术语校正 | 下降 > 30% |
| 术语准确率提升 | 与 Phase 0 基线对比 | 基于基线设定 |
| L2 连续术语遗漏率 | TC-01 | 0% |
| L3 任务丢失率 | 强制崩溃测试 | 0% |

---

## 附录 B：V3.3 → V4.0 变更摘要

| 变更项 | V3.3 | V4.0 | 触发来源 |
|--------|------|------|------|
| **L3 数据模型拆分** | `transcript_corrections` 一张表，`status` 同时承载任务状态和建议审核状态 | **拆为两张表**：`l3_correction_jobs`（任务状态：queued/running/done/failed/timeout）+ `transcript_corrections`（建议审核：pending/accepted/rejected/obsolete） | GPT P0-1 |
| **L3 状态机** | 单一字段多语义 | **两个独立状态机**，各自完整的生命周期和查询语义 | GPT P0-1 |
| **L3 高频片段控制** | 仅 <3 字符 + 高风险术语限制 | **追加**：命中次数 > 全文字符数 1/3 → 禁用接受全部 | GPT P1-2 |
| **raw 可见性** | 未定义产品边界 | **明确**：默认隐藏，设置中"显示原始转录"开关控制，导出时双列 | GPT P2-2 |
| **fancy-regex 降级联动** | 只提降级方向 | **明确**：若不可用则 Phase 1A 验收标准移除日/中 whole-word 承诺，UI 标注"暂不可用" | GPT P1-3 |
| **双轨改造工作量** | 提及受影响模块 | **显式标注**为"恢复链路级别改造包"，排期时预留集成测试时间 | GPT P1-1 |
| **useRef buffer 落点** | 示例在事件监听 | **明确落点**：`TranscriptContext.tsx` | GPT P2-1 |
| **DB 模型** | `TranscriptCorrection` 单 struct | **新增 `L3CorrectionJob` struct**，`TranscriptCorrection` 增加 `job_id` 字段 | GPT P0-1 |
| **Tauri 命令** | 队列状态查询混杂 | **拆分**：`get_l3_job_status`（查任务）+ `get_corrections_for_meeting`（查建议） | GPT P0-1 |
| **验收标准** | 未区分降级场景 | **新增** fancy-regex 降级场景的验收边界 | GPT P1-3 |

---

> **ドキュメントメンテナンス / 文档维护**：本 PRD V4.0 基于 V3.3 的 GPT-4 技术审查后修订。核心修正为拆分 L3 任务状态与建议审核状态的数据模型（`l3_correction_jobs` + `transcript_corrections`），并补充了高频片段控制、raw 可见性边界、fancy-regex 降级联动和双轨改造工作量显式标注。本版本可交付研发进入技术详细设计和 POC 开发。
