# PRD：转录专业术语定制化功能

> **文档状态**：V3.1（综合 Gemini + GPT 审查后修订）
> **创建日期**：2026-04-27
> **修订日期**：2026-04-27
> **关联项目**：Meetily v0.3.0
> **目标行业**：危险化学品制造业（日系企业）
> **支持语言**：日本語 / 中文 / English
> **核心目标**：使 Meetily 转录引擎支持用户自定义专业术语词库，通过三级管道（模型内提示 → 正则后处理 → LLM 深度校正建议）识别并纠正语音识别（STT）产生的术语拼写/识别错误。**原始 STT 输出强制保留，校正结果全程可追溯。**

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
10. [数据库设计](#10-数据库设计)
11. [后端实现规范](#11-后端实现规范)
12. [前端实现规范](#12-前端实现规范)
13. [配置与存储](#13-配置与存储)
14. [硬件要求与降级策略](#14-硬件要求与降级策略)
15. [实施计划与 MVP 策略](#15-实施计划与-mvp-策略)
16. [测试策略](#16-测试策略)
17. [风险与应对](#17-风险与应对)
18. [合规与法务审查](#18-合规与法务审查)
19. [回滚与功能淘汰](#19-回滚与功能淘汰)

---

## 1. 需求背景

### 1.1 业务场景

客户为一家在华日系危险化学品制造企业。日常会议特征：

| 特征 | 说明 |
|------|------|
| **多语言混合** | 会议中频繁切换日语、中文、英语，同一句话内常包含两种以上语言 |
| **高度专业化** | 涉及 MSDS（安全数据表）、CAS 编号、UN 危险货物编号、GHS 分类、化学物质 IUPAC 命名等 |
| **合规要求严格** | 转录文本用于内部审计与合规存档，术语准确性直接影响法律风险 |
| **三方沟通** | 日方技术人员（日语）、中方操作人员（中文）、国际供应商/客户（英语）共同参会 |

### 1.2 问题描述

Meetily 使用 Whisper / Parakeet 进行本地语音识别（STT）。在危化品行业的日企场景中存在以下叠加识别挑战：

1. **跨语言术语混乱**：模型在日/中/英切换时，容易将一种语言的发音"听成"另一种语言的文字。
2. **化学物质名称识别率极低**：IUPAC 命名和日文片假名化学名在通用 STT 训练语料中几乎不存在。
3. **安全编码格式特殊**：CAS RN、UN No.、GHS 危险代码等编码格式在语音转写中极易出错。
4. **片假名/汉字/罗马字混合**：日语化学术语同时使用多种文字系统，增加模型 token 预测难度。

### 1.3 用户故事

| 角色 | 需求 |
|------|------|
| 安全管理部门负责人 | 希望法定安全术语被正确转录，不可出现模糊或错误 |
| 工厂值班长（中文母语） | 希望中文术语不会被音近字替换 |
| 日本本社技术工程师 | 希望日语片假名术语被正确转写 |
| 国际采购对接人 | 希望英语 CAS 编号不会被逐字母拼写 |
| 合规审计员 | 希望原始 STT 输出完整保留，所有校正可追溯、可回放 |

### 1.4 目标

- **强制保留原始 STT 输出**，校正结果分层存储，证据链完整
- **多语言支持**：术语表支持日语（汉字/平假名/片假名）、中文（简/繁）、英语
- **三级校正管道**：模型内 `initial_prompt` 软引导 → 正则精确替换 → LLM 深度校正**建议**
- **用户可通过 UI 自定义行业专属术语表**
- **转录时实时应用前两级校正，录音停止后异步触发第三级校正建议**
- **L3 校正默认仅建议，需用户确认后生效**；第一版仅支持**纠错模式**（不包含术语扩写）

---

## 2. 核心问题分析

### 2.1 STT 输出的错误规律

#### 2.1.1 日语特有的错误模式

| 错误类型 | 示例 | 根因 |
|----------|------|------|
| 片假名 → 汉字误转 | `ポリウレタン` → `保利売れたん` | 模型对片假名化学术语的 token 覆盖不足 |
| 長音「ー」丢失 | `メチルエチルケトン` → 长音被省略 | whisper.cpp 对长音符 token 不敏感 |
| 促音「っ」丢失 | `引火性（いんかせい）` → `いんかせい` | 小さい「っ」的 token 在短音频段中易被丢弃 |
| 英语→片假名误回译 | 英文 `toluene` → `トルーエン`（多余长音） | 过度音译 |
| 浊音/半浊音混淆 | `ポリ` → `ボリ` 或 `ホリ` | 辅音 token 在噪声下歧义 |
| 拗音分割 | `メチル` → `メ チル` | 拗音 token 被拆分为两个假名 |

#### 2.1.2 中文特有的错误模式

| 错误类型 | 示例 | 根因 |
|----------|------|------|
| 同音字替换 | `甲苯二异氰酸酯` → `甲本二亿情酸纸` | 模型分不清化学术语的特殊汉字组合 |
| 数字+字母错位 | `H225` → `H二二五` | 中英数字混合 token 序列不稳定 |
| 多音字误读 | `重铬酸钾` → `重各酸钾` | "铬"是多音字 |
| 化学符号序列 | `NaOH` → `钠 O H` | 英文缩写在中文语境中的 token 歧义 |

#### 2.1.3 英语特有的错误模式（化学语境）

| 错误类型 | 示例 | 根因 |
|----------|------|------|
| IUPAC 命名拆分 | `2,4,6-Trinitrotoluene` → `two four six tri nitro toluene` | 数字+前缀的 token 序列不熟悉 |
| CAS 编号格式 | `CAS 108-88-3` → `k Ass one o eight eighty eight three` | 连字符导致 token 边界错误 |
| GHS 代码 | `H301` → `H three hundred one` | H+数字组合不在模型 token 表内 |
| MSDS 缩写 | `LD50` → `L D fifty` | 无上下文时缩写被逐字母展开 |

#### 2.1.4 跨语言混合特有的错误模式

| 错误类型 | 示例 | 根因 |
|----------|------|------|
| 日语→中文误转 | `この物質は引火性があります` → `这个物质是银华星游戏吗` | 语言切换点判断失误 |
| 中文→日语误转 | `闪点是负二十度` → `閃点は負にじゅうど` | 中→日 tokenizer 路径泄漏 |
| 英语→罗马音误转 | `flash point` → `フラッシュポイント` | 日英双语模式下的 token 竞争 |
| 代码混入自然语言 | `UN 1203` → `うん いちにーぜろさん` | 模型将代码视为日语假名发音 |

### 2.2 为什么不能把术语直接注入模型

- **Whisper（whisper.cpp）**：提供 `initial_prompt` 参数可偏置输出 token 分布，但这是**软引导**而非硬约束。prompt token 长度限制为 ~224 token，无法塞入完整术语表。
- **Parakeet（ONNX Runtime）**：完全不支持 prompt 或词典注入。

因此采用分层策略：`initial_prompt` 概率偏置 → 正则确定性替换 → LLM 上下文深度校正建议。

### 2.3 各级能力的边界

| 能力 | L1 initial_prompt | L2 正则 | L3 LLM 建议 |
|------|:---:|:---:|:---:|
| 已知变体精确替换 | ❌ (非确定性) | ✅ | ✅ |
| 未知变体识别 | ❌ | ❌ (需预定义) | ✅ |
| 跨语言上下文消歧 | ❌ | ❌ | ✅ |
| 日语长音符/促音修复 | ❌ | ✅ | ✅ |
| 化学编码格式恢复 | ❌ | ✅ | ✅ |
| 同音字语境消歧 | ⚠️ (概率偏置) | ❌ | ✅ |
| 性能 (单次) | <10ms (参数注入) | 见 7.5 节基准测试 | 2-10s |
| 成本 | 免费 | 免费 | 取决于模型 |
| 确定性 | 否 | 是 | 否 |
| 对 Parakeet 有效 | ❌ | ✅ | ✅ |

### 2.4 设计决策：三级校正管道

```
第一级: initial_prompt  →   推理时注入，软偏置 token 分布。
                           仅对 Whisper 引擎生效。Parakeet 用户跳过本级。

第二级: 正则后处理      →   模型输出后即时执行，确定性替换。对所有 STT 引擎生效。
                           使用捕获组模拟词边界（Rust regex 不支持 look-around）。

第三级: LLM 校正建议    →   录音停止后经任务队列串行执行（防止并发 OOM）。
                           默认仅建议，需用户确认。第一版仅支持纠错模式。
```

---

## 3. 总体架构设计

### 3.1 系统架构图

```
┌──────────────────────────────────────────────────────────────────────────────┐
│                          前端 (Next.js + React)                               │
│  ┌─────────────────────────┐    ┌──────────────────────────────────────────┐ │
│  │ 术语管理 UI              │    │ 转录查看 / 会议详情                        │ │
│  │ - 增删改查术语条目        │    │ - 实时转录面板 (L1+L2 校正后)              │ │
│  │ - 按语言分类管理          │    │ - L3 校正建议列表（按术语聚类）             │ │
│  │ - 导入/导出 CSV           │    │ - 差异高亮（raw vs L1+L2 vs L3 建议）    │ │
│  │ - L1 prompt 超载警告      │    │ - 逐条接受/拒绝 + 按术语批量操作           │ │
│  └───────────┬─────────────┘    └──────────────────────────────────────────┘ │
│              │ invoke()                                                       │
└──────────────┼───────────────────────────────────────────────────────────────┘
               │
┌──────────────┴───────────────────────────────────────────────────────────────┐
│                      Tauri IPC 层 (Rust Backend)                              │
│                                                                              │
│  ┌─────────────────────────┐    ┌──────────────────────────────────────────┐ │
│  │ 术语 CRUD 命令           │    │ 缓存管理（统一原子刷新）                    │ │
│  │ get/save/delete          │    │ refresh_all_terminology_caches()          │ │
│  │ import/export            │    │   → 原子性地刷新 INITIAL_PROMPT_CACHE    │ │
│  └───────────┬─────────────┘    │     + TERMINOLOGY_RULES                   │ │
│              │                  └──────────────┬───────────────────────────┘ │
│              ▼                                  ▼                             │
│  ┌──────────────────────────┐    ┌──────────────────────────────────────────┐ │
│  │ SQLite: terminology 表    │    │ 内存缓存（2 个缓存，单次原子刷新）           │ │
│  │ (持久化, 含 lang 字段)    │    │                                          │ │
│  │ UNIQUE(original, language)│   │  ┌──────────────────────────────────┐    │ │
│  └──────────────────────────┘    │  │ INITIAL_PROMPT_BY_LANG            │    │ │
│                                  │  │ LazyLock<RwLock<HashMap<String,   │    │ │
│                                  │  │   String>>                       │    │ │
│                                  │  └──────────────────────────────────┘    │ │
│                                  │                                          │ │
│                                  │  ┌──────────────────────────────────┐    │ │
│                                  │  │ TERMINOLOGY_RULES                  │    │ │
│                                  │  │ LazyLock<RwLock<Vec<               │    │ │
│                                  │  │   CompiledRule>>                  │    │ │
│                                  │  │ 按 original 字符数降序排列           │    │ │
│                                  │  │ 含捕获组感知的 replacement 模板     │    │ │
│                                  │  └──────────────────────────────────┘    │ │
│  └──────────────────────────┘    └──────────────┬───────────────────────────┘ │
│                                                 │                             │
│  ┌──────────────────────────────────────────────┴───────────────────────────┐ │
│  │              转录管道 (audio/transcription + whisper_engine)              │ │
│  │                                                                          │ │
│  │  每个音频 chunk（仅 Whisper 引擎走 L1）:                                   │ │
│  │       │                                                                  │ │
│  │       ├──► 【L1 - 仅 Whisper】initial_prompt 注入                         │ │
│  │       │    含 token 数检查，超出时按优先级+更新时间截断                       │ │
│  │       │                                                                  │ │
│  │       ├──► STT 模型推理 → 产生 raw_text（强制保留至 raw_transcript）         │ │
│  │       │                                                                  │ │
│  │       ├──► 【L2 - 所有引擎】apply_terminology_correction(raw_text)          │ │
│  │       │    捕获组模拟词边界；单次遍历；Cow<str> 优化；可独立关闭               │ │
│  │       │                                                                  │ │
│  │       └──► emit("transcript-update", corrected_text)                       │ │
│  └──────────────────────────────────────────────────────────────────────────┘ │
│                                                                              │
│  ┌──────────────────────────────────────────────────────────────────────────┐ │
│  │              录音后处理 (PostRecording Processor)                          │ │
│  │                                                                          │ │
│  │  recording-stopped (由 Rust recording_manager 发布)                        │ │
│  │       │                                                                  │ │
│  │       ├─► 前端发起保存请求                                                  │ │
│  │       │     ├─► raw_transcript（原始 STT 输出）→ 不可变，永久保留           │ │
│  │       │     └─► normalized_transcript（L1+L2 校正后）→ 当前展示版本         │ │
│  │       │                                                                  │ │
│  │       └─► 前端/后端异步触发 L3 校正建议任务                                  │ │
│  │             ├─► 经全局串行队列（Semaphore(1)），防止并发 OOM                  │ │
│  │             ├─► LLM 生成建议 → 每条含 offset / original / suggested         │ │
│  │             ├─► 对比 normalized_transcript 版本号，冲突时标记 obsolete      │ │
│  │             ├─► 保存至 transcript_corrections (status = "pending")         │ │
│  │             └─► emit("llm-corrections-ready") → 前端展示                   │ │
│  └──────────────────────────────────────────────────────────────────────────┘ │
└──────────────────────────────────────────────────────────────────────────────┘
```

### 3.2 数据流时序

```
用户点击开始录音
  │
  ├─► Rust: 原子加载 L1+L2 缓存（从 DB 单次加载，保证一致性）
  │        Whisper 引擎: params.set_initial_prompt(truncated_prompt)
  │        Parakeet 引擎: 跳过 L1
  │
  ├─► worker.rs: 转录循环
  │     │
  │     ├─► STT 引擎推理 → raw_text
  │     │
  │     ├─► 【L2 - 所有引擎】apply_terminology_correction(raw_text)
  │     │     └─► 捕获组模拟词边界替换（Rust regex 不支持 look-around）
  │     │
  │     └─► emit("transcript-update", corrected_text)
  │           → 前端聚合所有 chunk 为完整 raw_transcript 和 normalized_transcript

用户点击停止录音
  │
  ├─► recording_manager 发布 recording-stopped 事件
  │
  ├─► 前端监听事件 → invoke("save_transcript", { raw, normalized, meeting_id })
  │     ├─► 保存 raw_transcript（不可变）
  │     └─► 保存 normalized_transcript（L1+L2 校正后）
  │
  ├─► 前端 invoke("run_llm_terminology_correction", { meeting_id })
  │     └─► Rust: 将任务入队（全局 LLM 串行队列，Semaphore(1)）
  │           │
  │           ├─► 获取队列许可（前一个 L3 任务完成前阻塞等待）
  │           ├─► LLM 推理（支持超时 + 降级 + 重试）
  │           ├─► 每条建议: { offset, original_text, suggested_text, reason, lang }
  │           ├─► 版本比对: 检查 normalized_transcript 是否被用户手动修改
  │           ├─► 冲突 → 标记 obsolete; 无冲突 → 标记 pending
  │           └─► emit("llm-corrections-ready", { meeting_id, correction_count })
  │
  └─► 前端 polling 检测到完成 → 展示按术语聚类的差异对比
       用户逐条接受/拒绝，或按术语批量操作
```

---

## 4. 审计与证据链设计

> **核心原则**：原始 STT 输出是第一证据源，必须强制保留、不可修改。
> 所有后续处理结果分层存储，任何一层都可独立审计。

### 4.1 四层文本模型

| 层 | 字段/表 | 来源 | 可变性 | 用途 |
|---|---------|------|:---:|------|
| **L0 — 原始输出** | `transcripts.raw_transcript` | STT 引擎直接输出 | **不可变** | 审计锚点、误修正复盘 |
| **L1+L2 — 规范化** | `transcripts.transcript`（现有字段） | raw → L1(仅Whisper) → L2正则替换 | 可被 L3 建议接受后更新 | 实时显示、日常使用 |
| **L3 — 建议** | `transcript_corrections`（status=pending） | LLM 异步生成 | 可被接受/拒绝 | 深度校正候选 |
| **Final — 确认版** | 按 accepted corrections 重建 | 用户逐条确认后的合并结果 | 接受后写入 transcript | 归档导出、合规提交 |

### 4.2 不可变性保证

```rust
// transcripts 表：raw_transcript 写入后禁止 UPDATE
// 仅允许 INSERT 时设置，任何 UPDATE 语句不得修改此列
// 在 repository 层通过代码规范强制（SQLite 不支持列级权限）
```

### 4.3 审计追溯能力

任何时刻可以回答以下问题：

- 原始模型输出了什么？ → `raw_transcript`
- L1 initial_prompt 注入了什么？ → 日志中的 prompt 快照
- L2 做了哪些替换？ → 通过对比 raw 和 normalized 还原（确定性，可重放）
- L3 做了哪些建议？ → `transcript_corrections` 表
- 用户接受了哪些？ → `status = 'accepted'` 的记录
- 谁在何时做的决定？ → `reviewed_by` + `created_at`

---

## 5. Phase 0：基线测量（前置条件）

> **重要**：在投入开发资源之前，必须先完成基线测量。

### 5.1 目的

量化当前 STT 引擎在危化品场景下的术语准确率，为 Phase 优先级排序和效果评估提供数据依据。

### 5.2 测量方法

1. **准备测试音频集**（目标：至少 30 分钟，日/中/英混合，含 ~50 个领域术语）
2. **分别在 Whisper 和 Parakeet 引擎上运行**，记录原始输出
3. **按 2.1 节错误分类矩阵逐条标注**
4. **计算基线 WER/CER 和关键术语准确率**
5. **验证 whisper-rs 0.13.x 是否暴露 `set_initial_prompt` API**
6. **验证 Rust `regex` 1.11 对 `\p{Katakana}` / `\p{Han}` 的实际行为**（已有 Unicode 类支持，但需实测日/中混合文本）

### 5.3 输出物

| 指标 | 说明 |
|------|------|
| 术语准确率（基线） | 关键术语被正确转录的比例 |
| 错误类别分布 | 各错误类型的占比 |
| 引擎差异报告 | Whisper vs Parakeet 各语言表现差异 |
| API 可用性确认 | `set_initial_prompt` 是否存在 |

### 5.4 决策节点

- 基线术语准确率 > 90%：Phase 1 目标调整为 > 95%
- 基线术语准确率 < 50%：L2 正则覆盖率可能被高估，需重新评估
- `set_initial_prompt` API 缺失：废弃 L1，仅依赖 L2+L3
- Parakeet 用户占比 > 30%：加大 L2/L3 投入以补偿 L1 缺失

### 5.5 预估工作量

1-2 人天。

---

## 6. 第一级：Whisper initial_prompt 软引导

### 6.1 实现原理

Whisper.cpp 暴露 `initial_prompt` 参数。Prompt token 通过 transformer decoder 的 cross-attention 机制，使与 prompt token 语义相近的 token 在输出 logit 中获得更高值。这是软引导，不强制输出。

**关键限制**：
- prompt token 长度上限 ~224 token（whisper.cpp 默认）
- 非确定性，模型仍可能输出其他内容
- **Parakeet 不支持**，本级别仅对 Whisper 引擎生效
- **依赖 whisper-rs 0.13.x 暴露 `set_initial_prompt` — Phase 0 期间必须验证**

### 6.2 Token 截断策略

当日语/中文/英语 high 优先级术语组合后超过 224 token 限制时，采用以下截断策略：

```rust
fn truncate_terms_for_prompt(terms: &[String], max_tokens: usize) -> String {
    // 1. 按优先级排序：high > normal（此处 terms 已过滤为 high）
    // 2. 同优先级按 updated_at 降序（最近更新的优先保留）
    //    在术语表中记录 prompt_included 字段，供 UI 展示哪些术语实际生效
    // 3. 贪心拼接，每加一个术语检查 token 数估计值
    //    （日语约 1.5 token/词，中文约 2 token/词，英语约 1.3 token/词）

    let mut current_tokens = 0;
    let mut included = Vec::new();
    let mut excluded = Vec::new();

    for term in terms {
        let est_tokens = estimate_tokens(term); // 按语言估算
        if current_tokens + est_tokens <= max_tokens {
            current_tokens += est_tokens;
            included.push(term.clone());
        } else {
            excluded.push(term.clone());
        }
    }

    if !excluded.is_empty() {
        log::warn!(
            "L1 prompt truncated: {} terms included, {} excluded (token budget: {})",
            included.len(), excluded.len(), max_tokens
        );
        // emit 事件通知前端显示警告："高优先级术语超载，X 条未注入"
    }

    included.join(", ")
}
```

**前端反馈**：术语管理页面底部显示 "L1 Prompt: 已包含 25/38 条高优先级术语。8 条因 token 限制未注入，5 条待审核。" 用户可据此调整优先级或减少术语。

### 6.3 日/中/英多语言 prompt 示例

**日语会议时**：
```
危険化学品製造会議。以下の用語が含まれる可能性がある：
過酸化物, 引火性液体, 毒劇物, ポリウレタン, エポキシ樹脂,
トルエンジイソシアネート, メチルエチルケトン, 爆発性, 急性毒性,
特定化学物質, 有機溶剤, 作業環境測定, GHS分類, SDS, CAS番号
```

**中文会议时**：
```
危险化学品制造会议。以下术语可能出现：
甲苯二异氰酸酯, 二苯基甲烷二异氰酸酯, 苯乙烯, 环氧树脂, 聚氨酯,
过氧化物, 易燃液体, 急性毒性, 特定化学物质, 有机溶剂, 作业环境测定,
安全数据表, GHS分类, CAS编号, 危险货物编号
```

**英语会议时**：
```
Hazardous chemical manufacturing meeting. Terms:
toluene diisocyanate, methylene diphenyl diisocyanate, styrene monomer,
epoxy resin, polyurethane, peroxide, flammable liquid, acute toxicity,
LD50, LC50, GHS hazard statements H225 H301 H311, CAS registry number
```

### 6.4 代码集成位置

**文件位置**：`frontend/src-tauri/src/whisper_engine/whisper_engine.rs`

在 `transcribe_audio_with_confidence()` 中，`FullParams` 构造完成后、`state.full()` 之前插入（仅当 API 可用时）：

```rust
// ===== L1 initial_prompt 注入（仅 Whisper 引擎，Phase 0 验证 API 可用性） =====
if let Some(ref lang) = language {
    let prompt = terminology::cache::get_initial_prompt(Some(lang));
    if !prompt.is_empty() {
        params.set_initial_prompt(&prompt);  // 需 Phase 0 验证此 API 存在
    }
}
```

---

## 7. 第二级：正则实时校正通道

### 7.1 技术约束

> **关键约束**：Rust 标准 `regex` crate（v1.x）**不支持 look-ahead (`(?=...)`) 和 look-behind (`(?<!...)`) 断言**。
> V3 中基于 look-around 的日/中词边界方案无法直接编译。

### 7.2 替代方案：捕获组模拟词边界

使用捕获组替代 look-around。以日语为例：

```
原方案（不可用）:
  (?<![\p{Han}\p{Hiragana}\p{Katakana}ー])過酸化物(?![\p{Han}\p{Hiragana}\p{Katakana}ー])

替代方案（捕获组）:
  (^|[^\p{Han}\p{Hiragana}\p{Katakana}ー])過酸化物($|[^\p{Han}\p{Hiragana}\p{Katakana}ー])

替换模板:
  ${1}過酸化物${2}
```

**原理**：将"前面不是日文字符"转换为"行首或一个非日文字符（捕获为组 1）"，替换时原样放回。后面的判断同理（捕获为组 2）。

**局限**：
- 行首/行尾边界正确匹配
- 两个术语相邻时（如 `過酸化物ポリウレタン`），中间的边界字符 `物` 被第一个匹配消耗，第二个术语可能不匹配——但由于术语间通常有标点或空格分隔，实际影响有限
- 如果此局限在实际测试中导致显著遗漏，可评估引入 `fancy-regex` crate（支持 look-around，但性能较低）作为特定术语的补充引擎

### 7.3 数据结构与核心实现

```rust
use regex::Regex;
use std::borrow::Cow;

/// 编译后的术语校正规则
struct CompiledRule {
    regex: Regex,
    /// 替换模板。对于 whole_word 规则，含捕获组引用（如 "${1}過酸化物${2}"）
    /// 对于普通规则，直接为术语文本
    replacement_template: String,
    /// 匹配模式的字符数（用于排序）
    original_len: usize,
    /// 是否使用捕获组（决定替换时的行为）
    uses_capture_groups: bool,
}

/// 构建术语匹配模式
/// 返回 (pattern_string, uses_capture_groups)
fn build_term_pattern(entry: &TerminologyEntry) -> (String, bool) {
    let escaped = regex::escape(&entry.original);

    if !entry.whole_word {
        let case_flag = if entry.case_sensitive { "" } else { "(?i)" };
        return (format!("{}{}", case_flag, escaped), false);
    }

    match entry.language.as_str() {
        "ja" => {
            let boundary = r"[\p{Han}\p{Hiragana}\p{Katakana}ー]";
            (
                format!(r"(^|[^{b}]){escaped}($|[^{b}])", b = boundary, escaped = escaped),
                true,
            )
        }
        "zh" => {
            (
                format!(r"(^|[^\p{{Han}}])({})([^\p{{Han}}]|$)", escaped),
                true,
            )
        }
        _ => {
            let case_flag = if entry.case_sensitive { "" } else { "(?i)" };
            (format!(r"{}\b{}\b", case_flag, escaped), false)
        }
    }
}

/// 构建替换模板（与 build_term_pattern 配对使用）
fn build_replacement_template(entry: &TerminologyEntry, uses_capture_groups: bool) -> String {
    if uses_capture_groups {
        format!("${{1}}{}${{2}}", entry.replacement)
    } else {
        entry.replacement.clone()
    }
}

/// 从术语条目列表重建正则缓存
pub fn rebuild_terminology_regex_cache(entries: &[TerminologyEntry]) -> Vec<CompiledRule> {
    let mut entries: Vec<_> = entries.iter().filter(|e| e.enabled).collect();

    // 关键：按 original（匹配模式）的字符数降序排列，确保最长模式优先匹配
    entries.sort_by(|a, b| {
        b.original.chars().count()
            .cmp(&a.original.chars().count())
    });

    let mut rules = Vec::with_capacity(entries.len());
    for entry in entries {
        let (pattern, uses_capture_groups) = build_term_pattern(entry);
        match Regex::new(&pattern) {
            Ok(re) => rules.push(CompiledRule {
                regex: re,
                replacement_template: build_replacement_template(entry, uses_capture_groups),
                original_len: entry.original.chars().count(),
                uses_capture_groups,
            }),
            Err(e) => {
                log::warn!(
                    "Failed to compile regex for '{}': {}",
                    entry.original, e
                );
            }
        }
    }

    log::info!("Terminology regex cache rebuilt: {} rules", rules.len());
    rules
}

/// 对转录文本应用术语校正。
/// 使用 Cow<str> 避免无匹配时的字符串分配。
/// 单次遍历所有规则，在首次匹配时才分配新字符串。
pub fn apply_terminology_correction<'a>(text: &'a str, rules: &[CompiledRule]) -> Cow<'a, str> {
    let mut result = Cow::Borrowed(text);
    for rule in rules {
        if rule.regex.is_match(&result) {
            let owned = result.into_owned();
            result = Cow::Owned(
                rule.regex
                    .replace_all(&owned, rule.replacement_template.as_str())
                    .to_string(),
            );
        }
    }
    result
}
```

### 7.4 语言特定的词边界测试（开发时必测）

```
测试用例 1: 日语 — 术语在句中正确匹配
  输入:  "この物質は過酸化物を含む"
  模式:  (^|[^\p{Han}\p{Hiragana}\p{Katakana}ー])過酸化物($|[^\p{Han}\p{Hiragana}\p{Katakana}ー])
  期望:  "は" 被捕获为组 1，"を" 之后的字符被视为边界

测试用例 2: 日语 — 术语在行首
  输入:  "過酸化物は危険です"
  期望:  组 1 匹配行首（空字符串），正常替换

测试用例 3: 中文 — 短术语不命中长术语
  输入:  "甲苯二异氰酸酯的生产工艺"
  规则:  "甲苯" → 不命中（因为后面是 "二" 即 \p{Han}）
  期望:  不替换

测试用例 4: 中文 — 形近术语正确匹配
  输入:  "使用甲本二亿情酸纸作为原料"
  规则:  "甲本二亿情酸纸" → "甲苯二异氰酸酯"
  期望:  正确替换

测试用例 5: 日语 — 浊音变体
  输入:  "ポリウレタンとボリウレタンの混合"
  规则:  "ボリウレタン" → "ポリウレタン"
  期望:  仅替换 "ボリウレタン"，不误替换 "ポリウレタン"
```

### 7.5 性能基准

> **以下为预估值，以 Phase 1 实现后的实际测量为准。**
> 测试矩阵：术语 50/200/500 条 × 语言 ja/zh/en/mix × chunk 100/500/2000 chars × 设备低/中/高配。

| 规则数 | 文本长度 | 预估耗时 | 备注 |
|--------|----------|----------|------|
| 50 | 200 chars | < 5ms | 典型会议 chunk 场景 |
| 200 | 200 chars | < 20ms | 含大文本时首次分配开销 |
| 500 | 2000 chars | < 100ms | 上限场景，如超 100ms 触发熔断 |

> **熔断机制**：单次 `apply_terminology_correction` 耗时超过 100ms 时，自动降级为仅应用 priority=high 的规则子集，并在 UI 显示警告。

### 7.6 L2 可关闭性

虽然 L2 是确定性的，但用户可能因以下原因需要关闭：
- 调试 STT 引擎原始行为
- 某个术语包的替换规则有误，需暂时禁用排查
- 特定会议类型不适合术语替换

支持关闭粒度：
- **全局开关**：`terminology_enabled = 0`（关闭所有校正，含 L2）
- **按术语包**：前端提供"暂停此术语包"按钮，本质是批量设置 `enabled = 0`
- **调试模式**：`terminology_enabled = 0` 临时关闭，对比 raw 和 L2 后文本

### 7.7 调用位置

在 `worker.rs` 的 `transcribe_chunk_with_provider` 函数中，所有引擎分支的 `cleaned_text` 赋值后统一插入：

```rust
// 原有代码:
let cleaned_text = text.trim().to_string();

// L2 正则校正（所有引擎统一处理，可通过 settings 关闭）:
let cleaned_text = if terminology_enabled {
    let rules = terminology::cache::get_terminology_rules();
    apply_terminology_correction(&cleaned_text, &rules).into_owned()
} else {
    cleaned_text
};
```

---

## 8. 第三级：LLM 深度校正建议

### 8.1 模式定义：仅纠错，不扩写

> **第一版明确限定为"纠错模式"**。术语扩写/知识补全（如将缩写展开为全称+括号注释）不在第一版范围内。

| 操作 | 纠错模式（V1） | 扩写模式（Future） |
|------|:---:|:---:|
| 修正明显 STT 识别错误 | ✅ | ✅ |
| 统一术语表记（IUPAC 优先） | ✅ | ✅ |
| 将 `TDI` 替换为 `トルエンジイソシアネート (TDI)` | ❌ | ✅ |
| 将 `保護具` 替换为 `保護具（手袋・保護メガネ）` | ❌ | ✅ |
| 添加原文没有的解释性内容 | ❌ | ❌（任何时候都不允许） |

**如果 LLM 输出了扩写类建议**：前端在展示时标注 `[扩写]` 标签，用户可手动接受但默认不被纳入纠错建议流。

### 8.2 LLM 任务队列（防止并发 OOM）

本地 7B 模型占用 5-6GB 内存。如果两场短会议连续结束，两个并发的 L3 任务将导致 OOM。

```rust
use std::sync::LazyLock;
use tokio::sync::Semaphore;
use std::sync::Mutex;

/// 全局 LLM 校正任务队列 — 严格限制并发数为 1
static LLM_CORRECTION_QUEUE: LazyLock<Semaphore> =
    LazyLock::new(|| Semaphore::new(1));

/// 正在排队的任务数（供前端查询）
static LLM_QUEUE_LENGTH: LazyLock<Mutex<u32>> =
    LazyLock::new(|| Mutex::new(0));

async fn run_llm_correction_task(meeting_id: String, transcript: String) -> Result<()> {
    // 更新排队计数
    *LLM_QUEUE_LENGTH.lock().unwrap() += 1;

    // 获取信号量许可（前一个任务未完成则阻塞等待）
    let _permit = LLM_CORRECTION_QUEUE.acquire().await
        .map_err(|e| format!("Queue closed: {}", e))?;

    *LLM_QUEUE_LENGTH.lock().unwrap() -= 1;

    // 执行 LLM 校正...
    // 支持超时（默认 60s），超时后放弃本次校正
    // 支持优雅退出：监听 AppHandle 的 shutdown 信号，提前中断
}
```

**前端状态展示**：
- `idle`：无任务运行
- `queued (N)`：排队中，前面有 N 个任务（含当前运行的）
- `running`：LLM 正在生成建议
- `done`：建议已生成，等待查看
- `failed`：生成失败（超时/模型不可用/其他错误）

### 8.3 建议数据结构

每条 L3 建议是独立的、可逐条操作的"建议补丁"：

```rust
/// L3 单条校正建议
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct L3CorrectionSuggestion {
    pub id: String,
    pub meeting_id: String,

    // 定位信息 — 用于与 normalized_transcript 对齐
    pub char_offset: usize,         // 在 normalized_transcript 中的字符偏移
    pub original_span: String,      // 被建议替换的原文片段
    pub suggested_text: String,     // 建议替换为的文本

    // 元信息
    pub language: String,           // 涉及的术语语言
    pub correction_type: String,    // "chemical_name" | "ghs_code" | "cas_number" | ...
    pub reason: String,             // LLM 给出的修正理由（简短）
    pub confidence: Option<f32>,    // LLM 自评置信度（可选）

    // 版本追踪
    pub source_version_hash: String, // normalized_transcript 在建议生成时的 hash
                                     // 用于检测用户是否手动修改了原文

    // 状态
    pub status: String,             // "pending" | "accepted" | "rejected" | "obsolete"
    pub reviewed_by: Option<String>,
    pub reviewed_at: Option<String>,
}
```

**版本冲突检测**：当用户尝试接受一条建议时，系统对比当前 `normalized_transcript` 的 hash 与建议生成时的 `source_version_hash`。如果不一致（用户在 L3 生成期间手动编辑了文本），标记为 `obsolete` 并提示用户。

### 8.4 LLM Provider 选择（多语言场景）

| Provider | 多语言能力 | 延迟 | 内存占用 | 推荐场景 |
|----------|:---:|------|------|------|
| Ollama `qwen2.5:7b` | 中/英优，日可接受 | 2-5s | ~5GB | **默认推荐** |
| Ollama `qwen2.5:3b` | 中/英可接受，日弱 | 1-2s | ~2.5GB | 低内存降级 |
| Ollama `qwen2.5:14b` | 日/中/英均优 | 3-8s | ~9GB | 高配设备可选 |
| Ollama `llama3.1:8b` | 英优，中/日弱 | 3-8s | ~6GB | 备选 |
| Claude API (Sonnet) | 日/中/英均优 | 2-4s | N/A | API 可用时 |
| Built-in sidecar | 取决于模型 | 取决于硬件 | 取决于模型 | 已有 sidecar 的用户 |

### 8.5 LLM 不可用时的降级链

```
L3 触发
  ├─► 获取队列许可 (Semaphore)
  ├─► 尝试 Ollama qwen2.5:7b (默认)  → 成功 → 生成建议
  │                                   → 超时(60s)/失败 ↓
  ├─► 尝试 Ollama qwen2.5:3b (降级)  → 成功 → 生成建议（标注降级模型）
  │                                   → 失败 ↓
  ├─► 尝试 Built-in sidecar          → 成功 → 生成建议
  │                                   → 失败 ↓
  └─► 静默放弃，记录日志。
      前端显示: "深度校正建议暂不可用"。
      不阻塞任何主流程。
```

### 8.6 Prompt 设计

```markdown
## システムプロンプト / 系统提示 / System Prompt

あなたは Meetily の文字起こし校正器です。**纠错模式**で動作しています。
你是 Meetily 的转录文本校正器。工作于**纠错模式**。
You are Meetily's transcript corrector, operating in **correction mode**.

### 任務 / 任务 / Task
音声認識の明らかな誤りを修正してください。**原文にない情報の追加は禁止**。
请修正语音识别的明显错误。**禁止添加原文没有的信息**。
Correct only clear speech recognition errors. **Do NOT add information not in original speech**.

### 会議言語 / 会议语言
日本語・中国語・英語が混在します。

### 処理対象のエラー / 需处理的错误
1. 片仮名化学物質名の分割誤り（「ポリ ウレ たん」→「ポリウレタン」）
2. 中国語化学名の同音異字誤り（「甲本二亿情酸纸」→「甲苯二异氰酸酯」）
3. CAS番号・UN番号の誤認識（「k Ass one o eight」→「CAS 108-88-3」）
4. GHSコードの形式誤り（「H two twenty five」→「H225」）
5. 言語切替時の混同

### 用語リファレンス / 术语参考
%TERMINOLOGY_TABLE%

### 校正ルール / 校正规则
✅ 修正してよいもの:
   - 用語表にある用語への明らかな認識誤り
   - 化学物質名の表記ゆれ統一（IUPAC名優先）
   - CAS/UN番号・GHSコードの標準形式への修正
   - 言語切り替えの明らかな誤判定

❌ 禁止事項:
   - **原文にない情報の追加（略語のフルスペル展開も禁止）**
   - 語順・文体・語態の変更
   - 文法の添削
   - 文章の分割や結合
   - 確信が持てない箇所の修正

### 出力形式 / 输出格式
以下のJSON形式で修正提案のみを出力：
[{"original": "誤った文字列", "suggested": "修正後の文字列", "reason": "修正理由(簡潔に)", "language": "ja/zh/en"}]
修正不要の場合は [] を出力。説明・コメント不要。

### 元の文字起こし / 原始转录
%TRANSCRIPT_TEXT%
```

### 8.7 前端交互：按术语聚类批量操作

> V3 的"逐条确认"在一场 1 小时会议中可能产生 100+ 条建议，逐条点击会导致操作疲劳和盲目"全部接受"。

替代方案 — **按术语聚类**：

```
┌──────────────────────────────────────────────────────────────┐
│  L3 深度校正建议 (共 23 条, 涉及 5 个术语)                      │
│                                                              │
│  ┌─ 🟡 化学名: ポリウレタン (8 处) ─────────────────────┐   │
│  │  "ポリ ウレ たん" → "ポリウレタン"                    │   │
│  │  出现位置: 00:03:15, 00:07:42, 00:12:08, ...         │   │
│  │  预览: "...主原料はポリ ウレ たんで..."               │   │
│  │  [一键接受全部 8 处]  [逐条查看]  [全部拒绝]          │   │
│  └──────────────────────────────────────────────────────┘   │
│                                                              │
│  ┌─ 🔵 代码: H225 → H225 (5 处) ──────────────────────┐   │
│  │  "H two twenty five" / "H二二五" → "H225"              │   │
│  │  出现位置: 00:05:30, 00:15:10, ...                     │   │
│  │  [一键接受全部 5 处]  [逐条查看]  [全部拒绝]          │   │
│  └──────────────────────────────────────────────────────┘   │
│                                                              │
│  ┌─ 🟢 中文: 甲苯二异氰酸酯 (6 处) ────────────────────┐   │
│  │  "甲本二亿情酸纸" → "甲苯二异氰酸酯"                    │   │
│  │  [一键接受全部 6 处]  [逐条查看]  [全部拒绝]          │   │
│  └──────────────────────────────────────────────────────┘   │
│                                                              │
│  [接受全部 23 条]  [拒绝全部]                                │
└──────────────────────────────────────────────────────────────┘
```

用户先看到术语级别的摘要。对某个术语有疑虑可以展开查看逐条上下文。这比逐条滚动 100 条更高效，也不会削弱审核质量。

---

## 9. 与现有录音停止链路的集成

> **关键**：PRD 的描述必须与当前代码库的实际事件流对齐。
> 当前项目的录音停止和保存流程由 Rust `recording_manager` 发布事件，前端监听后发起后续操作。

### 9.1 现有链路（基于代码库分析）

```
recording_manager.rs: stop_recording()
  → 保存音频文件
  → emit("recording-stopped", { meeting_id, audio_file_path })
  
前端 page.tsx: 监听 "recording-stopped"
  → 获取聚合的 transcription chunks（前端内存中）
  → invoke("save_transcript", { meeting_id, transcript_text })
  → 可选：invoke("generate_summary", { meeting_id })
```

### 9.2 术语校正集成后的链路

```
recording_manager.rs: stop_recording()
  → emit("recording-stopped", { meeting_id })
  
前端 page.tsx: 监听 "recording-stopped"
  │
  ├─► Step 1: 保存（同步等待）
  │     invoke("save_transcript_with_terminology", {
  │         meeting_id,
  │         raw_transcript,         // 前端聚合的原始 STT 输出（L1 后、L2 前）
  │         normalized_transcript,  // 前端聚合的 L2 校正后文本
  │     })
  │     → Rust: INSERT raw_transcript (不可变)
  │     → Rust: INSERT/UPDATE transcript (normalized)
  │
  ├─► Step 2: 异步触发 L3（不阻塞 UI）
  │     invoke("run_llm_terminology_correction", { meeting_id })
  │     → Rust: 将任务加入全局 LLM 串行队列
  │     → 立即返回 { status: "queued" }
  │
  └─► Step 3: 前端 polling 或监听事件
        listen("llm-corrections-ready", (payload) => { ... })
        或 polling: invoke("get_corrections_for_meeting", { meeting_id })
        → 检测到 status = "done" → 展示按术语聚类的差异视图
```

### 9.3 职责划分

| 职责 | 负责方 | 说明 |
|------|:---:|------|
| 聚合转录 chunks | 前端 | 当前已由前端聚合 `transcript-update` 事件的文本 |
| 区分 raw vs normalized | 前端 | 前端持有每个 chunk 的 L2 前后版本，聚合时分别拼接 |
| 保存至 DB | Rust | 通过 `save_transcript_with_terminology` 命令 |
| 发起 L3 任务 | 前端 | 通过 `run_llm_terminology_correction` 命令 |
| L3 任务调度 | Rust | 全局串行队列，Semaphore(1) |
| 通知 L3 完成 | Rust → 前端 | 通过 Tauri event `llm-corrections-ready` |
| 摘要生成依赖 | Rust | 摘要系统默认读取 `normalized_transcript`（L1+L2 后），不等待 L3。用户可在接受 L3 建议后手动触发重新摘要 |

---

## 10. 数据库设计

### 10.1 新建表：`terminology`

**迁移文件**：`migrations/20260427000000_add_terminology.sql`

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
    created_at       TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at       TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(original, language)  -- CSV 导入冲突时的 upsert 键
);

CREATE INDEX IF NOT EXISTS idx_terminology_language ON terminology(language);
CREATE INDEX IF NOT EXISTS idx_terminology_enabled ON terminology(enabled);
CREATE INDEX IF NOT EXISTS idx_terminology_priority ON terminology(priority);
```

**CSV 导入冲突策略**：`original + language` 为联合唯一键。导入时若匹配到已有条目，默认覆盖（upsert）。前端导入预览中显示"将新增 X 条，覆盖 Y 条现有规则"。

### 10.2 新建表：`transcript_corrections`

```sql
CREATE TABLE IF NOT EXISTS transcript_corrections (
    id                  TEXT PRIMARY KEY,
    meeting_id          TEXT NOT NULL,
    char_offset         INTEGER,              -- 在 normalized_transcript 中的字符偏移
    original_span       TEXT NOT NULL,        -- 被建议替换的原文片段
    suggested_text      TEXT NOT NULL,        -- 建议替换为的文本
    language            TEXT,
    correction_type     TEXT NOT NULL DEFAULT 'llm',
    correction_level    TEXT NOT NULL DEFAULT 'l3',
    reason              TEXT,
    confidence          REAL,                 -- LLM 自评置信度 (0-1)
    source_version_hash TEXT,                 -- normalized_transcript 在建议生成时的 hash
    status              TEXT NOT NULL DEFAULT 'pending',
    reviewed_by         TEXT,
    reviewed_at         TEXT,
    created_at          TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (meeting_id) REFERENCES meetings(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_corrections_meeting ON transcript_corrections(meeting_id);
CREATE INDEX IF NOT EXISTS idx_corrections_status ON transcript_corrections(status);
```

### 10.3 现有表扩展（幂等迁移）

```sql
-- transcripts 表新增 raw_transcript（强制保留，不可变）
-- 原则：INSERT 时写入，UPDATE 时禁止修改此列（代码规范保证）

-- settings 表新增术语校正相关字段
-- terminology_enabled: 1 = 总开关
-- initial_prompt_enabled: 1 = L1 (仅 Whisper)
-- llm_correction_enabled: 1 = L3 LLM 建议
-- llm_correction_auto_accept: 0 = 默认仅建议 (必须用户手动开启)
```

**Rust 幂等迁移实现**：

```rust
// 在 database/setup.rs 中
async fn add_column_if_not_exists(
    pool: &SqlitePool,
    table: &str,
    column: &str,
    definition: &str,
) -> Result<()> {
    let query = format!(
        "SELECT COUNT(*) FROM pragma_table_info('{}') WHERE name = '{}'",
        table, column
    );
    let count: (i64,) = sqlx::query_as(&query).fetch_one(pool).await?;
    if count.0 == 0 {
        sqlx::query(&format!("ALTER TABLE {} ADD COLUMN {} {}", table, column, definition))
            .execute(pool)
            .await?;
        log::info!("Added column {}.{} ({})", table, column, definition);
    }
    Ok(())
}
```

### 10.4 数据模型（Rust）

```rust
// database/models.rs 追加

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
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct TranscriptCorrection {
    pub id: String,
    pub meeting_id: String,
    pub char_offset: Option<i64>,
    pub original_span: String,
    pub suggested_text: String,
    pub language: Option<String>,
    pub correction_type: String,
    pub correction_level: String,
    pub reason: Option<String>,
    pub confidence: Option<f64>,
    pub source_version_hash: Option<String>,
    pub status: String,
    pub reviewed_by: Option<String>,
    pub reviewed_at: Option<String>,
    pub created_at: String,
}
```

### 10.5 仓库层（新增）

```rust
// database/repositories/terminology.rs（新建）
impl DatabaseManager {
    pub async fn get_all_terminology(&self) -> Result<Vec<TerminologyEntry>>;
    pub async fn get_enabled_terminology(&self) -> Result<Vec<TerminologyEntry>>;
    pub async fn upsert_terminology(&self, entries: Vec<TerminologyEntry>) -> Result<u64>;
    pub async fn delete_terminology(&self, id: &str) -> Result<()>;
    pub async fn create_corrections(&self, corrections: Vec<TranscriptCorrection>) -> Result<()>;
    pub async fn update_correction_status(&self, id: &str, status: &str, reviewed_by: &str) -> Result<()>;
    pub async fn get_corrections_for_meeting(&self, meeting_id: &str) -> Result<Vec<TranscriptCorrection>>;
    pub async fn save_transcript_with_raw(&self, meeting_id: &str, raw: &str, normalized: &str) -> Result<()>;
}
```

---

## 11. 后端实现规范

### 11.1 模块组织结构

```
frontend/src-tauri/src/
├── terminology/                     # 新建模块
│   ├── mod.rs
│   ├── cache.rs                     # 统一缓存管理
│   │   ├── INITIAL_PROMPT_BY_LANG   #   L1 prompt 缓存
│   │   ├── TERMINOLOGY_RULES         #   L2 编译后规则缓存
│   │   ├── refresh_all_caches()     #   原子刷新（从 DB 单次加载）
│   │   ├── get_initial_prompt()     #   获取 L1 prompt（含截断）
│   │   └── get_terminology_rules()  #   获取 L2 规则列表
│   ├── commands.rs                  # Tauri 命令 (CRUD + 缓存刷新 + L3触发)
│   ├── corrector.rs                 # 校正器
│   │   ├── apply_terminology_correction()   # L2 正则（捕获组版）
│   │   ├── validate_chemical_codes()        # L3 后规则验证
│   │   └── llm_correction_task()            # L3 LLM 任务（含队列调度）
│   └── queue.rs                     # LLM 任务队列
│       ├── LLM_CORRECTION_QUEUE      #   Semaphore(1)
│       └── queue_status()           #   查询排队状态
│
├── whisper_engine/
│   └── whisper_engine.rs           # L1 集成（仅 Whisper，API 验证后）
│
├── audio/transcription/worker.rs   # L2 集成（所有引擎统一入口）
│
├── summary/llm_client.rs           # L3: 新增 suggest_corrections()
│
├── database/
│   ├── models.rs                   # 追加 TerminologyEntry, TranscriptCorrection
│   ├── repositories/
│   │   └── terminology.rs          # 新建
│   └── setup.rs                    # 幂等迁移
│
├── lib.rs                          # 注册新命令
│
└── migrations/
    └── 20260427000000_add_terminology.sql
```

### 11.2 Tauri 命令注册

```rust
// 术语管理 CRUD
terminology::commands::get_terminology_list,
terminology::commands::save_terminology_entry,
terminology::commands::delete_terminology_entry,
terminology::commands::import_terminology_csv,
terminology::commands::export_terminology_csv,

// 统一缓存刷新
terminology::commands::refresh_all_terminology_caches,

// L3 LLM 校正
terminology::commands::run_llm_terminology_correction,
terminology::commands::get_llm_queue_status,
terminology::commands::get_corrections_for_meeting,
terminology::commands::accept_correction,
terminology::commands::reject_correction,
terminology::commands::batch_accept_corrections_by_term,  // 按术语批量接受

// 保存（含 raw_transcript）
database::commands::save_transcript_with_terminology,

// 设置
terminology::commands::get_terminology_settings,
terminology::commands::set_terminology_settings,
```

### 11.3 启动初始化

```rust
// lib.rs setup 闭包，迁移完成后
let db = app.state::<state::AppState>().db_manager.clone();
tauri::async_runtime::spawn(async move {
    if let Err(e) = terminology::cache::refresh_all_caches(&db).await {
        log::warn!("Failed to initialize terminology caches: {}", e);
    }
});
```

### 11.4 统一缓存刷新（原子操作）

```rust
// terminology/cache.rs
pub async fn refresh_all_caches(db: &DatabaseManager) -> Result<(), String> {
    let entries = db.get_enabled_terminology().await
        .map_err(|e| format!("Failed to load terminology: {}", e))?;

    // 从同一批 entries 同时构建 L1 和 L2 缓存
    let prompts = build_initial_prompts_with_truncation(&entries, 200);  // 含截断逻辑
    let rules = rebuild_terminology_regex_cache(&entries);

    // 原子写入
    *INITIAL_PROMPT_BY_LANG.write().map_err(|e| e.to_string())? = prompts;
    *TERMINOLOGY_RULES.write().map_err(|e| e.to_string())? = rules;

    log::info!("Terminology caches refreshed: {} L1 languages, {} L2 rules",
        INITIAL_PROMPT_BY_LANG.read().unwrap().len(),
        TERMINOLOGY_RULES.read().unwrap().len());
    Ok(())
}
```

---

## 12. 前端实现规范

### 12.1 新增组件

| 组件 | 文件路径 | 说明 |
|------|----------|------|
| `TerminologyManager` | `components/TerminologyManager/index.tsx` | 术语管理主面板 |
| `TerminologyEntryRow` | `components/TerminologyManager/EntryRow.tsx` | 单条术语编辑行 |
| `TerminologyImportDialog` | `components/TerminologyManager/ImportDialog.tsx` | CSV 导入对话框（含冲突预览） |
| `CorrectionDiffView` | `components/CorrectionDiff/DiffView.tsx` | 按术语聚类的校正差异视图 |
| `L3QueueStatus` | `components/CorrectionDiff/QueueStatus.tsx` | L3 任务状态指示器 |

### 12.2 会议详情页 — 按术语聚类的差异视图

```
┌──────────────────────────────────────────────────────────────┐
│  L3 深度校正建议 (共 23 条, 涉及 5 个术语)      状态: ✅ 已完成 │
│                                                              │
│  ┌─ 🟡 化学名: ポリウレタン (8 处) ─────────────────────┐   │
│  │  "ポリ ウレ たん" → "ポリウレタン"                    │   │
│  │  出现位置: 00:03:15, 00:07:42, 00:12:08, ...         │   │
│  │  [一键接受全部 8 处]  [逐条查看]  [全部拒绝]          │   │
│  └──────────────────────────────────────────────────────┘   │
│                                                              │
│  ┌─ 🔵 代码: H225 → H225 (5 处) ──────────────────────┐   │
│  │  "H two twenty five" / "H二二五" → "H225"              │   │
│  │  [一键接受全部 5 处]  [逐条查看]  [全部拒绝]          │   │
│  └──────────────────────────────────────────────────────┘   │
│                                                              │
│  [接受全部]  [拒绝全部]                                      │
└──────────────────────────────────────────────────────────────┘
```

### 12.3 TypeScript 类型定义

```typescript
// frontend/src/types/terminology.ts（新建）

export type TerminologyLanguage = 'ja' | 'zh' | 'en' | 'auto';
export type TerminologyPriority = 'high' | 'normal' | 'low';
export type CorrectionStatus = 'pending' | 'accepted' | 'rejected' | 'obsolete';

export interface TerminologyEntry {
  id: string;
  original: string;
  replacement: string;
  language: TerminologyLanguage;
  caseSensitive: boolean;
  wholeWord: boolean;
  enabled: boolean;
  priority: TerminologyPriority;
  category: string;
  description?: string;
  createdAt: string;
  updatedAt: string;
}

export interface TranscriptCorrection {
  id: string;
  meetingId: string;
  charOffset: number | null;
  originalSpan: string;
  suggestedText: string;
  language?: TerminologyLanguage;
  correctionType: 'chemical_name' | 'ghs_code' | 'cas_number' | 'un_number' | 'language_switch' | 'other';
  correctionLevel: 'l3';
  reason?: string;
  confidence?: number;
  sourceVersionHash?: string;
  status: CorrectionStatus;
  reviewedBy?: string;
  reviewedAt?: string;
  createdAt: string;
}

/** 按术语聚类后的展示结构 */
export interface ClusteredSuggestions {
  term: string;           // 聚类键: 目标术语
  language: TerminologyLanguage;
  count: number;          // 出现次数
  suggestions: TranscriptCorrection[];
}

export interface TerminologySettings {
  terminologyEnabled: boolean;
  initialPromptEnabled: boolean;
  llmCorrectionEnabled: boolean;
  llmCorrectionAutoAccept: boolean;  // 默认 false
  llmProvider: string;
  llmModel: string;
}

export type L3QueueStatus = 'idle' | 'queued' | 'running' | 'done' | 'failed';
```

---

## 13. 配置与存储

### 13.1 配置项

| 配置项 | 默认值 | 说明 |
|--------|:---:|------|
| `terminology_enabled` | 1 | 总开关（含 L2） |
| `initial_prompt_enabled` | 1 | L1 开关（仅对 Whisper 有效） |
| `llm_correction_enabled` | 1 | L3 建议生成开关 |
| `llm_correction_auto_accept` | **0** | L3 自动接受（必须用户手动开启并二次确认） |
| L2 开关 | 通过 `terminology_enabled` 控制 | 也可按术语包批量禁用 |

### 13.2 CSV 导入/导出

导入格式（UTF-8 BOM, 逗号分隔, 带表头）：

```csv
original,replacement,language,case_sensitive,whole_word,enabled,priority,category,description
かさんかぶつ,過酸化物,ja,false,true,true,high,化学物質,片仮名→漢字修正
甲本二亿情酸纸,甲苯二异氰酸酯,zh,false,true,true,high,化学物質,同音字修正
H two twenty five,H225,en,false,true,true,high,GHSコード,危険有害性コード
```

**编码支持分阶段**：
- MVP（Phase 1A）：仅支持 UTF-8 BOM
- Phase 1B 追加：Shift-JIS 自动检测（前端预览 5 行 + 用户确认编码）
- 导入时显示冲突预览："将新增 X 条，覆盖 Y 条（original + language 重复）"

**冲突键**：`(original, language)` 联合唯一。匹配时 upsert（覆盖）。

### 13.3 内置预置术语包

```
preset_terminology/
├── chemical_ja.csv            # 日本語（約50条，覆盖高概率错误）
├── chemical_zh.csv            # 中文（約50条）
├── chemical_en.csv            # English（約50条）
├── ghs_codes.csv              # GHS代码（約30条）
└── README.md
```

---

## 14. 硬件要求与降级策略

### 14.1 最低配置

| 组件 | 最低配置 | 推荐配置 |
|------|----------|----------|
| RAM | 8GB (L1+L2) | 16GB+ (含 L3) |
| VRAM | 不需要 | 4GB+ (GPU STT + LLM 共存) |
| 磁盘 | 500MB | 3-5GB (含 LLM 模型) |

### 14.2 降级策略

| 设备条件 | L1 | L2 | L3 |
|------|:---:|:---:|:---:|
| RAM < 8GB | ❌ | ✅ | ❌ |
| RAM 8-12GB | ✅ | ✅ | 尝试 3B，不可用则跳过 |
| RAM > 12GB | ✅ | ✅ | ✅ (默认 7B) |
| 电池供电 | ✅ | ✅ | ❌ (可手动开启) |
| Ollama 未安装 | ✅ | ✅ | ❌ (提示安装) |

> **注意**：以上降级策略第一版作为**推荐策略**提供给用户（设置页面展示），实际执行依赖用户配置而非自动检测。自动硬件检测和电源状态感知在后续版本中完善。

### 14.3 电源建议

- 电池供电时，L3 默认禁用（用户可在设置中覆盖）
- UI 中显示当前策略和建议

---

## 15. 实施计划与 MVP 策略

### 15.1 MVP 定义

| 阶段 | 内容 | 交付价值 | 预估工作量 |
|------|------|----------|:---:|
| **Phase 0** | 基线测量 + API 验证（`set_initial_prompt`、regex Unicode） | 数据驱动决策 | 1-2 人天 |
| **Phase 1A (MVP)** | L2 正则（捕获组版）+ 数据库（含 raw_transcript）+ 基础术语管理 UI | ~50% 错误覆盖，审计链建立 | 3-4 人天 |
| **Phase 1B** | L1 initial_prompt（如 API 可用）+ CSV 导入（UTF-8 BOM） | 追加 ~15-20% 覆盖 | 2 人天 |
| **Phase 2** | L3 LLM 建议（队列 + 聚类 UI）+ 差异对比 | 追加 ~10-20% 覆盖 | 3-4 人天 |
| **Phase 3** | 完善：Shift-JIS、审计报告、注音辅助、摘要系统对齐 | 合规与易用性 | 2-3 人天 |

**总计：11-15 人天**

### 15.2 工作包拆分（替代视角）

如果按能力域拆分（而非流水线阶段）：

| 工作包 | 内容 | 可独立验收 |
|------|------|:---:|
| **WP-A: 审计基础** | raw_transcript 保留 + 分层存储 + 幂等迁移 | ✅ |
| **WP-B: L1+L2 术语匹配** | 正则引擎（捕获组）+ initial_prompt + 术语 CRUD | ✅ |
| **WP-C: L3 建议流** | LLM 队列 + 建议数据模型 + 聚类 UI + 版本冲突检测 | ✅ |
| **WP-D: 导入与治理** | CSV 导入导出 + 预置包 + 编码检测 | ✅ |

### 15.3 MVP（Phase 1A）核心范围

如果资源有限，最小可交付：

1. `terminology` 表 + `raw_transcript` 字段 + 幂等迁移
2. L2 正则缓存（捕获组版）+ 集成到 `worker.rs`
3. 基础术语管理 UI（表格 + 新增/删除/语言筛选）
4. 录制停止时同时保存 raw 和 normalized
5. 预置术语包（精简版 50 条）

**MVP 不包含**：L1 initial_prompt、L3 LLM 建议、CSV 导入导出、差异对比视图、Shift-JIS 支持。

---

## 16. 测试策略

### 16.1 测试矩阵

| 测试维度 | 变量 | 方法 |
|----------|------|------|
| 术语数量 | 50 / 200 / 500 | 基准测试 L2 耗时 |
| 语言模式 | ja / zh / en / mix | 各语言正确性 |
| Chunk 长度 | 100 / 500 / 2000 chars | 性能不退化 |
| 设备等级 | 低配(8GB) / 中配(16GB) / 高配(32GB) | L3 内存行为 |

### 16.2 关键测试用例

```
✅ 日语: "過酸化物は危険です" — "過酸化物"正确匹配，不被"は"干扰
✅ 日语: "ポリ ウレ たん" → "ポリウレタン"（分割修复）
✅ 日语: "ボリウレタン" → "ポリウレタン"（浊音修复），"ポリウレタン"不误替换
✅ 中文: "甲苯二异氰酸酯的生产" — "甲苯"不误匹配
✅ 中文: "甲本二亿情酸纸" → "甲苯二异氰酸酯"（同音字修复）
✅ 英语: "H two twenty five" → "H225"
✅ 混合: "この TDI は危険です" → TDI 保留（英语术语不误替换）
✅ 正则排序: 长 original 先于短 original 执行
✅ Cow<str>: 无匹配时不分配字符串
✅ L3 队列: 两个任务并发提交 → 第二个排队，不 OOM
✅ L3 版本冲突: 用户手动修改原文后，旧建议标记 obsolete
✅ 幂等迁移: 重复执行迁移不报错
```

### 16.3 验收标准

- [ ] 原始 STT 输出（raw_transcript）强制保留，不可修改
- [ ] L1+L2 校正后文本与 raw 明确区分存储
- [ ] L3 建议独立存储，默认仅建议，需用户确认后生效
- [ ] 按术语聚类展示建议，支持批量接受/拒绝
- [ ] 日语长音符/促音/浊音变体被正确替换
- [ ] 中文同音字变体被正确替换
- [ ] CAS/UN/GHS 编码格式被正确恢复
- [ ] L3 LLM 校正经全局串行队列执行，仅并发 1
- [ ] L3 版本冲突检测正常（手动编辑后旧建议废弃）
- [ ] L1 token 超载时截断 + 前端警告
- [ ] CSV 导入显示冲突预览（新增/覆盖条数）
- [ ] 校正审计日志完整可追溯
- [ ] 低内存设备可选降级策略

---

## 17. 风险与应对

| 风险 | 影响 | 概率 | 应对 |
|------|------|:---:|------|
| whisper-rs 0.13.x 未暴露 `set_initial_prompt` | L1 无法实现 | 中 | Phase 0 验证。若无此 API：放弃 L1，仅依赖 L2+L3 |
| Rust `regex` 对 `\p{Katakana}` 等 Unicode 类的实际行为与预期不符 | L2 日语匹配异常 | 低 | Phase 0 实测。regex 1.x 文档声称支持，但需验证混合文本 |
| 捕获组边界方案在术语相邻场景有遗漏 | 部分术语未替换 | 低 | 7.2 节已分析。如实际测试中问题显著，评估 `fancy-regex` 作为特定术语的补充引擎 |
| 两场会议连续结束，L3 任务堆积 | UI 长时间显示"排队中" | 中 | 串行队列 + 超时(60s) + 前端显示排队状态。不阻塞主流程 |
| 用户在 L3 生成期间手动编辑文本 | 建议过时 | 中 | 版本 hash 检测 + 标记 obsolete。用户接受时做最终对比 |
| 日/中/英混合时 LLM 语义错乱 | L3 建议质量差 | 中 | 仅纠错模式（不扩写），减少 LLM 自由度。建议逐条确认 |
| Shift-JIS CSV 乱码 | 日企用户导入失败 | 中 | MVP 仅 UTF-8 BOM。Phase 1B 追加编码检测（BOM → UTF-8 → Shift-JIS 尝试 → 手动选择） |
| 术语表 500+ 条 | L2 性能退化 | 低 | Cow 优化 + 最长优先 + 熔断降级（> 100ms → 仅 high 规则） |
| L3 默认建议模式不足以满足用户效率需求 | 用户逐条确认疲劳 | 中 | 按术语聚类 + 批量操作的 UI 降低认知负荷。用户可手动开启自动接受 |
| 危化品术语校正涉及合规风险 | 错误校正导致安全信息错误 | **高** | raw_transcript 强制保留。L3 默认仅建议。审计日志完整。自动接受需二次确认 |

---

## 18. 合规与法务审查

### 18.1 审查节点

| 节点 | 时机 | 审查内容 |
|------|------|----------|
| Gate A | Phase 1A 完成后 | raw_transcript 保留机制 + L2 确定性替换的审计兼容性 |
| Gate B | Phase 2 上线前 | L3 建议模式合规性（仅建议、需确认、可追溯） |
| Gate C | Phase 3 完成后 | 审计报告格式是否满足行业监管要求 |

### 18.2 合规原则

1. **原始证据保留**：`raw_transcript` 不可变，完整保留 STT 原始输出。
2. **L2（确定性）**：可自动应用。原始文本与替换记录分层存储。
3. **L3（非确定性）**：默认仅建议。自动接受需用户二次确认知情同意。
4. **审计链**：所有校正操作记录操作者、时间戳、修改前后文本、版本 hash。

### 18.3 危化品行业推荐默认设置

| 设置项 | 推荐默认值 | 说明 |
|--------|:---:|------|
| `terminology_enabled` | 1 | |
| `initial_prompt_enabled` | 1 | |
| `llm_correction_enabled` | 1 | 允许生成建议 |
| `llm_correction_auto_accept` | **0** | **禁止自动接受** |

---

## 19. 回滚与功能淘汰

### 19.1 运行时回滚

- **总开关**：`terminology_enabled = 0` → 禁用所有校正（含 L2），转录管道恢复原始行为
- **分级回滚**：
  - L1：`initial_prompt_enabled = 0` → Whisper 不再注入 prompt
  - L2：`terminology_enabled = 0` 或批量禁用术语包
  - L3：`llm_correction_enabled = 0` → 停止触发 L3 任务
- **回滚不删除数据**：术语表和校正记录保留

### 19.2 数据回滚

- 被 rejected 的校正保留记录（status = 'rejected'）
- raw_transcript 永远不变，始终可作为回滚锚点
- 术语表 CSV 导入前自动备份

### 19.3 性能熔断

- L2 单次耗时 > 100ms → 仅应用 high 优先级规则，UI 显示警告
- L3 单次耗时 > 60s → 超时放弃，记录日志
- 连续 3 次 L3 超时 → 建议用户检查 LLM 配置

---

## 附录 A：成功指标体系

不承诺单一数字目标。按层级评估：

| 指标 | 测量方法 | 目标值 |
|------|----------|:---:|
| L2 误替换率 | 人工审核 100 条 L2 替换样本 | < 1% |
| L3 建议接受率 | 用户实际接受/拒绝比例 | > 70%（说明建议质量高） |
| 用户人工复核时长 | 对比有无术语校正时的审核耗时 | 下降 > 30% |
| 高风险术语误修正率 | 对安全法规术语的专项审核 | < 0.1% |
| 术语准确率提升 | 与 Phase 0 基线对比 | 基于基线设定（不预设绝对值） |

---

## 附录 B：V3.0 → V3.1 变更摘要

| 变更项 | V3.0 | V3.1 | 触发来源 |
|--------|------|------|------|
| **L2 正则实现** | look-around 断言（`(?<!...)`) | **捕获组模拟边界**（`(^|[^X])term($|[^X])`） | GPT P0-1：Rust regex 不支持 look-around |
| **raw_transcript** | 可选增强（"如需回溯需添加"） | **强制保留，不可变，L0 层** | GPT P0-2：审计链不完整 |
| **L3 数据模型** | 整段 before/after | **逐条建议补丁**（offset + span + version_hash） | GPT P1-1 + Gemini：细粒度操作 |
| **L3 模式定义** | 纠错+扩写混在一起 | **第一版仅纠错模式** | GPT P1-2：原则冲突 |
| **L3 并发控制** | 无 | **全局 Semaphore(1) 串行队列** | Gemini：并发 OOM |
| **L1 截断策略** | 未定义 | **按优先级+updated_at 截断 + 前端超载警告** | Gemini：超 224 token 时行为未定义 |
| **L3 UX** | 逐条接受/拒绝 | **按术语聚类 + 批量操作** | Gemini：操作疲劳使合规形同虚设 |
| **版本冲突处理** | 无 | **source_version_hash + obsolete 标记** | Gemini + GPT：手动编辑竞态 |
| **与现有架构对齐** | Rust 端串行保存 | **明确前端聚合 + Rust 保存 + 职责划分** | GPT P1-3：与实际事件流对齐 |
| **性能承诺** | 写成既定结论 | **标注为预估值，以 Phase 0 实测为准** | GPT P1-4 |
| **L2 可关闭性** | 不可单独关闭 | **terminology_enabled 总开关 + 按术语包禁用** | GPT P2-1 |
| **硬件自动降级** | 自动检测+自动调度 | **推荐策略，用户手动配置，后续版本完善** | GPT P2-2 |
| **CSV 编码** | 自动检测 UTF-8/Shift-JIS | **MVP 仅 UTF-8 BOM，Phase 1B 追加 Shift-JIS** | GPT P2-3 |
| **CSV 冲突策略** | 未定义 | **UNIQUE(original, language) + upsert + 预览提示** | Gemini |
| **Aho-Corasick** | "替代方案" | **修正为仅用于纯字面量子集（不支持 Unicode 边界）** | Gemini |
| **成功指标** | 单一 "> 99%" | **多维指标体系（误替换率、接受率、复核时长等）** | GPT |
| **摘要系统对齐** | 未提及 | **摘要默认依赖 normalized_transcript** | GPT P1-3 |
| **测试矩阵** | 散列用例 | **术语数×语言×chunk长×设备的矩阵** | GPT P1-4 |

---

> **ドキュメントメンテナンス / 文档维护**：本 PRD V3.1 基于 V3.0 的 Gemini-3.1-Pro 和 GPT-4 双重技术审查后修订，修正了方案级硬伤（正则引擎兼容性、原始证据链缺失），补充了并发控制、版本冲突检测和按术语聚类的 UX 设计。实现过程中如遇架构变更，请同步更新本文档。
