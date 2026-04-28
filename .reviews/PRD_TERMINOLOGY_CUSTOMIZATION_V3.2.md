# PRD：转录专业术语定制化功能

> **文档状态**：V3.2（综合 Gemini + GPT 对 V3.1 的审查后修订）
> **创建日期**：2026-04-27
> **修订日期**：2026-04-27
> **关联项目**：Meetily v0.3.0
> **目标行业**：危险化学品制造业（日系企业）
> **支持语言**：日本語 / 中文 / English
> **核心目标**：使 Meetily 转录引擎支持用户自定义专业术语词库，通过三级管道纠正语音识别（STT）产生的术语识别错误。原始 STT 输出强制保留，校正全程可追溯、可审计。

---

## 目录

1. [需求背景](#1-需求背景)
2. [核心问题分析](#2-核心问题分析)
3. [总体架构设计](#3-总体架构设计)
4. [审计与证据链设计](#4-审计与证据链设计)
5. [Phase 0：基线测量（前置条件）](#5-phase-0基线测量前置条件)
6. [第一级：Whisper initial_prompt 软引导](#6-第一级whisper-initial_prompt-软引导)
7. [第二级：正则实时校正通道](#7-第二级正则实时校正通道)
8. [第三级：LLM 深度校正建议](#8-第三级llm-深度校正建议)
9. [与现有录音停止链路的集成](#9-与现有录音停止链路的集成)
10. [TranscriptUpdate 事件扩展](#10-transcriptupdate-事件扩展)
11. [数据库设计](#11-数据库设计)
12. [后端实现规范](#12-后端实现规范)
13. [前端实现规范](#13-前端实现规范)
14. [配置与存储](#14-配置与存储)
15. [硬件要求与降级策略](#15-硬件要求与降级策略)
16. [实施计划与 MVP 策略](#16-实施计划与-mvp-策略)
17. [测试策略](#17-测试策略)
18. [风险与应对](#18-风险与应对)
19. [合规与法务审查](#19-合规与法务审查)
20. [回滚与功能淘汰](#20-回滚与功能淘汰)

---

## 1. 需求背景

### 1.1 业务场景

客户为在华日系危险化学品制造企业。日常会议特征：

| 特征 | 说明 |
|------|------|
| **多语言混合** | 会议中频繁切换日语、中文、英语 |
| **高度专业化** | 涉及 MSDS、CAS 编号、UN 危险货物编号、GHS 分类、化学物质 IUPAC 命名等 |
| **合规要求严格** | 转录文本用于内部审计与合规存档，术语准确性直接影响法律风险 |
| **三方沟通** | 日方技术人员（日语）、中方操作人员（中文）、国际供应商/客户（英语）共同参会 |

### 1.2 问题描述

Meetily 使用 Whisper / Parakeet 进行本地语音识别（STT）。危化品行业日企场景的关键挑战：

1. **跨语言术语混乱**：模型在日/中/英切换时将一种语言的发音"听成"另一种语言的文字
2. **化学物质名称识别率极低**：IUPAC 命名和日文片假名化学名在通用语料中几乎不存在
3. **安全编码格式特殊**：CAS RN、UN No.、GHS 代码在语音转写中极易出错
4. **片假名/汉字/罗马字混合**：日语化学术语同时使用多套文字系统

### 1.3 目标

- **强制保留原始 STT 输出**，校正结果分层存储，证据链完整
- **多语言支持**：日语、中文、英语
- **三级校正管道**：模型内 `initial_prompt` 软引导 → 正则精确替换 → LLM 校正**建议**
- **术语表可溯源**：记录每条术语的来源（预置/导入/手动），校正时记录使用的术语快照版本
- **转录时实时应用前两级校正，录音停止后通过串行队列异步触发第三级**
- **L3 校正默认仅建议，需用户确认后生效**；第一版仅支持纠错模式

---

## 2. 核心问题分析

### 2.1 STT 输出错误规律

#### 2.1.1 日语

| 错误类型 | 示例 | 根因 |
|----------|------|------|
| 片假名 → 汉字误转 | `ポリウレタン` → `保利売れたん` | token 覆盖不足 |
| 長音「ー」丢失 | `メチルエチルケトン` → 长音被省略 | whisper.cpp 对长音符不敏感 |
| 促音「っ」丢失 | `いんかせい` → 促音丢失 | 短音频段 token 易丢弃 |
| 浊音/半浊音混淆 | `ポリ` → `ボリ` / `ホリ` | 噪声下辅音歧义 |
| 拗音分割 | `メチル` → `メ チル` | 拗音 token 被拆分 |

#### 2.1.2 中文

| 错误类型 | 示例 | 根因 |
|----------|------|------|
| 同音字替换 | `甲苯二异氰酸酯` → `甲本二亿情酸纸` | 化学术语汉字组合生僻 |
| 数字+字母错位 | `H225` → `H二二五` | 中英混合 token 不稳定 |
| 多音字误读 | `重铬酸钾` → `重各酸钾` | "铬"是多音字 |

#### 2.1.3 英语（化学语境）

| 错误类型 | 示例 | 根因 |
|----------|------|------|
| IUPAC 命名拆分 | `2,4,6-Trinitrotoluene` → `two four six tri nitro toluene` | token 序列不熟悉 |
| CAS 编号格式 | `CAS 108-88-3` → `k Ass one o eight eighty eight three` | 连字符边界错误 |
| GHS 代码 | `H301` → `H three hundred one` | H+数字不在常见 token 表 |

#### 2.1.4 跨语言混合

| 错误类型 | 示例 | 根因 |
|----------|------|------|
| 日语→中文误转 | `この物質は…` → `这个物质是…` | 语言切换点失误 |
| 代码混入自然语言 | `UN 1203` → `うん いちにーぜろさん` | 代码被当作假名发音 |

### 2.2 设计决策：三级校正管道

| 维度 | L1：initial_prompt | L2：正则实时 | L3：LLM 异步建议 |
|------|:---|:---|:---|
| **执行时机** | Whisper 推理时 | 每次转录输出后 | 录音停止后（经串行队列） |
| **支持引擎** | 仅 Whisper | Whisper / Parakeet / Provider | 所有引擎 |
| **覆盖率（估计）** | ~15-30% | ~50-60% | ~10-20% |
| **确定性与审计** | 低（概率性） | 高（确定性） | 中（需确认） |
| **成本** | 免费 | 免费 | 本地免费 / API 付费 |

---

## 3. 总体架构设计

### 3.1 系统架构图

```
┌──────────────────────────────────────────────────────────────────────────────┐
│                          前端 (Next.js + React)                               │
│  ┌─────────────────────────┐    ┌──────────────────────────────────────────┐ │
│  │ 术语管理 UI              │    │ 转录查看 / 会议详情                        │ │
│  │ - 增删改查，按语言/包管理 │    │ - 实时转录面板 (L1+L2 校正后)              │ │
│  │ - 导入/导出 CSV          │    │ - L3 校正建议列表（按术语聚类）             │ │
│  │ - L1 prompt 超载警告      │    │ - 差异高亮（raw vs L1+L2 vs L3 建议）    │ │
│  │ - 术语包级启用/禁用       │    │ - 逐条/按术语批量接受拒绝                  │ │
│  └───────────┬─────────────┘    └──────────────────────────────────────────┘ │
│              │ invoke()                                                       │
└──────────────┼───────────────────────────────────────────────────────────────┘
               │
┌──────────────┴───────────────────────────────────────────────────────────────┐
│                      Tauri IPC 层 (Rust Backend)                              │
│                                                                              │
│  ┌─────────────────────────┐    ┌──────────────────────────────────────────┐ │
│  │ 术语 CRUD 命令           │    │ 缓存管理（统一原子刷新）                    │ │
│  │ 术语包级操作命令         │    │ refresh_all_terminology_caches()          │ │
│  └───────────┬─────────────┘    └──────────────┬───────────────────────────┘ │
│              │                                  ▼                             │
│  ┌──────────────────────────┐    ┌──────────────────────────────────────────┐ │
│  │ SQLite: terminology 表    │    │ 内存缓存（单次原子刷新）                    │ │
│  │ 含 source_type/package_id │   │  INITIAL_PROMPT_BY_LANG (L1)              │ │
│  │ UNIQUE(original,language) │   │  TERMINOLOGY_RULES (L2, 混合引擎)         │ │
│  └──────────────────────────┘    └──────────────┬───────────────────────────┘ │
│                                                 │                             │
│  ┌──────────────────────────────────────────────┴───────────────────────────┐ │
│  │              转录管道                                                     │ │
│  │                                                                          │ │
│  │  每个音频 chunk:                                                          │ │
│  │    ├─► Whisper → raw_text                                                 │ │
│  │    │     └─► 【L1】initial_prompt 注入（含精确 token 计数+截断）              │ │
│  │    ├─► Parakeet/Provider → raw_text (无 L1)                              │ │
│  │    ├─► 【L2】apply_correction(raw_text)                                   │ │
│  │    │     混合引擎: 英/子串=regex, 日/中全词=fancy-regex (含 look-around)   │ │
│  │    └─► emit("transcript-update", { text, raw_text, ... })                 │ │
│  │         → 前端同时接收 raw 和 corrected，分别聚合                            │ │
│  └──────────────────────────────────────────────────────────────────────────┘ │
│                                                                              │
│  ┌──────────────────────────────────────────────────────────────────────────┐ │
│  │              录音后处理                                                    │ │
│  │                                                                          │ │
│  │  recording-stopped                                                       │ │
│  │    ├─► 前端保存: raw_transcript + normalized_transcript                    │ │
│  │    │     + terminology_snapshot_hash（审计锚点）                            │ │
│  │    └─► 异步 L3 校正建议任务                                                │ │
│  │          ├─► 全局 Semaphore(1) 串行队列                                    │ │
│  │          ├─► 版本冲突检测（snapshot_hash 比对）                             │ │
│  │          ├─► 建议按"查找替换规则"存储（前端全局高亮所有匹配）                 │ │
│  │          └─► 应用退出时: 未完成任务在下次启动时自动恢复                      │ │
│  └──────────────────────────────────────────────────────────────────────────┘ │
└──────────────────────────────────────────────────────────────────────────────┘
```

---

## 4. 审计与证据链设计

### 4.1 四层文本模型

| 层 | 存储位置 | 来源 | 可变性 | 用途 |
|---|---------|------|:---:|------|
| **L0** | `transcripts.raw_transcript` | STT 引擎直接输出（L1 后、L2 前） | **不可变** | 审计锚点 |
| **L1+L2** | `transcripts.transcript` | raw → L2 正则替换 | 可被 L3 接受后更新 | 实时显示、日常使用 |
| **L3 建议** | `transcript_corrections` | LLM 异步生成 | status 可变化 | 深度校正候选 |
| **Final** | `transcripts.transcript`（更新后） | 用户确认的合并结果 | 接受后写入 | 归档导出 |

### 4.2 术语快照版本（审计增强）

每次录音停止并保存时，记录当时术语表的 SHA-256 摘要：

```rust
/// 在保存 transcript 时，计算当前术语表的快照哈希
fn compute_terminology_snapshot_hash(entries: &[TerminologyEntry]) -> String {
    // 将所有启用的术语按 (original, language, replacement) 排序后序列化，取 SHA-256
    // 用于后续审计时验证"当时用了哪套规则"
}
```

快照哈希保存在 `transcripts` 表的新增字段 `terminology_snapshot_hash` 中。

**审计追溯能力**：

- 原始模型输出了什么？ → `raw_transcript`
- L2 用了哪套规则？ → `terminology_snapshot_hash` → 对比术语表历史
- L3 做了哪些建议？ → `transcript_corrections` 表
- 用户接受了哪些？ → `status = 'accepted'` 的记录，含操作人和时间

---

## 5. Phase 0：基线测量（前置条件）

### 5.1 验证清单

除 V3.1 中定义的基线测量外，追加以下技术验证：

| 验证项 | 方法 | 阻塞级别 |
|--------|------|:---:|
| whisper-rs 0.13.x 是否暴露 `set_initial_prompt` API | 代码审查 FullParams 结构体 | **P0** — 缺失则废弃 L1 |
| `fancy-regex` crate 在项目 Rust 版本下可编译 | `cargo add fancy-regex && cargo check` | **P0** — 缺失则 L2 日/中全词匹配降级 |
| `regex` 1.11 对 `\p{Katakana}` / `\p{Han}` 的实际行为 | 编写 5 个边界测试用例并运行 | **P1** |
| `tiktoken-rs` 可用性（用于 L1 精确 token 计数） | `cargo add tiktoken-rs && cargo check` | **P2** — 缺失则回落至估算 |
| 当前 `transcript-update` 事件在前端的监听和聚合路径 | 追踪 page.tsx 中的 `listen('transcript-update', ...)` 逻辑 | **P1** — 影响双轨扩展方案 |

### 5.2 预估工作量

1-2 人天。

---

## 6. 第一级：Whisper initial_prompt 软引导

### 6.1 实现原理

Whisper.cpp 暴露 `initial_prompt` 参数，通过 decoder 的 cross-attention 偏置 token 分布。软引导，非确定性。224 token 硬限制。仅对 Whisper 引擎生效。

### 6.2 精确 Token 计数（替代估算）

化学术语在 Whisper BPE tokenizer 下可能被切分为大量 subword token（例如 `メチルエチルケトン` 可能被切为 6-8 个 token）。粗略的"词数 × 系数"估算不可靠。

```rust
use tiktoken_rs::cl100k_base; // Whisper 使用 cl100k_base 词表

/// 使用 Whisper 的真实 BPE tokenizer 精确计数
fn count_prompt_tokens(terms: &[String], base_prompt: &str) -> usize {
    let bpe = cl100k_base().expect("Failed to load cl100k_base tokenizer");
    let full_text = format!("{} {}", base_prompt, terms.join(", "));
    bpe.encode_with_special_tokens(&full_text).len()
}

/// 如果 tiktoken-rs 不可用时的回退估算
fn estimate_tokens_fallback(text: &str, lang: &str) -> usize {
    // 保守估算，偏向高估以确保不超限
    match lang {
        "ja" => (text.chars().count() as f64 * 0.8) as usize,
        "zh" => (text.chars().count() as f64 * 0.9) as usize,
        _ => (text.chars().count() as f64 * 0.4) as usize,
    }
}
```

**回退策略**：如果 `tiktoken-rs` 不可用，使用保守估算（偏向高估），宁可少注入几条也不冒险超出 224 token 限制。

### 6.3 Token 截断策略

```rust
fn build_prompt_with_truncation(
    language: &str, 
    high_priority_terms: &[&TerminologyEntry], 
    max_tokens: usize,
) -> (String, Vec<String>) {  // 返回 (prompt, 被排除的术语 ID 列表)
    let base = get_base_prompt(language);
    let base_tokens = count_or_estimate_tokens(&base);

    let mut included = Vec::new();
    let mut excluded = Vec::new();
    let mut current_tokens = base_tokens;

    // 按 updated_at 降序（最近更新的优先保留）
    let mut sorted: Vec<_> = high_priority_terms.iter().collect();
    sorted.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));

    for entry in sorted {
        let term_tokens = count_or_estimate_tokens(&entry.replacement);
        if current_tokens + term_tokens + 2 <= max_tokens { // +2 for ", "
            current_tokens += term_tokens + 2;
            included.push(entry.replacement.clone());
        } else {
            excluded.push(entry.id.clone());
        }
    }

    let prompt = format!("{} {}", base, included.join(", "));
    (prompt, excluded)
}
```

**前端反馈**：术语管理页面显示 "L1 Prompt: 25/38 条高优先级术语已注入，13 条因 token 限制未包含"。用户可按需调整优先级。

### 6.4 L1 Prompt 审计存储

Prompt 完整内容**不写入常规日志**（避免客户术语泄露）。改为：

- 运行日志仅记录：`"L1 prompt injected: 25 terms, 187 tokens, 13 excluded"`
- 完整 prompt 快照写入 `transcripts` 表的 `l1_prompt_snapshot` 字段（与 transcript 数据同生命周期、同访问控制）

### 6.5 代码集成位置

**文件位置**：`frontend/src-tauri/src/whisper_engine/whisper_engine.rs`

`FullParams` 构造完成后、`state.full()` 之前插入（仅当 Phase 0 验证 API 可用时）。

---

## 7. 第二级：正则实时校正通道

### 7.1 技术背景

Rust 标准 `regex` crate（v1.x）**不支持 look-ahead/look-behind**，但支持 `\p{Katakana}` / `\p{Han}` 等 Unicode 字符类。

V3.0 的 look-around 方案不可编译。V3.1 的捕获组替代方案在连续术语场景下有"消费效应" bug（第一条匹配消耗了术语间的分隔符，导致第二条无法匹配）。

**V3.2 方案：混合引擎**，按规则类型选择最优引擎：

| 规则类型 | 引擎 | 原因 |
|----------|------|------|
| `whole_word = false`（所有语言） | `regex` | 纯子串匹配，DFA 最快 |
| `whole_word = true`，语言 = `en` | `regex` + `\b` | 英语词边界 DFA 原生支持 |
| `whole_word = true`，语言 = `ja`/`zh` | **`fancy-regex`** + look-around | 需要 `(?<!...)`/`(?!...)` 模拟日/中词边界 |

`fancy-regex` 底层优先使用 `regex` 的 DFA 引擎，仅在遇到 look-around 断言时回退到回溯引擎。对于术语匹配场景（模式较短、文本量小），性能影响可忽略不计。

### 7.2 模式构建

```rust
use fancy_regex::Regex as FancyRegex;
use regex::Regex;

enum CompiledRule {
    /// 标准 regex（用于英语全词、所有语言的子串匹配）
    Standard {
        regex: Regex,
        replacement: String,
        original_len: usize,
    },
    /// fancy-regex（用于日/中全词匹配，含 look-around）
    Fancy {
        regex: FancyRegex,
        replacement: String,
        original_len: usize,
    },
}

fn build_whole_word_pattern_ja(escaped_term: &str) -> String {
    // 前面不是日文字符，后面不是日文字符
    let boundary = r"[\p{Han}\p{Hiragana}\p{Katakana}ー]";
    format!(
        r"(?<!{boundary}){escaped}(?!{boundary})",
        boundary = boundary,
        escaped = escaped_term
    )
}

fn build_whole_word_pattern_zh(escaped_term: &str) -> String {
    // 前面不是 CJK 字符，后面不是 CJK 字符
    format!(
        r"(?<!\p{{Han}}){}(?!\p{{Han}})",
        escaped_term
    )
}

fn build_term_rule(entry: &TerminologyEntry) -> Result<CompiledRule, String> {
    let escaped = regex::escape(&entry.original);
    let original_len = entry.original.chars().count();

    if !entry.whole_word {
        let pattern = if entry.case_sensitive {
            escaped
        } else {
            format!("(?i){}", escaped)
        };
        let re = Regex::new(&pattern).map_err(|e| e.to_string())?;
        return Ok(CompiledRule::Standard {
            regex: re,
            replacement: entry.replacement.clone(),
            original_len,
        });
    }

    match entry.language.as_str() {
        "ja" | "zh" => {
            let pattern = match entry.language.as_str() {
                "ja" => build_whole_word_pattern_ja(&escaped),
                _    => build_whole_word_pattern_zh(&escaped),
            };
            // 如未设置 case_sensitive，添加 (?i) 前缀
            let pattern = if entry.case_sensitive { pattern } else { format!("(?i){}", pattern) };
            let re = FancyRegex::new(&pattern).map_err(|e| e.to_string())?;
            Ok(CompiledRule::Fancy {
                regex: re,
                replacement: entry.replacement.clone(),
                original_len,
            })
        }
        _ => {
            // 英语等：标准 \b
            let pattern = if entry.case_sensitive {
                format!(r"\b{}\b", escaped)
            } else {
                format!(r"(?i)\b{}\b", escaped)
            };
            let re = Regex::new(&pattern).map_err(|e| e.to_string())?;
            Ok(CompiledRule::Standard {
                regex: re,
                replacement: entry.replacement.clone(),
                original_len,
            })
        }
    }
}
```

### 7.3 核心替换逻辑

```rust
use std::borrow::Cow;

pub fn apply_terminology_correction<'a>(
    text: &'a str,
    rules: &[CompiledRule],
) -> Cow<'a, str> {
    let mut result = Cow::Borrowed(text);

    for rule in rules {
        let matched = match rule {
            CompiledRule::Standard { regex, .. } => regex.is_match(&result),
            CompiledRule::Fancy { regex, .. } => {
                regex.is_match(&result).unwrap_or(false)
            }
        };

        if matched {
            let owned = result.into_owned();
            let replaced = match rule {
                CompiledRule::Standard { regex, replacement, .. } => {
                    regex.replace_all(&owned, replacement.as_str()).to_string()
                }
                CompiledRule::Fancy { regex, replacement, .. } => {
                    regex.replace_all(&owned, replacement.as_str())
                        .map_err(|e| {
                            log::warn!("fancy-regex replace failed: {}", e);
                            owned.clone() // 出错时返回原文
                        })
                        .unwrap_or(owned)
                }
            };
            result = Cow::Owned(replaced);
        }
    }

    result
}
```

### 7.4 关键测试用例（开发前必读）

```
TC-01: 日语连续术语 — 验证无"消费效应"
  输入:  "ポリウレタン、過酸化物を含む"
  原始:  (ポリウレタン 正确，過酸化物 写成了 かさんかぶつ)
  规则:  "かさんかぶつ" → "過酸化物" (ja, whole_word=true)
  期望:  "ポリウレタン、過酸化物を含む"
  说明:  两个术语被单个逗号分隔，第二个术语应正确匹配。

TC-02: 日语 — 术语在行首/行尾
  输入:  "過酸化物は危険です"
  期望:  正常匹配，不被"は"干扰

TC-03: 中文 — 短术语不命中长术语
  输入:  "甲苯二异氰酸酯的生产工艺"
  规则:  "甲苯" → 不命中（因为后面是"二"，即 \p{Han}）
  期望:  不替换

TC-04: 中文 — 形近术语正确匹配
  输入:  "使用甲本二亿情酸纸作为原料"
  规则:  "甲本二亿情酸纸" → "甲苯二异氰酸酯"
  期望:  正确替换

TC-05: 日语浊音变体
  输入:  "ポリウレタンとボリウレタンの混合"
  规则:  "ボリウレタン" → "ポリウレタン"
  期望:  仅替换"ボリウレタン"，"ポリウレタン"不受影响

TC-06: 混合语言 — 英语术语在日语语境中
  输入:  "このTDIは危険です"
  规则:  "TDI" → 保留
  期望:  英语术语不误替换
```

### 7.5 性能基准

> 以下为预估值，以 Phase 1 实现后的实际测量（测试矩阵：术语 50/200/500 × 语言 ja/zh/en/mix × chunk 100/500/2000 chars × 设备低/中/高配）为准。

| 规则数 | 文本长度 | whole_word 占比 | 引擎分布 | 预估耗时 |
|--------|----------|:---:|------|----------|
| 50 | 200 chars | 30% 日/中 | 35 regex + 15 fancy | < 5ms |
| 200 | 500 chars | 40% 日/中 | 120 regex + 80 fancy | < 30ms |
| 500 | 2000 chars | 50% 日/中 | 250 regex + 250 fancy | < 100ms |

> **熔断**：单次 > 100ms → 降级为仅 high 优先级规则，UI 警告。

### 7.6 L2 可关闭性

- **全局**：`terminology_enabled = 0`
- **按术语包**：`package_id` 级批量禁用（`UPDATE terminology SET enabled=0 WHERE package_id=?`）
- **调试模式**：关闭后对比 raw 和 L2 后文本

---

## 8. 第三级：LLM 深度校正建议

### 8.1 模式定义：仅纠错，不扩写

| 操作 | 纠错模式（V1） | 扩写模式（Future） |
|------|:---:|:---:|
| 修正明显 STT 识别错误 | ✅ | ✅ |
| 统一术语表记（IUPAC 优先） | ✅ | ✅ |
| 将 `TDI` 展开为 `トルエンジイソシアネート (TDI)` | ❌ | ✅ |
| 添加原文没有的解释 | ❌ | ❌ |

### 8.2 LLM 任务队列

```rust
use tokio::sync::Semaphore;

/// 全局 LLM 校正任务队列 — 严格限制并发数为 1，防止本地模型 OOM
static LLM_CORRECTION_QUEUE: LazyLock<Semaphore> =
    LazyLock::new(|| Semaphore::new(1));

async fn enqueue_llm_correction(meeting_id: String) -> Result<(), String> {
    // 尝试获取许可（非阻塞检查，用于返回排队状态给前端）
    let permit = match LLM_CORRECTION_QUEUE.try_acquire() {
        Ok(p) => p,
        Err(_) => {
            // 队列已满（已有任务在运行）→ 返回 queued 状态
            return Ok(()); // 实际等待逻辑在 spawn 的 task 中
        }
    };

    // 超时：60s，超时后放弃本次校正
    let result = tokio::time::timeout(
        Duration::from_secs(60),
        run_llm_correction(&meeting_id),
    ).await;

    drop(permit); // 释放许可
    result.map_err(|_| "LLM correction timed out".to_string())?
}
```

### 8.3 L3 任务恢复（App 重启）

如果用户在 L3 运行期间关闭了 App，任务会被丢弃。恢复机制：

```rust
// 在 lib.rs 的 setup 闭包中
tauri::async_runtime::spawn(async move {
    // 查询是否有 meetings 在录音停止后未完成 L3 校正
    let pending_meetings = db.get_meetings_pending_l3_correction().await
        .unwrap_or_default();

    for meeting_id in pending_meetings {
        log::info!("Recovering L3 correction for meeting: {}", meeting_id);
        let db = db.clone();
        tauri::async_runtime::spawn(async move {
            if let Err(e) = terminology::queue::enqueue_llm_correction_task(
                &db, &meeting_id
            ).await {
                log::warn!("Failed to recover L3 correction for {}: {}", meeting_id, e);
            }
        });
    }
});
```

**前端状态流**：
```
idle → queued(N) → running → done → (user review)
                              ↘ failed → (retry available)
```

### 8.4 L3 建议数据结构

```rust
/// L3 单条校正建议
/// 作为"查找替换规则"存储，前端在全文范围内全局高亮所有匹配
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct L3CorrectionSuggestion {
    pub id: String,
    pub meeting_id: String,

    // 查找替换：LLM 仅返回 original_span 和 suggested_text
    // Rust 后端通过 str::find 计算所有出现位置
    pub original_span: String,      // 查找文本
    pub suggested_text: String,     // 替换文本

    // 后端填充（不在 LLM 输出中）
    pub occurrences: Vec<CharRange>, // 所有匹配位置
    // CharRange = { start: usize, end: usize }  // Unicode scalar value 索引

    pub language: String,
    pub correction_type: String,
    pub reason: String,

    // 版本追踪
    pub source_snapshot_hash: String, // 建议生成时的 normalized_transcript 哈希

    pub status: String,               // pending | accepted | rejected | obsolete
    pub reviewed_by: Option<String>,
    pub reviewed_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharRange {
    pub start: usize,  // Unicode scalar value 索引（非字节索引）
    pub end: usize,
}
```

### 8.5 Offset 语义定义（跨 Rust/JS 一致性）

**统一采用 Unicode scalar value 计数**（Rust `char` 的索引）。

| 环境 | 获取方式 | 与 Unicode scalar value 索引的关系 |
|------|----------|------|
| Rust | `s.char_indices()` | **原生一致** |
| JavaScript | `[...str]` 展开后索引 | **一致**（ES6 迭代器按 code point） |
| JavaScript | `String.length` | **不一致**（按 UTF-16 code unit，emoji/生僻字会错位） |

**规范**：
- `start` / `end`：Unicode scalar value 从 0 开始计数
- Rust 端：使用 `s.chars().take(start).collect::<String>().len()` 转换为字节offset后操作
- 前端：使用 `[...str].slice(start, end).join('')` 高亮
- 前端**禁止**使用 `String.slice()` 按 UTF-16 code unit 索引

### 8.6 多出现次数的处理策略

由于 LLM 不返回字符偏移量，且同一错误词可能在文中出现多次：

1. **后端**：LLM 返回 `{"original": "H二二五", "suggested": "H225"}` 后，Rust 使用 `str::find` 找到**所有**匹配位置
2. **前端**：展示为"`H二二五` → `H225`（全文共 N 处）"，用户可选择：
   - 一键接受全部 N 处（推荐默认）
   - 展开逐条查看上下文后分别处理
3. **特殊情况**：如果用户只想修正其中部分出现，需手动编辑（此场景罕见）

### 8.7 LLM Provider 与降级链

| Provider | 语言能力 | 延迟 | 内存 | 场景 |
|----------|:---:|------|------|------|
| Ollama `qwen2.5:7b` | 中/英优，日可接受 | 2-5s | ~5GB | **默认** |
| Ollama `qwen2.5:3b` | 中/英可接受，日弱 | 1-2s | ~2.5GB | 低内存降级 |
| Ollama `qwen2.5:14b` | 日/中/英均优 | 3-8s | ~9GB | 高配可选 |

降级链：7b → (60s 超时) → 3b → (失败) → 静默放弃 + 日志记录。

### 8.8 前端交互：按术语聚类

```
┌──────────────────────────────────────────────────────────────┐
│  L3 深度校正建议 (共 23 条, 涉及 5 个术语)      状态: ✅ 已完成 │
│                                                              │
│  ┌─ 🟡 化学名: ポリウレタン (全文 8 处) ────────────────┐   │
│  │  "ポリ ウレ たん" → "ポリウレタン"                    │   │
│  │  出现位置: 00:03:15, 00:07:42, 00:12:08, ...         │   │
│  │  [一键接受全部 8 处]  [逐条查看上下文]  [全部拒绝]    │   │
│  └──────────────────────────────────────────────────────┘   │
│                                                              │
│  ┌─ 🔵 GHS代码: H225 (全文 5 处) ──────────────────────┐   │
│  │  "H二二五" → "H225"   [一键接受全部 5 处]              │   │
│  └──────────────────────────────────────────────────────┘   │
│                                                              │
│  [接受全部 23 条]  [拒绝全部]                                │
└──────────────────────────────────────────────────────────────┘
```

### 8.9 auto_accept 策略

`llm_correction_auto_accept` 字段保留在数据模型和 settings 表中，但**第一版不在 UI 中暴露此配置项**。默认值强制为 `false`。仅在后续版本中根据用户反馈和法律合规审查结果决定是否开放。

### 8.10 L3 接受后触发摘要重新生成

用户接受 L3 校正建议后（`normalized_transcript` 被更新），前端提示："转录文本已更新，是否重新生成会议摘要？" 用户可以手动触发或忽略。

---

## 9. 与现有录音停止链路的集成

### 9.1 当前链路（基于代码库分析）

```
recording_manager.rs: stop_recording()
  → emit("recording-stopped", { meeting_id })

前端 page.tsx: 监听 "recording-stopped"
  → 获取聚合的转录文本
  → invoke("save_transcript", { meeting_id, transcript_text })
  → invoke("generate_summary", { meeting_id })
```

### 9.2 术语校正集成后的链路

```
recording_manager.rs: stop_recording()
  → emit("recording-stopped", { meeting_id })

前端 page.tsx:
  │
  ├─► Step 1: 保存（同步等待）
  │     const raw = aggregatedRawTranscripts   // 前端聚合 raw_text
  │     const normalized = aggregatedDisplayTexts // 前端聚合 text
  │     const hash = await invoke("compute_terminology_snapshot_hash")
  │     await invoke("save_transcript_with_terminology", {
  │         meeting_id,
  │         raw_transcript: raw,
  │         transcript: normalized,
  │         terminology_snapshot_hash: hash,
  │     })
  │
  ├─► Step 2: 异步触发 L3（不阻塞 UI）
  │     const { status } = await invoke("run_llm_terminology_correction", {
  │         meeting_id,
  │     })
  │     // status: "running" | "queued" — L3 已入队
  │
  └─► Step 3: 监听 L3 完成
        listen("llm-corrections-ready", (payload) => {
          setCorrections(payload.suggestions)
        })
```

### 9.3 职责划分

| 职责 | 负责方 | 说明 |
|------|:---:|------|
| 聚合转录 chunks（raw + normalized 双轨） | 前端 | 前端持有每个 chunk 的 raw 和 corrected 版本 |
| 计算术语快照哈希 | Rust | 基于当前术语表 SHA-256 |
| 保存双版本至 DB | Rust | `save_transcript_with_terminology` 命令 |
| 发起 L3 任务 | 前端 | 调用后立即返回队列状态 |
| L3 任务调度 | Rust | Semaphore(1) + 超时 + 启动恢复 |
| 通知 L3 完成 | Rust → 前端 | Tauri event `llm-corrections-ready` |
| 摘要生成 | Rust | 默认读 `transcript`（L1+L2 后）。L3 接受后前端提示可重新生成 |

---

## 10. TranscriptUpdate 事件扩展

### 10.1 当前结构

```rust
// worker.rs:27 (当前)
pub struct TranscriptUpdate {
    pub text: String,              // STT 输出（当前无 L2 前的版本）
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
// worker.rs:27 (修改后)
pub struct TranscriptUpdate {
    // === 现有字段（保持不变，向后兼容）===
    pub text: String,              // 展示文本（L2 校正后）
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
    pub raw_text: Option<String>,  // STT 原始输出（L1 后、L2 前）
                                   // None = 术语校正总开关关闭时
    #[serde(skip_serializing_if = "Option::is_none")]
    pub corrections_applied: Option<u32>, // 本 chunk 被 L2 替换的次数
}
```

### 10.3 前端双轨聚合

```typescript
// 监听 transcript-update 事件
listen<TranscriptUpdate>('transcript-update', (event) => {
  // 用于实时显示
  setDisplayTranscripts(prev => [...prev, event.payload.text]);

  // 用于最终保存的 L0 原始文本
  if (event.payload.raw_text !== undefined) {
    setRawTranscripts(prev => [...prev, event.payload.raw_text!]);
  } else {
    // 兼容：术语校正关闭时，raw = text
    setRawTranscripts(prev => [...prev, event.payload.text]);
  }
});
```

### 10.4 worker.rs 中的集成位置

```rust
// Whisper 分支（line ~456）:
let raw_text = text.trim().to_string();       // L0: 原始 STT
let corrected = apply_terminology_correction(&raw_text, &rules);
let display_text = corrected.into_owned();

// Parakeet 分支（line ~491）:
let raw_text = text.trim().to_string();
let corrected = apply_terminology_correction(&raw_text, &rules);
let display_text = corrected.into_owned();

// emit:
let update = TranscriptUpdate {
    text: display_text,                         // 展示文本（向后兼容）
    raw_text: Some(raw_text),                   // 原始文本（新增）
    corrections_applied: Some(count),           // 替换次数（新增）
    // ... 其余字段不变
};
```

---

## 11. 数据库设计

### 11.1 新建表：`terminology`

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

    -- 术语来源与包治理（V3.2 新增）
    source_type      TEXT NOT NULL DEFAULT 'manual',  -- 'preset' | 'imported' | 'manual'
    package_id       TEXT,              -- 预置包标识或导入批次 ID
    package_name     TEXT,              -- 人类可读的包名称
    import_batch_id  TEXT,              -- CSV 导入批次 ID（用于批次回滚）

    created_at       TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at       TEXT NOT NULL DEFAULT (datetime('now')),

    UNIQUE(original, language)  -- CSV 导入冲突时的 upsert 键
);

CREATE INDEX IF NOT EXISTS idx_terminology_language ON terminology(language);
CREATE INDEX IF NOT EXISTS idx_terminology_enabled ON terminology(enabled);
CREATE INDEX IF NOT EXISTS idx_terminology_package ON terminology(package_id);
CREATE INDEX IF NOT EXISTS idx_terminology_source ON terminology(source_type);
```

### 11.2 新建表：`transcript_corrections`

```sql
CREATE TABLE IF NOT EXISTS transcript_corrections (
    id                  TEXT PRIMARY KEY,
    meeting_id          TEXT NOT NULL,
    original_span       TEXT NOT NULL,        -- 查找文本
    suggested_text      TEXT NOT NULL,        -- 替换文本
    occurrences_json    TEXT,                 -- JSON: [{start, end}, ...] — 所有匹配位置
    language            TEXT,
    correction_type     TEXT NOT NULL DEFAULT 'llm',
    correction_level    TEXT NOT NULL DEFAULT 'l3',
    reason              TEXT,
    source_snapshot_hash TEXT,               -- normalized_transcript 在建议生成时的哈希
    status              TEXT NOT NULL DEFAULT 'pending',
    reviewed_by         TEXT,
    reviewed_at         TEXT,
    created_at          TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (meeting_id) REFERENCES meetings(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_corrections_meeting ON transcript_corrections(meeting_id);
CREATE INDEX IF NOT EXISTS idx_corrections_status ON transcript_corrections(status);
```

### 11.3 现有表扩展（幂等迁移）

```sql
-- transcripts 表新增:
--   raw_transcript TEXT          — L0: 原始 STT 输出（不可变）
--   terminology_snapshot_hash TEXT — 保存时的术语表 SHA-256
--   l1_prompt_snapshot TEXT      — L1 注入的完整 prompt（审计用，不写入常规日志）

-- settings 表新增:
--   terminology_enabled INTEGER DEFAULT 1
--   initial_prompt_enabled INTEGER DEFAULT 1
--   llm_correction_enabled INTEGER DEFAULT 1
--   llm_correction_auto_accept INTEGER DEFAULT 0  — 第一版 UI 不暴露
```

所有 ALTER TABLE 操作通过 `add_column_if_not_exists()` 幂等执行：

```rust
async fn add_column_if_not_exists(
    pool: &SqlitePool, table: &str, column: &str, definition: &str,
) -> Result<()> {
    let count: (i64,) = sqlx::query_as(&format!(
        "SELECT COUNT(*) FROM pragma_table_info('{}') WHERE name = '{}'",
        table, column
    )).fetch_one(pool).await?;
    if count.0 == 0 {
        sqlx::query(&format!(
            "ALTER TABLE {} ADD COLUMN {} {}", table, column, definition
        )).execute(pool).await?;
    }
    Ok(())
}
```

### 11.4 数据模型（Rust）

```rust
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct TerminologyEntry {
    pub id: String,
    pub original: String,
    pub replacement: String,
    pub language: String,
    pub case_sensitive: bool,
    pub whole_word: bool,
    pub enabled: bool,
    pub priority: String,
    pub category: String,
    pub description: Option<String>,
    pub source_type: String,        // 'preset' | 'imported' | 'manual'
    pub package_id: Option<String>,
    pub package_name: Option<String>,
    pub import_batch_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct TranscriptCorrection {
    pub id: String,
    pub meeting_id: String,
    pub original_span: String,
    pub suggested_text: String,
    pub occurrences_json: Option<String>,  // JSON of Vec<CharRange>
    pub language: Option<String>,
    pub correction_type: String,
    pub correction_level: String,
    pub reason: Option<String>,
    pub source_snapshot_hash: Option<String>,
    pub status: String,
    pub reviewed_by: Option<String>,
    pub reviewed_at: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharRange {
    pub start: usize,  // Unicode scalar value 索引
    pub end: usize,
}
```

---

## 12. 后端实现规范

### 12.1 模块组织结构

```
frontend/src-tauri/src/
├── terminology/
│   ├── mod.rs
│   ├── cache.rs                     # 统一缓存管理
│   │   ├── INITIAL_PROMPT_BY_LANG   #
│   │   ├── TERMINOLOGY_RULES        #   Vec<CompiledRule>（混合 regex + fancy-regex）
│   │   ├── refresh_all_caches()     #   原子刷新 + 计算 snapshot_hash
│   │   ├── compute_snapshot_hash()  #   SHA-256 of enabled terminology entries
│   │   ├── get_initial_prompt()     #   含精确 token 计数 + 截断
│   │   └── get_terminology_rules()  #
│   ├── commands.rs                  # Tauri 命令
│   ├── corrector.rs                 # 校正器
│   │   ├── apply_terminology_correction()   # L2（混合引擎）
│   │   ├── validate_chemical_codes()        # L3 后规则验证
│   │   └── build_term_rule()                # 规则编译
│   ├── queue.rs                     # L3 任务队列
│   │   ├── LLM_CORRECTION_QUEUE      #   Semaphore(1)
│   │   ├── enqueue_llm_correction()  #   入队 + 超时
│   │   └── recover_pending_tasks()   #   启动时恢复未完成的任务
│   └── snapshot.rs                  # 术语快照管理
│
├── whisper_engine/
│   └── whisper_engine.rs           # L1 集成
│
├── audio/transcription/worker.rs   # L2 集成 + TranscriptUpdate 扩展
│
├── summary/llm_client.rs           # L3: suggest_corrections()
│
├── database/
│   ├── models.rs                   # 追加模型
│   ├── repositories/
│   │   └── terminology.rs          # 新建（含 package 级查询）
│   └── setup.rs                    # 幂等迁移 + L3 任务恢复
│
├── lib.rs                          # 注册命令
│
└── migrations/
    └── 20260427000000_add_terminology.sql
```

### 12.2 Tauri 命令注册

```rust
// 术语管理 CRUD
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

// L3 LLM 校正
terminology::commands::run_llm_terminology_correction,
terminology::commands::get_llm_queue_status,
terminology::commands::get_corrections_for_meeting,
terminology::commands::accept_correction,
terminology::commands::accept_correction_for_term,  // 按术语全量接受
terminology::commands::reject_correction,

// 保存（含 raw + snapshot_hash）
database::commands::save_transcript_with_terminology,

// 设置
terminology::commands::get_terminology_settings,
terminology::commands::set_terminology_settings,
```

### 12.3 启动时初始化

```rust
// lib.rs setup 闭包
let db = app.state::<state::AppState>().db_manager.clone();

tauri::async_runtime::spawn(async move {
    // 1. 初始化术语缓存（L1 + L2 + snapshot hash）
    if let Err(e) = terminology::cache::refresh_all_caches(&db).await {
        log::warn!("Terminology caches init failed: {}", e);
    }

    // 2. 恢复未完成的 L3 任务
    if let Err(e) = terminology::queue::recover_pending_tasks(&db).await {
        log::warn!("L3 task recovery failed: {}", e);
    }
});
```

---

## 13. 前端实现规范

### 13.1 新增组件

| 组件 | 路径 | 说明 |
|------|------|------|
| `TerminologyManager` | `components/TerminologyManager/index.tsx` | 术语管理主面板（含包筛选） |
| `TerminologyImportDialog` | `components/TerminologyManager/ImportDialog.tsx` | CSV 导入（含冲突预览） |
| `CorrectionDiffView` | `components/CorrectionDiff/DiffView.tsx` | 按术语聚类的校正差异视图 |
| `L3QueueStatus` | `components/CorrectionDiff/QueueStatus.tsx` | L3 任务状态指示器 |

### 13.2 TypeScript 类型定义

```typescript
// frontend/src/types/terminology.ts

export type TerminologyLanguage = 'ja' | 'zh' | 'en' | 'auto';
export type TerminologySource = 'preset' | 'imported' | 'manual';

export interface TerminologyEntry {
  id: string;
  original: string;
  replacement: string;
  language: TerminologyLanguage;
  caseSensitive: boolean;
  wholeWord: boolean;
  enabled: boolean;
  priority: 'high' | 'normal' | 'low';
  category: string;
  description?: string;
  sourceType: TerminologySource;
  packageId?: string;
  packageName?: string;
  importBatchId?: string;
  createdAt: string;
  updatedAt: string;
}

export interface TranscriptUpdate {
  // 现有字段
  text: string;
  timestamp: string;
  source: string;
  sequence_id: number;
  chunk_start_time: number;
  is_partial: boolean;
  confidence: number;
  audio_start_time: number;
  audio_end_time: number;
  // 新增字段
  raw_text?: string;          // L0: STT 原始输出
  corrections_applied?: number; // L2 替换次数
}

export interface CharRange {
  start: number;  // Unicode scalar value 索引
  end: number;
}

export interface TranscriptCorrection {
  id: string;
  meetingId: string;
  originalSpan: string;
  suggestedText: string;
  occurrences: CharRange[];       // 所有匹配位置
  language?: TerminologyLanguage;
  correctionType: string;
  reason?: string;
  sourceSnapshotHash?: string;
  status: 'pending' | 'accepted' | 'rejected' | 'obsolete';
  reviewedBy?: string;
  reviewedAt?: string;
  createdAt: string;
}

/** 按术语聚类后的展示结构 */
export interface ClusteredSuggestions {
  term: string;
  language: TerminologyLanguage;
  count: number;
  suggestions: TranscriptCorrection[];
}

export type L3QueueStatus = 'idle' | 'queued' | 'running' | 'done' | 'failed';
```

### 13.3 前端双轨聚合

参见第 10.3 节的实现示例。关键点：前端维护两个聚合 buffer（`rawTranscripts` 和 `displayTranscripts`），在停止录音时分别提交。

---

## 14. 配置与存储

### 14.1 配置项

| 配置项 | 默认值 | V1 UI 暴露 | 说明 |
|--------|:---:|:---:|------|
| `terminology_enabled` | 1 | ✅ | 总开关（含 L2） |
| `initial_prompt_enabled` | 1 | ✅ | L1 开关（仅对 Whisper） |
| `llm_correction_enabled` | 1 | ✅ | L3 建议生成开关 |
| `llm_correction_auto_accept` | **0** | **❌** | V1 不暴露，强制为 false |

### 14.2 CSV 导入/导出

**编码支持分阶段**：
- MVP：仅 UTF-8 BOM
- Phase 1B：Shift-JIS 自动检测（前端预览前 5 行）

**冲突键**：`(original, language)` 联合唯一。导入时 upsert。导入预览显示："将新增 X 条，覆盖 Y 条"。

**导入批次追踪**：每次 CSV 导入生成 `import_batch_id`，可在 `terminology` 表中按批次查询和回滚。

### 14.3 内置预置术语包

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

## 15. 硬件要求与降级策略

### 15.1 最低配置

| 组件 | 最低配置 | 推荐配置 |
|------|----------|----------|
| RAM | 8GB (L1+L2) | 16GB+ (含 L3) |
| 磁盘 | 500MB | 3-5GB (含 LLM 模型) |

### 15.2 降级策略（推荐策略，用户手动配置）

| 条件 | L1 | L2 | L3 |
|------|:---:|:---:|:---:|
| RAM < 8GB | ❌ | ✅ | ❌ |
| RAM 8-12GB | ✅ | ✅ | 尝试 3B |
| RAM > 12GB | ✅ | ✅ | ✅ (默认 7B) |
| 电池供电 | ✅ | ✅ | ❌ (可手动开启) |

> 自动硬件检测和电源状态感知在后续版本中完善。第一版以设置页面中的推荐策略形式呈现。

---

## 16. 实施计划与 MVP 策略

### 16.1 分阶段交付

| 阶段 | 内容 | 交付价值 | 工作量 |
|------|------|----------|:---:|
| **Phase 0** | 基线测量 + 5 项技术验证 | 数据驱动决策 + 方案可行性确认 | 1-2 人天 |
| **Phase 1A (MVP)** | L2（混合引擎）+ DB（含 raw/snapshot/package）+ TranscriptUpdate 双轨扩展 + 基础 UI | ~50% 覆盖，审计链建立 | 3-4 人天 |
| **Phase 1B** | L1 initial_prompt（如 API 可用）+ CSV 导入（UTF-8 BOM）+ 精确 token 计数 | 追加 ~15-20% | 2 人天 |
| **Phase 2** | L3 LLM 建议（队列+恢复+聚类 UI）+ 差异对比 | 追加 ~10-20% | 3-4 人天 |
| **Phase 3** | 完善：Shift-JIS、审计报告、摘要重新生成提示、术语包升级/回滚 | 合规与治理 | 2-3 人天 |

**总计：11-15 人天**

### 16.2 MVP（Phase 1A）核心范围

最小可交付：
1. `terminology` 表（含 source_type/package_id）+ `raw_transcript` + `terminology_snapshot_hash` + 幂等迁移
2. L2 混合引擎（regex + fancy-regex）+ `worker.rs` 集成
3. `TranscriptUpdate` 双轨扩展（raw_text + text）
4. 前端双轨聚合 + 保存命令适配
5. 基础术语管理 UI（表格 + 新增/删除/语言筛选 + 包级禁用）
6. 预置术语包（精简版 50 条）

---

## 17. 测试策略

### 17.1 关键测试用例（开发前必过）

```
✅ TC-01: 日语连续术语 — 逗号分隔的两个术语均被正确匹配（验证无消费效应）
✅ TC-02: 日语 — 术语在行首/行尾正确匹配
✅ TC-03: 中文 — 短术语不命中长术语（"甲苯"不命中"甲苯二异氰酸酯"）
✅ TC-04: 中文 — 同音字变体被正确替换
✅ TC-05: 日语浊音变体 — 仅替换错误变体，正确拼写不受影响
✅ TC-06: 混合语言 — 英语术语在日语语境中不误替换
✅ TC-07: Regex vs Fancy — whole_word 英语走 regex，日语走 fancy-regex
✅ TC-08: Cow<str> — 无匹配时不分配字符串
✅ TC-09: TranscriptUpdate — raw_text 和 text 分别携带正确的值
✅ TC-10: L3 队列 — 两个并发任务串行执行，不 OOM
✅ TC-11: L3 恢复 — App 重启后未完成的任务被重新入队
✅ TC-12: L3 版本冲突 — 用户手动修改原文后，旧建议标记 obsolete
✅ TC-13: 术语快照哈希 — 修改术语表后哈希值变化
✅ TC-14: 幂等迁移 — 重复执行不报错
✅ TC-15: Offset 一致性 — Rust char_indices 和 JS [...str] 对同一文本返回相同索引
```

### 17.2 验收标准

- [ ] `raw_transcript` 强制保留且不可变，`TranscriptUpdate` 双轨数据正确
- [ ] `terminology_snapshot_hash` 被保存，修改术语表后哈希变化
- [ ] L2 日语连续术语"消费效应"已验证修复（TC-01）
- [ ] fancy-regex 编译通过，日/中全词匹配正确
- [ ] 英语、子串匹配仍使用标准 regex（无性能退化）
- [ ] L1 精确 token 计数（或回退估算）+ 截断 + 前端超载警告
- [ ] L1 prompt 完整快照写入 `l1_prompt_snapshot`，不写入常规日志
- [ ] L3 经 Semaphore(1) 串行队列，App 重启可恢复
- [ ] L3 建议按"查找替换"存储，前端展示全文匹配次数
- [ ] `char_offset` 统一为 Unicode scalar value 索引，Rust/JS 行为一致
- [ ] 按术语聚类 + 批量操作的 UI 可用
- [ ] `auto_accept` 字段存在但 V1 UI 不暴露
- [ ] 术语包级启用/禁用 (`package_id` 粒度)
- [ ] CSV 导入显示冲突预览 + 批次追踪
- [ ] 校正审计日志完整可追溯

---

## 18. 风险与应对

| 风险 | 影响 | 概率 | 应对 |
|------|------|:---:|------|
| whisper-rs 未暴露 `set_initial_prompt` | L1 无法实现 | 中 | Phase 0 验证。若无 → 废弃 L1，仅 L2+L3 |
| `fancy-regex` 编译/性能问题 | L2 日/中全词匹配不可用 | 低 | Phase 0 预研。若不可用 → 日/中仅支持子串匹配（非全词），标注为已知限制 |
| L2 连续术语仍有边缘遗漏 | 极少数术语未匹配 | 低 | 已修复消费效应。若仍有罕见遗漏，后续版本增加 multi-pass 重放 |
| 两场会议连续结束，L3 任务堆积 | UI 显示排队 | 中 | 串行队列 + 60s 超时 + 前端显示排队状态 |
| 用户在 L3 运行期间关闭 App | 任务中断 | 中 | 启动时自动恢复未完成的 L3 任务 |
| 用户在 L3 生成期间手动编辑文本 | 建议过时 | 中 | snapshot_hash 版本检测 → 标记 obsolete |
| 日/中/英混合时 LLM 语义错乱 | L3 建议质量差 | 中 | 仅纠错模式（减少自由度）+ 用户逐条确认 |
| 危化品术语校正涉及合规风险 | 安全信息错误 | **高** | raw 不可变 + L3 默认仅建议 + auto_accept 不暴露 + 完整审计日志 |
| 术语表随时间变化导致旧记录不可回放 | 审计链断裂 | 低 | `terminology_snapshot_hash` 保留在每条 transcript 中 |

---

## 19. 合规与法务审查

| 节点 | 时机 | 内容 |
|------|------|------|
| Gate A | Phase 1A 后 | raw 保留 + L2 确定性的审计兼容性 |
| Gate B | Phase 2 上线前 | L3 建议模式（仅建议、需确认、可追溯） |
| Gate C | Phase 3 后 | 审计报告格式是否满足行业监管 |

**核心原则**：
1. raw_transcript 不可变，完整保留 STT 原始输出
2. L2 确定性替换可自动应用，所有替换可重放验证
3. L3 非确定性建议默认仅建议，auto_accept V1 不暴露
4. 所有校正操作记录操作者、时间戳、前后文本、术语快照哈希

---

## 20. 回滚与功能淘汰

- **总开关**：`terminology_enabled = 0` → 恢复原始行为
- **分级回滚**：L1 / L3 可独立关闭
- **术语包回滚**：通过 `import_batch_id` 回滚单次导入
- **raw_transcript 永远不变**：作为最终回滚锚点
- **性能熔断**：L2 > 100ms → 仅 high 规则；L3 > 60s → 超时放弃

---

## 附录 A：成功指标体系

| 指标 | 方法 | 目标 |
|------|------|:---:|
| L2 误替换率 | 人工审核 100 条 L2 替换样本 | < 1% |
| L3 建议接受率 | 用户实际接受/拒绝比例 | > 70% |
| 用户人工复核时长 | 对比有无术语校正时的审核耗时 | 下降 > 30% |
| 术语准确率提升 | 与 Phase 0 基线对比 | 基于基线设定 |
| L2 连续术语遗漏率 | TC-01 类测试 | 0% |

---

## 附录 B：V3.1 → V3.2 变更摘要

| 变更项 | V3.1 | V3.2 | 触发来源 |
|--------|------|------|------|
| **L2 引擎选择** | 全部使用捕获组（有消费效应 bug） | **混合引擎**：英/子串=regex，日/中全词=fancy-regex（look-around） | Gemini P0：消费效应 |
| **L2 示例代码** | 日语边界字符类嵌套错误，中文捕获组数量与模板不匹配 | **修正为正确的模式构建和模板**，日语/中文各自独立处理 | GPT P0-1 |
| **TranscriptUpdate 事件** | 未描述扩展方式 | **新增第 10 节**：具体字段扩展（raw_text/ corrections_applied）+ 前端双轨聚合代码 | GPT P1-1 |
| **L3 多出现次数** | 仅用 str::find 找第一个 | **查找替换模式**：后端找所有位置，前端展示"全文 N 处" | Gemini：偏移量错位 |
| **Offset 语义** | 未精确定义 | **Unicode scalar value 索引** + Rust char_indices ↔ JS [...str] 对齐 + 禁止 String.length | GPT P1-4 |
| **术语快照版本** | 未包含 | **terminology_snapshot_hash** 存入每条 transcript，支持事后审计回放 | GPT P1-2 |
| **术语包治理字段** | 仅 category/priority | **新增 source_type / package_id / package_name / import_batch_id** | GPT P1-3 |
| **L1 token 计数** | 粗略估算 | **tiktoken-rs 精确 BPE 计数** + 回退估算 | Gemini |
| **L1 prompt 存储** | 写入常规日志 | **不写入日志**，存入 `l1_prompt_snapshot` 字段 | GPT P2-1 |
| **L3 任务恢复** | 仅提及优雅退出 | **启动时自动恢复**未完成的 L3 任务 | Gemini |
| **auto_accept** | 字段存在且默认 false | **V1 不暴露 UI**，字段保留仅为后续版本 | GPT P2-2 |
| **摘要重新生成** | 未提及 | **L3 接受后提示**用户可手动触发重新摘要 | GPT |
| **Phase 0 验证清单** | 2 项 | **5 项**：追加 fancy-regex 编译、tiktoken-rs、事件路径追踪 | 本轮新增 |

---

> **ドキュメントメンテナンス / 文档维护**：本 PRD V3.2 基于 V3.1 的 Gemini-3.1-Pro 和 GPT-4 双重技术审查后修订，修复了 L2 正则引擎"消费效应"这一致命逻辑缺陷，补充了 TranscriptUpdate 双轨扩展设计、术语快照版本审计、术语包治理数据模型、L3 任务恢复机制和跨语言 offset 语义精确定义。已具备交付研发进入技术详细设计和 POC 开发的条件。
