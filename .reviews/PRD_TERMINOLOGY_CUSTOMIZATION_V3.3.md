# PRD：转录专业术语定制化功能

> **文档状态**：V3.3（综合 Gemini + GPT 对 V3.2 的审查后修订）
> **创建日期**：2026-04-27
> **修订日期**：2026-04-27
> **关联项目**：Meetily v0.3.0
> **目标行业**：危险化学品制造业（日系企业）
> **支持语言**：日本語 / 中文 / English
> **核心目标**：使 Meetily 转录引擎支持用户自定义专业术语词库，通过三级管道纠正语音识别（STT）产生的术语识别错误。原始 STT 输出强制保留，校正全程可追溯、可审计。
>
> **本轮评级**：已具备进入技术详细设计和 POC 开发的条件。剩余问题属于实现级精度，已在各章节标注。

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

1. **跨语言术语混乱**：模型在日/中/英切换时误判语言
2. **化学物质名称识别率极低**：IUPAC 命名和片假名化学名不在通用训练语料中
3. **安全编码格式特殊**：CAS RN、UN No.、GHS 代码在语音转写中极易出错
4. **片假名/汉字/罗马字混合**：日语化学术语同时使用多套文字系统

### 1.3 目标

- **强制保留原始 STT 输出**，校正结果分层存储，证据链完整
- **多语言支持**：日语、中文、英语
- **三级校正管道**：模型内 `initial_prompt` → 正则精确替换 → LLM 校正**建议**
- **术语表可溯源**：术语来源可查（预置/导入/手动），校正时记录术语快照版本
- 转录时实时应用 L1+L2，录音停止后通过**持久化串行队列**异步触发 L3
- L3 校正默认仅建议，需用户确认后生效；第一版仅纠错模式

---

## 2. 核心问题分析

### 2.1 STT 输出错误规律

#### 2.1.1 日语

| 错误类型 | 示例 | 根因 |
|----------|------|------|
| 片假名 → 汉字误转 | `ポリウレタン` → `保利売れたん` | token 覆盖不足 |
| 長音「ー」/ 促音「っ」丢失 | `メチルエチルケトン` → 长音被省略 | whisper.cpp 对特殊假名 token 不敏感 |
| 浊音/半浊音混淆 | `ポリ` → `ボリ` / `ホリ` | 噪声下辅音歧义 |
| 拗音分割 | `メチル` → `メ チル` | 拗音 token 被拆分 |

#### 2.1.2 中文

| 错误类型 | 示例 | 根因 |
|----------|------|------|
| 同音字替换 | `甲苯二异氰酸酯` → `甲本二亿情酸纸` | 化学术语汉字组合生僻 |
| 数字+字母错位 | `H225` → `H二二五` | 中英混合 token 不稳定 |
| 多音字误读 | `重铬酸钾` → `重各酸钾` | 化学用字多音 |

#### 2.1.3 英语（化学语境）

| 错误类型 | 示例 | 根因 |
|----------|------|------|
| IUPAC 命名拆分 | `2,4,6-Trinitrotoluene` → `two four six tri nitro toluene` | 数字序列 token 化不熟悉 |
| CAS 编号格式 | `CAS 108-88-3` → `k Ass one o eight...` | 连字符边界错误 |
| GHS 代码 | `H301` → `H three hundred one` | H+数字不在常见 token 表 |

#### 2.1.4 跨语言混合

| 错误类型 | 示例 | 根因 |
|----------|------|------|
| 日语→中文误转 | `この物質は…` → `这个物质是…` | 语言切换点失误 |
| 代码混入自然语言 | `UN 1203` → `うん いちにーぜろさん` | 代码被当作假名发音 |

### 2.2 设计决策：三级校正管道

| 维度 | L1：initial_prompt | L2：正则实时 | L3：LLM 异步建议 |
|------|:---|:---|:---|
| **执行时机** | Whisper 推理时 | 每次转录输出后 | 录音停止后（持久化串行队列） |
| **支持引擎** | 仅 Whisper | Whisper / Parakeet / Provider | 所有引擎 |
| **覆盖率（估计，以 Phase 0 实测为准）** | ~15-30% | ~50-60% | ~10-20% |
| **确定性** | 否 | 是 | 否 |

---

## 3. 总体架构设计

### 3.1 系统架构图

```
┌──────────────────────────────────────────────────────────────────────────────┐
│                          前端 (Next.js + React)                               │
│                                                                              │
│  TranscriptContext (双轨聚合)         useRecordingStop (停止+保存编排)           │
│  ├─ displayTranscripts (L2后)        ├─ storageService.saveMeeting()          │
│  └─ rawTranscripts (L0)             └─ 触发 L3 任务入队                       │
│                                                                              │
│  storageService            transcriptService              indexedDBService    │
│  ├─ saveMeeting (新版)     ├─ listen('transcript-update') ├─ saveTranscript   │
│  └─ getMeeting             └─ 接收双轨 payload            └─ 本地恢复缓存        │
│                                                                              │
│  TerminologyManager                     CorrectionDiffView                   │
│  ├─ 术语 CRUD + 包级管理                 ├─ 按术语聚类                          │
│  └─ 失效规则标记 (编译错误)              └─ 全文 N 处提示 + 逐条展开确认            │
└──────────────────────────────────┬───────────────────────────────────────────┘
                                   │ invoke() / listen()
┌──────────────────────────────────┴───────────────────────────────────────────┐
│                      Tauri IPC 层 (Rust Backend)                              │
│                                                                              │
│  terminology/                            数据库层                              │
│  ├─ cache.rs: 原子刷新 + snapshot_hash      ├─ terminology 表 (含 package 字段)  │
│  ├─ corrector.rs: 混合引擎 (regex/fancy)    ├─ transcript_corrections 表        │
│  ├─ queue.rs: 持久化串行队列 + 恢复          ├─ transcripts 表 (raw + hash 字段)  │
│  └─ commands.rs: Tauri 命令注册             └─ 幂等迁移                           │
│                                                                              │
│  转录管道:  whisper_engine (L1) → worker.rs (L2) → emit transcript-update     │
│  后处理:     llm_client (L3) → 持久化队列 → 建议写入 DB → emit corrections-ready │
└──────────────────────────────────────────────────────────────────────────────┘
```

---

## 4. 审计与证据链设计

### 4.1 四层文本模型

| 层 | 存储位置 | 来源 | 可变性 | 用途 |
|---|---------|------|:---:|------|
| **L0** | `transcripts.raw_transcript` | STT 引擎直接输出（L1 后、L2 前） | **不可变** | 审计锚点 |
| **L1+L2** | `transcripts.transcript` | raw → L2 正则替换 | 可更新 | 实时显示、日常使用 |
| **L3 建议** | `transcript_corrections` | LLM 异步生成 | status 可变 | 深度校正候选 |
| **Final** | `transcripts.transcript`（更新后） | 用户确认的合并结果 | 接受后写入 | 归档导出 |

### 4.2 术语快照版本

每次录音停止保存时，记录当时术语表的 SHA-256 摘要到 `transcripts.terminology_snapshot_hash`。

**能力边界说明**：

| 版本 | 能力 | 说明 |
|------|------|------|
| **V1（当前）** | `terminology_snapshot_hash` | 快照指纹：可证明"当时规则集与现在不同" |
| **Future** | 术语表历史快照 | 完整证据：可重建"当时有哪些术语条目" |

V1 的 `snapshot_hash` 是审计索引，不是完整证据本体。V1 不要求保留术语表的完整历史版本。

### 4.3 审计追溯能力

| 问题 | 答案来源 |
|------|----------|
| 原始模型输出了什么？ | `raw_transcript`（不可变） |
| L2 用了哪套规则？ | `terminology_snapshot_hash`（可证明与当前不同） |
| L3 做了哪些建议？ | `transcript_corrections` 表 |
| 用户接受了哪些？ | `status = 'accepted'` + `reviewed_by` + `reviewed_at` |

---

## 5. Phase 0：基线测量与方案验证

### 5.1 验证清单

| # | 验证项 | 方法 | 阻塞级别 | 若失败的回退 |
|---|--------|------|:---:|------|
| 1 | whisper-rs 0.13.x 是否暴露 `set_initial_prompt` | 代码审查 FullParams 结构体 | **P0** | 废弃 L1，仅 L2+L3 |
| 2 | `fancy-regex` 在当前 Rust 版本下可编译且通过 TC-01~TC-06 | `cargo add fancy-regex && cargo test` | **P0** | 日/中全词匹配降级为子串匹配 |
| 3 | **Whisper 实际使用的 tokenizer 确认** | 查阅 whisper.cpp 源码或文档，确认 tokenizer 类型（当前候选：`cl100k_base`，但**未经验证**）。若确认则用 `tiktoken-rs` 精确计数；若无法确认则回退为保守估算 | **P1** | 保守估算（日语 0.8×, 中文 0.9×, 英语 0.4× char count） |
| 4 | `regex` 1.11 对 `\p{Katakana}` / `\p{Han}` 的实际行为 | 运行 5 个边界测试用例 | **P1** | — |
| 5 | 当前 `transcript-update` 监听链路（transcriptService → TranscriptContext → useRecordingStop → storageService） | 追踪代码路径，确认双轨扩展的插入点 | **P1** | — |

### 5.2 预估工作量

1-2 人天。

---

## 6. 第一级：Whisper initial_prompt 软引导

### 6.1 实现原理

Whisper.cpp 的 `initial_prompt` 参数通过 decoder cross-attention 偏置 token 分布。软引导，非确定性。~224 token 硬限制。仅对 Whisper 引擎生效。

### 6.2 Token 计数方案

> **重要**：Whisper 实际使用的 tokenizer 类型需在 Phase 0 中确认。以下为候选方案，非既定事实。

**方案 A（优先，需 Phase 0 验证）**：若确认 Whisper 使用 `cl100k_base` tokenizer，使用 `tiktoken-rs` 精确 BPE 计数。

**方案 B（回退）**：若 tokenizer 无法确认，使用保守估算。估算偏保守（偏向高估），宁可少注入几条也不冒险超出 224 token 限制。

```rust
fn estimate_tokens_fallback(text: &str, lang: &str) -> usize {
    match lang {
        "ja" => (text.chars().count() as f64 * 0.8) as usize,  // 偏保守
        "zh" => (text.chars().count() as f64 * 0.9) as usize,
        _    => (text.chars().count() as f64 * 0.4) as usize,
    }
}
```

**若两种方案均不可用**：前端提供手动输入 token 上限的配置项，用户自行控制注入术语数量。

### 6.3 Token 截断策略

按 `priority=high` 过滤 → 按 `updated_at` 降序（最近更新的优先保留）→ 贪心拼接 → 超限则截断。被排除的术语 ID 列表通过 event 通知前端。

**前端反馈**："L1 Prompt: 25/38 条高优先级术语已注入。13 条因 token 限制未包含。[查看被排除的术语]"

### 6.4 L1 Prompt 审计存储

- 运行日志仅记录：`"L1 prompt injected: 25 terms, ~187 tokens, 13 excluded"`
- 完整 prompt 快照写入 `transcripts.l1_prompt_snapshot` 字段（与 transcript 同生命周期、同访问控制）

### 6.5 代码集成位置

`whisper_engine.rs` 的 `FullParams` 构造完成后、`state.full()` 之前。仅当 Phase 0 验证 API 可用时集成。

---

## 7. 第二级：正则实时校正通道

### 7.1 混合引擎方案

| 规则类型 | 引擎 | 原因 |
|----------|------|------|
| `whole_word = false`（所有语言） | `regex` | 纯子串匹配，DFA 最快 |
| `whole_word = true`，语言 = `en` | `regex` + `\b` | 标准词边界，DFA 支持 |
| `whole_word = true`，语言 = `ja`/`zh` | **`fancy-regex`** | 需要 look-around 模拟日/中词边界 |

`fancy-regex` 底层优先 DFA，仅在遇到 look-around 断言时回退回溯引擎。Phase 0 需验证其在项目 Rust 版本下可编译且通过日语连续术语测试。

### 7.2 模式构建（含编译失败优雅降级）

```rust
use fancy_regex::Regex as FancyRegex;
use regex::Regex;

enum CompiledRule {
    Standard { regex: Regex, replacement: String, original_len: usize },
    Fancy   { regex: FancyRegex, replacement: String, original_len: usize },
}

enum RuleBuildError {
    CompileFailed { original: String, reason: String },
}

fn build_term_rule(entry: &TerminologyEntry) -> Result<CompiledRule, RuleBuildError> {
    let escaped = regex::escape(&entry.original);
    let original_len = entry.original.chars().count();

    if !entry.whole_word {
        let pattern = if entry.case_sensitive { escaped } else { format!("(?i){}", escaped) };
        let re = Regex::new(&pattern)
            .map_err(|e| RuleBuildError::CompileFailed {
                original: entry.original.clone(),
                reason: e.to_string(),
            })?;
        return Ok(CompiledRule::Standard { regex: re, replacement: entry.replacement.clone(), original_len });
    }

    match entry.language.as_str() {
        "ja" | "zh" => {
            let boundary = match entry.language.as_str() {
                "ja" => r"[\p{Han}\p{Hiragana}\p{Katakana}ー]",
                _    => r"\p{Han}",
            };
            let pattern = format!(
                r"(?<!{b}){e}(?!{b})",
                b = boundary, e = escaped
            );
            let pattern = if entry.case_sensitive { pattern } else { format!("(?i){}", pattern) };
            let re = FancyRegex::new(&pattern)
                .map_err(|e| RuleBuildError::CompileFailed {
                    original: entry.original.clone(),
                    reason: e.to_string(),
                })?;
            Ok(CompiledRule::Fancy { regex: re, replacement: entry.replacement.clone(), original_len })
        }
        _ => {
            let pattern = if entry.case_sensitive {
                format!(r"\b{}\b", escaped)
            } else {
                format!(r"(?i)\b{}\b", escaped)
            };
            let re = Regex::new(&pattern)
                .map_err(|e| RuleBuildError::CompileFailed {
                    original: entry.original.clone(),
                    reason: e.to_string(),
                })?;
            Ok(CompiledRule::Standard { regex: re, replacement: entry.replacement.clone(), original_len })
        }
    }
}
```

**编译失败的 UI 反馈**：当 `build_term_rule` 返回 `Err` 时，该术语条目在 UI 中标记为"⚠ 规则语法错误/已失效"，状态设置为 `enabled: false`。前端在术语管理页面用红色边框或警告图标标识，并提供错误信息查看入口。避免用户困惑"为什么这个词没有被替换"。

### 7.3 核心替换逻辑

参见 V3.2 7.3 节，无变化。

### 7.4 关键测试用例

```
TC-01: 日语连续术语（验证无消费效应）
  输入:  "ポリウレタン、過酸化物を含む"
  规则:  "かさんかぶつ" → "過酸化物" (ja, whole_word=true, fancy-regex)
  期望:  两个术语均被正确处理，逗号不会阻止第二个术语匹配

TC-02: 日语行首/行尾正确匹配
TC-03: 中文短术语不命中长术语（"甲苯"不命中"甲苯二异氰酸酯"）
TC-04: 中文同音字变体正确替换
TC-05: 日语浊音变体 — 仅替换错误变体
TC-06: 混合语言 — 英语术语不误替换
```

### 7.5 性能基准

> 预估值，以 Phase 1 实测（术语 50/200/500 × 语言 × chunk 长 × 设备）为准。熔断：> 100ms → 仅 high 规则 + UI 警告。

### 7.6 L2 可关闭性

- 全局：`terminology_enabled = 0`
- 按术语包：`UPDATE terminology SET enabled=0 WHERE package_id=?`
- 调试模式：关闭后对比 raw 和 L2

---

## 8. 第三级：LLM 深度校正建议

### 8.1 模式定义：仅纠错，不扩写

| 操作 | 纠错模式（V1） | 扩写模式（Future） |
|------|:---:|:---:|
| 修正 STT 识别错误 | ✅ | ✅ |
| 统一术语表记（IUPAC 优先） | ✅ | ✅ |
| 将 `TDI` 展开为 `トルエンジイソシアネート (TDI)` | ❌ | ✅ |
| 添加原文没有的解释 | ❌ | ❌ |

### 8.2 L3 持久化任务队列

#### 8.2.1 状态迁移

```
                    ┌──────────┐
         enqueue →  │  queued  │  ← 任务写入 DB (status='queued')
                    └────┬─────┘
                         │ Semaphore.acquire() 成功
                         ▼
                    ┌──────────┐
                    │ running  │  ← DB 更新 (status='running')
                    └────┬─────┘
                         │
              ┌──────────┼──────────┐
              │          │          │
              ▼          ▼          ▼
         ┌────────┐ ┌────────┐ ┌────────┐
         │  done  │ │ failed │ │ timeout│  ← DB 更新 (status + error_detail)
         └────────┘ └────────┘ └────────┘
```

#### 8.2.2 入队语义

```rust
/// 入队 L3 校正任务
/// 1. 先写 DB: INSERT INTO transcript_corrections 的 meta 记录 (status='queued')
/// 2. 再 spawn 异步任务（内部获取 Semaphore 许可后执行）
/// 3. 立即返回当前队列状态给前端
async fn enqueue_l3_correction(db: &Db, meeting_id: &str) -> Result<QueueStatus> {
    // 幂等: 同一 meeting 已有 queued/running 任务 → 不重复提交
    if db.has_pending_l3_task(meeting_id).await? {
        return Ok(QueueStatus::AlreadyQueued);
    }
    db.insert_l3_task_meta(meeting_id, "queued").await?;

    let db = db.clone();
    let mid = meeting_id.to_string();
    tauri::async_runtime::spawn(async move {
        let _permit = LLM_CORRECTION_QUEUE.acquire().await;
        db.update_l3_task_status(&mid, "running").await.ok();
        match tokio::time::timeout(Duration::from_secs(60), run_llm_correction(&db, &mid)).await {
            Ok(Ok(())) => { db.update_l3_task_status(&mid, "done").await.ok(); }
            Ok(Err(e)) => { db.update_l3_task_status(&mid, "failed").await.ok(); log::error!("L3 failed: {}", e); }
            Err(_)     => { db.update_l3_task_status(&mid, "timeout").await.ok(); log::warn!("L3 timeout: {}", mid); }
        }
    });
    Ok(QueueStatus::Queued)
}
```

**关键约束**：
- 入队时**立即写入 DB**，任务不会因 App 崩溃而丢失
- 同一 meeting **不可重复入队**（幂等保护）
- 超时任务保留 `timeout` 状态，前端显示"超时，可重试"按钮
- App 重启时：查询所有 `queued`/`running` 任务，重新 spawn

### 8.3 L3 建议数据结构

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct L3CorrectionSuggestion {
    pub id: String,
    pub meeting_id: String,
    pub original_span: String,       // 查找文本（LLM 产出）
    pub suggested_text: String,      // 替换文本（LLM 产出）
    pub occurrences: Vec<CharRange>, // 后端 str::find 所有匹配位置
    pub language: String,
    pub correction_type: String,     // "chemical_name" | "ghs_code" | ...
    pub reason: String,
    pub source_snapshot_hash: String,
    pub status: String,
    pub reviewed_by: Option<String>,
    pub reviewed_at: Option<String>,
}

pub struct CharRange {
    pub start: usize,  // Unicode scalar value 索引
    pub end: usize,    // Unicode scalar value 索引
}
```

### 8.4 Offset 语义定义

**统一采用 Unicode scalar value 计数**（Rust `char` 的索引）。

| 环境 | 获取方式 | 与 Unicode scalar value 的关系 |
|------|----------|------|
| Rust | `s.char_indices()` | **原生一致** |
| JavaScript | `[...str]` 展开后索引 | **一致**（ES6 迭代器按 code point） |
| JavaScript | `String.length` | **不一致**（按 UTF-16 code unit，**禁止使用**） |

### 8.5 L3 "全文 N 处统一接受"的限制与风险说明

> **这是 V1 的受控简化策略，非天然安全的默认行为。必须明确其边界。**

**策略**：LLM 返回 `(original_span, suggested_text)`，后端在全文搜索所有出现位置，前端展示为"全文 N 处"。

**风险**：
1. **上下文差异**：同一字符串在不同语境中，可能只有部分需要修正（一处是识别错误，另一处是用户有意表达）
2. **短子串误匹配**：LLM 返回过短的 `original_span`（如单个汉字"酸"），会误匹配"硫酸""硝酸"等

**约束**：
1. **LLM Prompt 中要求 `original_span` 至少包含 3-4 个字符**，避免过短子串的灾难性匹配
2. 后端对超短 `original_span`（< 3 字符）执行额外边界检查，若匹配次数超过阈值（> 全文字符数/2），标记为"待人工复核"并降级为逐条确认模式
3. **前端 UI 明确提示**："以下建议基于全文同串匹配。如仅需修正部分出现，请点击[逐条展开]逐处确认"
4. **高风险术语类型**（`correction_type = "ghs_code" | "cas_number" | "un_number"`）默认要求展开逐条查看上下文后方可接受

**后续版本**：若用户反馈显示逐条确认需求强烈，V2 将支持 occurrence 级别的独立接受/拒绝。

### 8.6 LLM Provider 与降级链

默认 `qwen2.5:7b`，降级至 `3b`，再失败则静默放弃。降级链不阻塞主流程。

### 8.7 前端交互：按术语聚类 + 风险提示

```
┌──────────────────────────────────────────────────────────────┐
│  L3 深度校正建议 (共 23 条, 涉及 5 个术语)    状态: ✅ 已完成  │
│  ⚠️ 以下建议基于全文同串匹配，如仅需修正部分出现请逐条展开       │
│                                                              │
│  ┌─ 🟡 化学名: ポリウレタン (全文 8 处) ────────────────┐   │
│  │  "ポリ ウレ たん" → "ポリウレタン"                    │   │
│  │  [一键接受全部 8 处]  [逐条查看上下文]  [全部拒绝]    │   │
│  └──────────────────────────────────────────────────────┘   │
│                                                              │
│  ┌─ 🔴 代码（高风险）: H225 (全文 5 处) ──────────────┐   │
│  │  "H二二五" → "H225"                                    │   │
│  │  ⚠️ 安全代码类术语建议逐条确认                           │   │
│  │  [逐条展开确认 (5处)]  [全部拒绝]                       │   │
│  └──────────────────────────────────────────────────────┘   │
│                                                              │
│  [接受全部]  [拒绝全部]                                      │
└──────────────────────────────────────────────────────────────┘
```

### 8.8 auto_accept 策略

字段保留在数据模型和 settings 表中，**V1 UI 不暴露**。默认强制为 `false`。

### 8.9 L3 接受后摘要重新生成提示

用户接受 L3 校正后，前端提示可手动触发重新生成摘要。

---

## 9. 与现有录音停止链路的集成

### 9.1 当前链路（基于实际代码）

```
recording_manager.rs: stop_recording()
  → emit("recording-stopped", { meeting_id })

transcriptService.ts (line ~57):
  listen<TranscriptUpdate>('transcript-update', ...) → 各 chunk 到达

TranscriptContext.tsx (line ~326):
  聚合 transcript state → indexedDBService.saveTranscript(meetingId, update)

useRecordingStop.ts (line ~80):
  编排停止流程 → storageService.saveMeeting(title, transcripts, folderPath)

storageService.ts (line ~44):
  invoke<SaveMeetingResponse>('api_save_transcript', { meetingTitle, transcripts, folderPath })

useTranscriptRecovery.ts:
  从 IndexedDB 恢复未保存的 transcript（App 崩溃后重新打开时）
```

### 9.2 术语校正集成后的链路

```
recording_manager.rs: stop_recording()
  → emit("recording-stopped", { meeting_id })

useRecordingStop.ts:
  │
  ├─► Step 1: 保存（同步等待）
  │     // 从 TranscriptContext 读取双轨 buffer
  │     const rawBuffer = transcriptContext.getRawTranscripts()      // 新增
  │     const normalizedBuffer = transcriptContext.getDisplayTranscripts()
  │     const hash = await invoke("compute_terminology_snapshot_hash")
  │     await storageService.saveMeeting(...)  // 内部调用新命令
  │       // 'save_transcript_with_terminology', {
  │       //   meetingTitle, transcripts: normalizedBuffer,
  │       //   rawTranscript: rawBuffer,
  │       //   terminologySnapshotHash: hash
  │       // }
  │     // 旧 'api_save_transcript' 保留兼容期（Phase 1A），
  │     // 无 rawTranscript 参数时 raw 字段留空
  │
  ├─► Step 2: 异步触发 L3
  │     const { status } = await invoke("run_llm_terminology_correction", { meeting_id })
  │     // status: "queued" | "running" | "already_queued" — L3 已入队（DB 已写入）
  │
  └─► Step 3: 监听 L3 完成
        listen("llm-corrections-ready", ...)
```

### 9.3 职责划分

| 职责 | 负责方 | 说明 |
|------|:---:|------|
| 双轨状态管理 | `TranscriptContext` | 聚合 raw 和 display 两个 buffer |
| 停止流程编排 | `useRecordingStop` | 调用保存 + 触发 L3 |
| IPC 封装 | `storageService` | `saveMeeting` 适配新命令 |
| 本地缓存 | `indexedDBService` | 同步保存双轨数据，支持恢复 |
| 崩溃恢复 | `useTranscriptRecovery` | 从 IndexedDB 恢复双轨 buffer |

---

## 10. TranscriptUpdate 事件扩展

### 10.1 当前结构

```rust
// worker.rs:27
pub struct TranscriptUpdate {
    pub text: String,              // STT 输出
    pub timestamp: String,
    pub source: String,
    pub sequence_id: u64,
    pub chunk_start_time: f64,
    pub is_partial: bool,
    pub confidence: f32,
    pub audio_start_time: f64,
    pub audio_end_time: f64,
}
```

### 10.2 扩展后结构

```rust
pub struct TranscriptUpdate {
    // === 现有字段（向后兼容）===
    pub text: String,
    pub timestamp: String,
    pub source: String,
    pub sequence_id: u64,
    pub chunk_start_time: f64,
    pub is_partial: bool,
    pub confidence: f32,
    pub audio_start_time: f64,
    pub audio_end_time: f64,

    // === 新增字段（可选，旧前端忽略） ===
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_text: Option<String>,         // L0: STT 原始输出（L1后、L2前）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub corrections_applied: Option<u32>, // L2 替换次数
}
```

### 10.3 worker.rs 集成位置

```rust
// 所有 STT 引擎分支，cleaned_text 赋值后:
let raw_text = text.trim().to_string();               // L0
let corrected = apply_terminology_correction(&raw_text, &rules);
let display_text = corrected.into_owned();             // L1+L2

let update = TranscriptUpdate {
    text: display_text,
    raw_text: Some(raw_text),
    corrections_applied: Some(count),
    // ... 其余不变
};
```

### 10.4 前端双轨聚合实现建议

**内存优化**：对于 2-3 小时的会议（TranscriptUpdate 事件可达数千次），避免每个事件触发 React State 的不可变数组重组。建议使用 `useRef` 维护 mutable buffer，仅在渲染间隔同步到 State：

```typescript
// transcriptService.ts 中的建议模式
const rawBufferRef = useRef<string[]>([]);
const displayBufferRef = useRef<string[]>([]);

listen<TranscriptUpdate>('transcript-update', (event) => {
  if (event.payload.raw_text !== undefined) {
    rawBufferRef.current.push(event.payload.raw_text!);
  } else {
    rawBufferRef.current.push(event.payload.text);
  }
  displayBufferRef.current.push(event.payload.text);

  // 仅在需要时同步到 State（例如每次 chunk 或每秒一次）
  syncToState();
});
```

---

## 11. 迁移策略与受影响模块

### 11.1 现有命令兼容策略

| 命令 | V3.3 中的处理 | 说明 |
|------|------|------|
| `api_save_transcript`（旧） | **保留兼容期（Phase 1A-2）** | 无 raw/ hash 参数时 raw 和 hash 字段留空。旧前端/旧调用路径不受影响 |
| `save_transcript_with_terminology`（新） | **Phase 1A 新增** | 接收 rawTranscript + terminologySnapshotHash 参数 |
| `api_get_meeting` | **扩展返回** | 返回中增加 rawTranscript 字段（可选） |
| transcript history 读取 | **兼容** | 旧数据 raw 字段为空，前端优雅处理 |

### 11.2 双轨扩展的受影响模块清单

| 模块 | 文件 | 需改造内容 |
|------|------|------|
| **TranscriptUpdate 事件** | `worker.rs` | 增加 raw_text + corrections_applied 字段 |
| **事件监听** | `transcriptService.ts` | 解析双轨 payload |
| **状态管理** | `TranscriptContext.tsx` | 维护 raw + display 两个 buffer |
| **本地持久化** | `indexedDBService.ts` | saveTranscript 存储双轨数据 |
| **崩溃恢复** | `useTranscriptRecovery.ts` | 从 IndexedDB 恢复双轨 buffer |
| **停止保存** | `useRecordingStop.ts` | 编排双轨保存 + L3 触发 |
| **IPC 封装** | `storageService.ts` | saveMeeting 适配新命令参数 |
| **命令层** | `database/commands.rs` | 新增 save_transcript_with_terminology |
| **DB 模型** | `database/models.rs` | Transcript 增加 raw_transcript / hash 字段 |
| **meeting details** | `app/_components/` | 扩展展示 raw 查看入口 |
| **摘要生成** | `summary/` | 明确依赖 normalized_transcript（V1 不变） |

### 11.3 分阶段迁移

| 阶段 | 迁移内容 |
|------|------|
| Phase 1A | 新增命令 + 新 DB 字段 + TranscriptUpdate 扩展 + 前端双轨 buffer |
| Phase 1A | 旧命令保留，无 raw 参数时 raw 字段留空 |
| Phase 1B-2 | 新命令 + 双轨稳定后，旧调用路径逐步迁移至新命令 |
| Phase 3 | 评估是否废弃旧命令 |

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

    -- 术语来源与包治理
    source_type      TEXT NOT NULL DEFAULT 'manual',  -- 'preset' | 'imported' | 'manual'
    package_id       TEXT,               -- 预置包标识或导入批次 ID
    package_name     TEXT,               -- 人类可读的包名称
    import_batch_id  TEXT,               -- CSV 导入批次 ID（用于批次回滚）

    created_at       TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at       TEXT NOT NULL DEFAULT (datetime('now')),

    UNIQUE(original, language)  -- CSV 导入 upsert 键
);
```

### 12.2 新建表：`transcript_corrections`

```sql
CREATE TABLE IF NOT EXISTS transcript_corrections (
    id                  TEXT PRIMARY KEY,
    meeting_id          TEXT NOT NULL,
    original_span       TEXT NOT NULL,
    suggested_text      TEXT NOT NULL,
    occurrences_json    TEXT,               -- JSON: [{start, end}, ...]
    language            TEXT,
    correction_type     TEXT NOT NULL DEFAULT 'llm',
    correction_level    TEXT NOT NULL DEFAULT 'l3',
    reason              TEXT,
    source_snapshot_hash TEXT,
    status              TEXT NOT NULL DEFAULT 'pending',
    -- L3 任务生命周期状态 (meta 记录):
    --   'queued' | 'running' | 'done' | 'failed' | 'timeout'
    error_detail        TEXT,               -- 失败/超时原因
    reviewed_by         TEXT,
    reviewed_at         TEXT,
    created_at          TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (meeting_id) REFERENCES meetings(id) ON DELETE CASCADE
);
```

### 12.3 现有表扩展（幂等迁移）

```sql
-- transcripts 表新增:
--   raw_transcript TEXT            — L0: 原始 STT 输出（不可变）
--   terminology_snapshot_hash TEXT — 保存时的术语表 SHA-256
--   l1_prompt_snapshot TEXT        — L1 prompt 完整内容（审计用）

-- settings 表新增:
--   terminology_enabled INTEGER DEFAULT 1
--   initial_prompt_enabled INTEGER DEFAULT 1
--   llm_correction_enabled INTEGER DEFAULT 1
--   llm_correction_auto_accept INTEGER DEFAULT 0  — V1 UI 不暴露
```

所有 ALTER TABLE 通过 `add_column_if_not_exists()` 幂等执行。

### 12.4 数据模型（Rust）

参见 V3.2 11.4 节，追加 `TranscriptCorrection.error_detail` 字段。

---

## 13. 后端实现规范

### 13.1 模块组织结构

```
frontend/src-tauri/src/
├── terminology/
│   ├── mod.rs
│   ├── cache.rs                     # 统一缓存 + snapshot_hash + refresh_all_caches()
│   ├── commands.rs                  # Tauri 命令（含 L3 入队/状态查询）
│   ├── corrector.rs                 # apply_correction() + build_term_rule() + RuleBuildError
│   ├── queue.rs                     # L3 持久化队列（入队写DB + Semaphore(1) + 恢复）
│   └── snapshot.rs                  # 术语快照哈希计算
│
├── whisper_engine/
│   └── whisper_engine.rs           # L1 集成（API 验证后）
│
├── audio/transcription/worker.rs   # L2 集成 + TranscriptUpdate 扩展
│
├── summary/llm_client.rs           # L3: suggest_corrections()
│
├── database/
│   ├── models.rs                   # 追加模型
│   ├── repositories/
│   │   └── terminology.rs          # 含 package 级查询 + L3 任务状态查询
│   └── setup.rs                    # 幂等迁移 + L3 任务恢复触发
│
├── lib.rs
│
└── migrations/
    └── 20260427000000_add_terminology.sql
```

### 13.2 Tauri 命令

```rust
// 术语 CRUD
terminology::commands::get_terminology_list,
terminology::commands::save_terminology_entry,  // 返回 RuleBuildError 信息（失效规则标记）
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

// L3 LLM 校正（持久化队列）
terminology::commands::run_llm_terminology_correction,   // 入队（写DB）+ 返回队列状态
terminology::commands::get_llm_queue_status,              // 查询任务状态
terminology::commands::retry_llm_correction,              // 超时/失败后重试
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
// lib.rs setup 闭包
let db = app.state::<state::AppState>().db_manager.clone();

tauri::async_runtime::spawn(async move {
    // 1. 初始化术语缓存（L1 + L2 + snapshot hash）
    if let Err(e) = terminology::cache::refresh_all_caches(&db).await {
        log::warn!("Terminology caches init failed: {}", e);
    }

    // 2. 恢复 L3 任务：查询所有 status IN ('queued','running') 的 meeting
    //    running 状态意味着上次崩溃，直接重新入队
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
| `CorrectionDiffView` | `components/CorrectionDiff/DiffView.tsx` | 按术语聚类 + 风险提示 |
| `L3QueueStatus` | `components/CorrectionDiff/QueueStatus.tsx` | L3 任务状态指示器 + 重试按钮 |

### 14.2 TypeScript 类型

```typescript
// frontend/src/types/terminology.ts

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

export type L3QueueStatus = 'idle' | 'queued' | 'running' | 'done' | 'failed' | 'timeout';
```

### 14.3 双轨聚合内存优化

参见第 10.4 节。使用 `useRef` + 间隔同步，避免数千次 State 更新导致 GC 压力和渲染卡顿。

---

## 15. 配置与存储

配置项同 V3.2。`auto_accept` 字段保留但 V1 UI 不暴露。CSV 导入：MVP 仅 UTF-8 BOM，Phase 1B 追加 Shift-JIS。预置术语包通过 `package_id` 级管理。

---

## 16. 硬件要求与降级策略

同 V3.2。第一版降级策略为**推荐策略**（用户手动配置），自动硬件检测后续版本完善。

---

## 17. 实施计划与 MVP 策略

| 阶段 | 内容 | 工作量 |
|------|------|:---:|
| **Phase 0** | 基线测量 + 5 项技术验证（含 tokenizer 确认） | 1-2 人天 |
| **Phase 1A (MVP)** | L2 混合引擎 + DB（含 raw/snapshot/package）+ TranscriptUpdate 双轨 + 基础 UI + 迁移策略 | 3-4 人天 |
| **Phase 1B** | L1（API 验证后）+ CSV 导入（UTF-8 BOM）+ 精确 token 计数 | 2 人天 |
| **Phase 2** | L3 持久化队列 + 恢复 + 聚类 UI | 3-4 人天 |
| **Phase 3** | 完善：Shift-JIS、审计报告、摘要重生成提示、包升级/回滚 | 2-3 人天 |

**总计：11-15 人天**

MVP（Phase 1A）核心范围：L2 + DB 迁移 + TranscriptUpdate 双轨 + 基础 UI + 预置包（50 条）。

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
✅ TC-07: fancy-regex 编译失败的规则 UI 标记为"已失效"
✅ TC-08: Cow<str> 无匹配时不分配
✅ TC-09: TranscriptUpdate raw_text 和 text 分别正确
✅ TC-10: L3 队列：2 个并发任务串行，不 OOM
✅ TC-11: L3 持久化：App 崩溃重启后任务恢复
✅ TC-12: L3 幂等：同一 meeting 不可重复入队
✅ TC-13: L3 超时任务 → status='timeout' → 前端重试按钮可见
✅ TC-14: L3 超短 original_span（<3字符）→ 触发边界检查
✅ TC-15: 全文 N 处提示文案正确显示
✅ TC-16: 高风险术语默认要求逐条展开确认
✅ TC-17: 术语快照哈希修改后正确变化
✅ TC-18: 幂等迁移重复执行不报错
✅ TC-19: Offset: Rust char_indices ↔ JS [...str] 索引一致
✅ TC-20: 前端双轨: useRef buffer 不触发内存抖动
```

### 18.2 验收标准

- [ ] `raw_transcript` 强制保留，TranscriptUpdate 双轨正确
- [ ] `terminology_snapshot_hash` 保存正确，术语变更后哈希变化
- [ ] L2 日语连续术语无消费效应（TC-01）
- [ ] fancy-regex 编译失败的规则 UI 标记为"已失效"
- [ ] L1 token 计数方案经过 Phase 0 验证确认
- [ ] L1 prompt 不写入常规日志
- [ ] L3 持久化队列：入队写 DB、Semaphore(1)、启动恢复、幂等、超时+重试
- [ ] L3 全文 N 处提示文案 + 高风险术语逐条确认
- [ ] L3 超短 original_span 边界检查生效
- [ ] 按术语聚类 + 批量操作 UI
- [ ] `auto_accept` V1 UI 不暴露
- [ ] 旧 `api_save_transcript` 兼容运行
- [ ] 双轨扩展覆盖 IndexedDB / transcript history / reload 恢复
- [ ] 前端 useRef buffer 无内存抖动（2 小时会议场景）
- [ ] 校正审计日志完整

---

## 19. 风险与应对

| 风险 | 影响 | 概率 | 应对 |
|------|------|:---:|------|
| whisper-rs 未暴露 `set_initial_prompt` | L1 无法实现 | 中 | Phase 0 验证，若无则废弃 L1 |
| `fancy-regex` 编译或性能问题 | L2 日/中全词不可用 | 低 | Phase 0 预研，若不可用降级为子串匹配 |
| Whisper tokenizer 类型与 `cl100k_base` 不一致 | L1 精确 token 计数不准确 | 中 | Phase 0 验证，若无法确认回退保守估算 |
| L3 全文同串匹配误用于不同上下文 | 部分不需要修正的地方被建议修正 | 中 | 全文 N 处提示 + 高风险术语逐条 + 短子串边界检查 |
| L3 任务持久化未正确实现（入队未写 DB） | App 崩溃任务丢失 | 低 | 入队即写 DB + TC-11 测试 |
| 两场会议连续结束 L3 堆积 | UI 显示排队 | 中 | Semaphore(1) + 前端排队状态 |
| 危化品合规风险 | 错误校正导致安全信息错误 | **高** | raw 不可变 + L3 仅建议 + auto_accept 隐藏 + 审计日志 |

---

## 20. 合规与法务审查

| 节点 | 时机 | 内容 |
|------|------|------|
| Gate A | Phase 1A 后 | raw 保留 + L2 确定性的审计兼容性 |
| Gate B | Phase 2 上线前 | L3 建议模式（仅建议、全文 N 处提示、高风险逐条确认） |
| Gate C | Phase 3 后 | 审计报告格式满足行业监管 |

核心原则：raw 不可变、L2 可审计、L3 默认仅建议、auto_accept V1 不暴露。

---

## 21. 回滚与功能淘汰

- 总开关：`terminology_enabled = 0`
- 分级回滚：L1/L3 可独立关闭
- 术语包回滚：`import_batch_id` 级回滚
- L3 任务：超时可重试，失败可重试
- raw_transcript 永远不变（最终回滚锚点）
- 旧 `api_save_transcript` 兼容期保留

---

## 附录 A：成功指标体系

| 指标 | 方法 | 目标 |
|------|------|:---:|
| L2 误替换率 | 人工审核 100 条替换 | < 1% |
| L3 建议接受率 | 用户实际接受/拒绝比 | > 70% |
| 用户人工复核时长 | 对比有无术语校正 | 下降 > 30% |
| 术语准确率提升 | 与 Phase 0 基线对比 | 基于基线设定 |
| L2 连续术语遗漏率 | TC-01 | 0% |
| L3 任务丢失率 | 强制崩溃测试 | 0% |

---

## 附录 B：V3.2 → V3.3 变更摘要

| 变更项 | V3.2 | V3.3 | 触发来源 |
|--------|------|------|------|
| **L1 tokenizer 表述** | `cl100k_base` 写成既定事实 | **降级为候选方案**，需 Phase 0 验证，回退至保守估算 | GPT P0-1 |
| **L3 全文匹配风险** | 未标注风险和约束 | **受控简化**：明确风险 + 全文 N 处提示 + 短子串边界检查 + 高风险术语强制逐条 | Gemini + GPT P1-1 |
| **L3 队列实现** | 伪代码未完整表达持久化语义 | **完整状态机**：queued→running→done/failed/timeout + 入队即写 DB + 幂等 + 超时重试 | GPT P1-2 |
| **前端模块引用** | 用 "page.tsx" 指代前端 | **精确为实际模块**：TranscriptContext / useRecordingStop / storageService / transcriptService / indexedDBService / useTranscriptRecovery | GPT P1-3 |
| **迁移策略** | 未描述 | **新增第 11 节**：旧命令兼容期 + 11 个受影响模块清单 + 分阶段迁移计划 | GPT P1-4 |
| **双轨扩展影响范围** | 仅提及实时+保存 | **明确**：双轨扩展影响 IndexedDB / transcript history / reload recovery / meeting details | GPT P2-2 |
| **前端内存优化** | 用 `[...prev, new]` State 追加 | **useRef mutable buffer + 间隔同步**，避免 2-3 小时会议的内存抖动 | Gemini |
| **fancy-regex 编译失败** | 仅 `log::warn` + 跳过 | **UI 标记为"已失效"** + 错误信息入口 + `RuleBuildError` 类型 | Gemini |
| **snapshot hash 能力边界** | 未区分 | **明确 V1/Future 边界**：hash 是审计索引，非完整证据本体 | GPT P2-1 |
| **高风险术语策略** | 未区分 | **GHS/CAS/UN 代码类建议默认逐条展开确认** | GPT |

---

> **ドキュメントメンテナンス / 文档维护**：本 PRD V3.3 基于 V3.2 的 Gemini-3.1-Pro 和 GPT-4 双重技术审查后修订。本版本已具备交付研发进入技术详细设计和 POC 开发的条件。剩余 3 点 Gemini 建议（useRef 优化、L3 子串嵌套陷阱、fancy-regex 优雅降级）已作为实现注意事项写入对应章节。
