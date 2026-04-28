# PRD：转录专业术语定制化功能

> **文档状态**：V3.0（基于 V2.0 技术审查后重写）
> **创建日期**：2026-04-27
> **修订日期**：2026-04-27
> **关联项目**：Meetily v0.3.0
> **目标行业**：危险化学品制造业（日系企业）
> **支持语言**：日本語 / 中文 / English
> **核心目标**：使 Meetily 转录引擎支持用户自定义专业术语词库，通过三级管道（模型内提示 → 正则后处理 → LLM 深度校正）识别并纠正语音识别（STT）产生的术语拼写/识别错误。

---

## 目录

1. [需求背景](#1-需求背景)
2. [核心问题分析](#2-核心问题分析)
3. [总体架构设计](#3-总体架构设计)
4. [Phase 0：基线测量（前置条件）](#4-phase-0基线测量前置条件)
5. [第一级：Whisper initial_prompt 软引导](#5-第一级whisper-initial_prompt-软引导)
6. [第二级：正则实时校正通道](#6-第二级正则实时校正通道)
7. [第三级：LLM 深度校正通道](#7-第三级llm-深度校正通道)
8. [数据库设计](#8-数据库设计)
9. [后端实现规范](#9-后端实现规范)
10. [前端实现规范](#10-前端实现规范)
11. [配置与存储](#11-配置与存储)
12. [硬件要求与降级策略](#12-硬件要求与降级策略)
13. [实施计划与 MVP 策略](#13-实施计划与-mvp-策略)
14. [测试策略](#14-测试策略)
15. [风险与应对](#15-风险与应对)
16. [合规与法务审查](#16-合规与法务审查)
17. [回滚与功能淘汰](#17-回滚与功能淘汰)

---

## 1. 需求背景

### 1.1 业务场景

客户为一家在华日系危险化学品制造企业。日常会议具有以下特征：

| 特征 | 说明 |
|------|------|
| **多语言混合** | 会议中频繁切换日语、中文、英语，同一句话内常包含两种以上语言 |
| **高度专业化** | 涉及 MSDS（安全数据表）、CAS 编号、UN 危险货物编号、GHS 分类、化学物质 IUPAC 命名等 |
| **合规要求严格** | 转录文本用于内部审计与合规存档，术语准确性直接影响法律风险 |
| **三方沟通** | 日方技术人员（日语）、中方操作人员（中文）、国际供应商/客户（英语）共同参会 |

### 1.2 问题描述

Meetily 使用 Whisper / Parakeet 进行本地语音识别（STT）。在危化品行业的日企场景中，存在以下叠加的识别挑战：

1. **跨语言术语混乱**：模型在日/中/英切换时，容易将一种语言的发音"听成"另一种语言的文字。
2. **化学物质名称识别率极低**：IUPAC 命名和日文片假名化学名在通用 STT 训练语料中几乎不存在。
3. **安全编码格式特殊**：CAS RN（如 108-88-3）、UN No.（如 UN 1203）、GHS 危险代码（H225, H301, H311）在语音转写中极易出错。
4. **片假名/汉字/罗马字混合**：日语中化学术语同时使用汉字、片假名、罗马字缩略，增加了模型 token 预测难度。

### 1.3 用户故事

| 角色 | 需求 |
|------|------|
| 安全管理部门负责人 | 希望法定安全术语被 100% 正确转录，不可出现模糊或错误 |
| 工厂值班长（中文母语） | 希望中文术语不会被音近字替换 |
| 日本本社技术工程师 | 希望日语片假名术语被正确转写 |
| 国际采购对接人 | 希望英语 CAS 编号不会被逐字母拼写 |
| 合规审计员 | 希望所有转录文本可直接归档，术语准确度达到可审计标准 |

### 1.4 目标

- **多语言支持**：术语表支持日语（汉字/平假名/片假名）、中文（简/繁）、英语三条并行通道
- **三级校正管道**：模型内 `initial_prompt` 软引导 → 正则精确替换 → LLM 上下文深度校正
- **用户可通过 UI 自定义行业专属术语表**
- **转录时实时应用前两级校正，录音停止后异步触发第三级校正**
- **处理延迟不影响实时转录体验**
- **校正结果可追溯、可审计、可回滚**

---

## 2. 核心问题分析

### 2.1 STT 输出的错误规律（按语言分类）

Whisper/Parakeet 等模型基于 subword token 级概率采样生成文本。专有名词/术语在训练数据中频率低，模型缺乏对其完整 token 序列的统计偏好。

#### 2.1.1 日语特有的错误模式

| 错误类型 | 示例 | 根因 |
|----------|------|------|
| 片假名 → 汉字误转 | `ポリウレタン` → `保利売れたん` | 模型对片假名化学术语的 token 覆盖不足 |
| 長音「ー」丢失 | `メチルエチルケトン` → `メチルエチルケトン` 的「ー」被省略 | whisper.cpp 对长音符 token 不敏感 |
| 促音「っ」丢失 | `引火性（いんかせい）` → `いんかせい` | 小さい「っ」的 token 在短音频段中易被丢弃 |
| 英语→片假名误回译 | 英文 `toluene` → 日文 `トルエン` 但模型输出 `トルーエン` | 过度音译 |
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
| IUPAC 命名拆分 | `2,4,6-Trinitrotoluene` → `two four six tri nitro toluene` | 数字+前缀的 token 序列模型不熟悉 |
| CAS 编号格式 | `CAS 108-88-3` → `k Ass one o eight eighty eight three` | 连字符导致 token 边界错误 |
| GHS 代码 | `H301` → `H three hundred one` | H+数字组合不在模型的常见 token 表内 |
| MSDS 缩写 | `LD50` → `L D fifty` | 无上下文时缩写被逐字母展开 |

#### 2.1.4 跨语言混合特有的错误模式

| 错误类型 | 示例 | 根因 |
|----------|------|------|
| 日语→中文误转 | `この物質は引火性があります` → `这个物质是银华星游戏吗` | 模型在语言切换点判断失误 |
| 中文→日语误转 | `闪点是负二十度` → `閃点は負にじゅうど` | 中→日 tokenizer 路径泄漏 |
| 英语→罗马音误转 | `flash point` → `フラッシュポイント` | 日英双语模式下的 token 竞争 |
| 代码混入自然语言 | `UN 1203` → `うん いちにーぜろさん` | 模型将代码视为日语假名发音 |

### 2.2 为什么不能把术语直接注入模型

- **Whisper（whisper.cpp）**：虽然提供 `initial_prompt` 参数可以偏置输出的 token 分布，但这是**软引导**而非硬约束。`initial_prompt` 通过 decoder 的 cross-attention 影响 logit 值，但不保证输出命中。`initial_prompt` 的 token 长度有限制（通常 224 token），不能直接塞入完整术语表。
- **Parakeet（ONNX Runtime）**：完全不支持 prompt 或词典注入。

因此，需要**分层策略**：先用 `initial_prompt` 在模型推理时做概率偏置（第一级），再用正则做确定性字符串替换（第二级），最后用 LLM 做上下文深度校正（第三级）。

### 2.3 各级能力的边界

| 能力 | L1 initial_prompt | L2 正则 | L3 LLM |
|------|:---:|:---:|:---:|
| 已知变体精确替换 | ❌ (非确定性) | ✅ | ✅ |
| 未知变体识别 | ❌ | ❌ (需预定义) | ✅ |
| 跨语言上下文消歧 | ❌ | ❌ | ✅ |
| 日语长音符/促音修复 | ❌ | ✅ | ✅ |
| 化学编码格式恢复 | ❌ | ✅ | ✅ |
| 同音字语境消歧 | ⚠️ (概率偏置) | ❌ | ✅ |
| 性能 (单次) | <10ms (参数注入) | <1ms/规则, 最多 ~50ms | 2-10s |
| 成本 | 免费 | 免费 | 取决于模型 |
| 确定性 | 否 | 是 | 否 |
| 对 Parakeet 有效 | ❌ | ✅ | ✅ |

### 2.4 设计决策：三级校正管道

```
三级管道总览：

第一级: initial_prompt  →   推理时注入，软偏置 token 分布。作用于每个 chunk 的模型推理阶段。
                           仅对 Whisper 引擎生效。Parakeet 用户跳过本级。
                           例: prompt 中含「過酸化物」→ 模型在该 token 候选上 logit 值提高。

第二级: 正则后处理      →   模型输出后即时执行，确定性替换。作用于 worker.rs 的统一出口。
                           对所有 STT 引擎（Whisper/Parakeet/Provider）均生效。
                           例: "かさんかぶつ" → "過酸化物" (已知变体精确替换)

第三级: LLM 深度校正    →   录音停止后异步执行，语义级修复。作用于整段转录文本。
                           默认仅建议，不自动应用；需用户确认后生效。
                           例: 结合上下文确定"容器"应为"反応器"
```

| 维度 | L1：initial_prompt | L2：正则实时 | L3：LLM 异步 |
|------|:---|:---|:---|
| **执行时机** | 每次 Whisper 推理时 | 每次转录输出后 | 录音停止后 |
| **延迟** | <10ms (参数注入) | <1ms/规则 (总计<50ms@200条) | 2-10s |
| **覆盖率（估计）** | 概率偏置 ~15-30% | 已知变体 ~50-60% | 未知变体+上下文 ~10-20% |
| **确定性与审计** | 低（概率性） | 高（确定性、可回放） | 中（需人工确认） |
| **成本** | 免费 | 免费 | Ollama 免费/API 付费 |
| **是否阻塞实时显示** | 否 | 否 | 否（异步） |
| **对 Parakeet 生效** | 否 | 是 | 是 |

---

## 3. 总体架构设计

### 3.1 系统架构图

```
┌──────────────────────────────────────────────────────────────────────────────┐
│                          前端 (Next.js + React)                               │
│                                                                              │
│  ┌─────────────────────────┐    ┌──────────────────────────────────────────┐ │
│  │ 术语管理 UI              │    │ 转录查看 / 会议详情                        │ │
│  │ TerminologyManager       │    │ - 实时转录面板 (L1+L2 校正后)              │ │
│  │ - 增删改查术语条目        │    │ - 深度校正建议 (L3 校正后)                │ │
│  │ - 按语言分类管理          │    │ - 差异高亮（原文 vs 校正）                 │ │
│  │ - 导入/导出 CSV           │    │ - 逐条接受/拒绝/回滚                      │ │
│  │ - 设置 initial_prompt 策略│    │                                           │ │
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
│  └───────────┬─────────────┘    │     + TERMINOLOGY_CACHE                   │ │
│              │                  └──────────────┬───────────────────────────┘ │
│              ▼                                  ▼                             │
│  ┌──────────────────────────┐    ┌──────────────────────────────────────────┐ │
│  │ SQLite: terminology 表    │    │ 内存缓存（2 个缓存，单次原子刷新）           │ │
│  │ (持久化, 含 lang 字段)    │    │                                          │ │
│  │                          │    │  ┌──────────────────────────────────┐    │ │
│  │ 多语言支持:               │    │  │ INITIAL_PROMPT_CACHE              │    │ │
│  │ lang: ja / zh / en       │    │  │ LazyLock<RwLock<String>>           │    │ │
│  │                          │    │  │ 用于 Whisper params.set_           │    │ │
│  │                          │    │  │      initial_prompt(&cache)        │    │ │
│  │                          │    │  └──────────────────────────────────┘    │ │
│  │                          │    │                                          │ │
│  │                          │    │  ┌──────────────────────────────────┐    │ │
│  │                          │    │  │ TERMINOLOGY_CACHE                  │    │ │
│  │                          │    │  │ LazyLock<RwLock<Vec<(Regex,Str)>>> │   │ │
│  │                          │    │  │ 预编译正则状态机 (按 original 长度   │    │ │
│  │                          │    │  │ 降序排列，确保最长模式优先匹配)       │    │ │
│  │                          │    │  └──────────────────────────────────┘    │ │
│  └──────────────────────────┘    └──────────────┬───────────────────────────┘ │
│                                                 │                             │
│  ┌──────────────────────────────────────────────┴───────────────────────────┐ │
│  │              转录管道 (audio/transcription + whisper_engine)              │ │
│  │                                                                          │ │
│  │  每个音频 chunk（仅 Whisper 引擎走 L1）:                                   │ │
│  │       │                                                                  │ │
│  │       ├──► 【L1 - 仅 Whisper】initial_prompt 注入                         │ │
│  │       │    WhisperEngine::transcribe_audio_with_confidence()              │ │
│  │       │    在 FullParams 构造后调用 params.set_initial_prompt(&cache)       │ │
│  │       │    效果: token 概率分布偏置                                        │ │
│  │       │                                                                  │ │
│  │       ├──► STT 模型推理（Whisper / Parakeet / Provider）                     │ │
│  │       │                                                                  │ │
│  │       ├──► 【L2 - 所有引擎】apply_terminology_correction(raw_text)          │ │
│  │       │    在 worker.rs 统一出口执行正则批量替换（单次遍历）                    │ │
│  │       │    效果: 确定性纠正已知变体                                          │ │
│  │       │                                                                  │ │
│  │       └──► emit("transcript-update", corrected_text)                       │ │
│  │            → 前端实时显示 (用户看到的是 L1+L2 校正后的文本)                     │ │
│  └──────────────────────────────────────────────────────────────────────────┘ │
│                                                                              │
│  ┌──────────────────────────────────────────────────────────────────────────┐ │
│  │              录音后处理 (PostRecording Processor)                          │ │
│  │                                                                          │ │
│  │  recording-stopped → 收集完整转录文本                                       │ │
│  │       │                                                                  │ │
│  │       ▼                                                                  │ │
│  │  ┌────────────────────────────────────┐                                  │ │
│  │  │ 【L3】llm_term_correct()           │  ← LLM 全文本深度校正              │ │
│  │  │      多语言术语表 + 行业上下文       │     2-10s (异步, 不阻塞 UI)      │ │
│  │  │      跨语言消歧 + 化学编码验证      │     默认仅建议，需用户确认         │ │
│  │  └────────────────┬───────────────────┘                                  │ │
│  │                   │                                                      │ │
│  │                   ▼                                                      │ │
│  │  保存校正建议至 transcript_corrections 表                                  │ │
│  │  状态: pending → 用户 accept/reject 后更新                                 │ │
│  └──────────────────────────────────────────────────────────────────────────┘ │
└──────────────────────────────────────────────────────────────────────────────┘
```

### 3.2 数据流时序

```
时间轴 ────────────────────────────────────────────────────────────────────►

用户点击开始录音
  │
  ├─► Rust: 原子加载 initial_prompt 缓存 + 正则缓存
  │        （两者从单次 DB 查询同步构建，保证一致性）
  │        Whisper 引擎: params.set_initial_prompt(...)
  │        Parakeet 引擎: 跳过 L1，直接进入 L2
  │
  ├─► worker.rs: 转录循环
  │     │
  │     ├─► STT 引擎推理
  │     │     ├─► 【L1 - 仅 Whisper】FullParams 已含 initial_prompt
  │     │     └─► 返回 raw_text
  │     │
  │     ├─► 【L2 - 所有引擎】apply_terminology_correction(raw_text)
  │     │     └─► 单次遍历预编译正则列表（最长模式优先）
  │     │
  │     └─► emit("transcript-update", L1+L2校正后文本)
  │           → 前端实时渲染
  │
  │     └─► ... 录音持续 ...

用户点击停止录音
  │
  ├─► recording-stopped 事件
  ├─► 保存 L1+L2 校正版转录到 DB (标记: correction_level = "l1_l2")
  │
  └─► 异步触发 【L3】LLM 校正建议 ←───── tokio::spawn, 不阻塞 UI
        │
        ├─► llm_suggest_corrections(
        │       full_transcript,
        │       terminology_table(ja/zh/en),
        │       industry_context = "危化品制造业"
        │    )
        │     ├─► 按 meeting_language 选择多语言 prompt
        │     ├─► 调用 LLM (默认 Ollama，支持降级)
        │     └─► 解析校正建议列表
        │
        ├─► validate_chemical_codes(校正后文本)  ← 规则级兜底
        ├─► 保存校正建议到 transcript_corrections 表 (status = "pending")
        │
        └─► emit("llm-corrections-ready") → 前端展示差异对比
             用户逐条 accept/reject → 更新 status
```

---

## 4. Phase 0：基线测量（前置条件）

> **重要**：在投入开发资源之前，必须先完成基线测量。没有基线，99% 准确率的目标无意义。

### 4.1 目的

量化当前 STT 引擎在危化品场景下的术语准确率，为后续 Phase 的优先级排序和效果评估提供数据依据。

### 4.2 测量方法

1. **准备测试音频集**（目标：至少 30 分钟）
   - 15 分钟模拟会议录音（日/中/英混合，含 ~50 个领域术语）
   - 15 分钟真实会议录音（如客户允许）
   - 人工标注 Ground Truth 文本
2. **分别在 Whisper 和 Parakeet 引擎上运行**
   - 记录原始转录输出
3. **按错误类别统计**
   - 使用 2.1 节的错误分类矩阵逐条标注
   - 计算每类错误的频率和占比
4. **计算基线 WER/CER 和关键术语准确率**

### 4.3 输出物

| 指标 | 说明 |
|------|------|
| 术语准确率（基线） | 关键术语被正确转录的比例 |
| 错误类别分布 | 各错误类型（片假名↔汉字、同音字、编码格式等）的占比 |
| 引擎差异报告 | Whisper vs Parakeet 在各语言上的表现差异 |
| 置信区间 | 由于测试集有限，标注准确率的 95% 置信区间 |

### 4.4 决策节点

基线测量完成后，根据结果决定：

- 如果基线术语准确率已 > 90%：Phase 1 的目标可调整为 > 95%
- 如果基线术语准确率 < 50%：L2 正则的覆盖率可能被严重高估，需重新评估
- 如果 Parakeet vs Whisper 差异显著：需调整 L1 的资源投入优先级
- **如果基线测量结果表明 L3 LLM 是唯一能解决 > 30% 错误的途径：考虑提前 Phase 2**

### 4.5 预估工作量

1-2 人天（假设已有日语/中文母语者协助标注）。

---

## 5. 第一级：Whisper initial_prompt 软引导

### 5.1 实现原理

Whisper.cpp 暴露了 `initial_prompt` 参数。其底层机制是：

```
Whisper Encoder → Cross-Attention → Decoder
                       ▲
              initial_prompt tokens
              作为 decoder 的前缀 token 送入

原理: prompt token 通过 transformer decoder 的 cross-attention 机制，
     使得与 prompt token 语义/拼写相近的 token 在输出 logit 分布中获得更高值。
     这是一种"软引导"——不强制输出，但提高了特定术语的概率。
```

**关键限制**：
- prompt 的 token 长度有上限（whisper.cpp 默认 224 token）
- 不是硬约束，模型仍可能输出其他内容
- **Parakeet 不支持此参数，本级别仅对 Whisper 引擎生效**
- **依赖 whisper-rs 0.13.x 暴露 `set_initial_prompt` API — Phase 0 期间需验证**

### 5.2 API 可用性验证（开发前必做）

```rust
// 在 Phase 0 期间执行此验证，确认 whisper-rs 的 FullParams 是否暴露 initial_prompt
// 检查路径：whisper-rs 0.13.2 的 FullParams 结构体
// 如果不存在此 API，需要评估是否升级 whisper-rs 版本或提交 feature request
```

### 5.3 Prompt 构建策略

从用户设置的录音语言偏好决定注入哪些术语：

```rust
fn build_initial_prompt(language: Option<&str>, high_priority_terms: &[String]) -> String {
    let base = match language {
        Some("ja") | Some("jp") =>
            "危険化学品製造会議。以下の用語が含まれる可能性がある：",
        Some("zh") | Some("cn") =>
            "危险化学品制造会议。以下术语可能出现：",
        Some("en") =>
            "Hazardous chemical manufacturing meeting. Terms:",
        _ =>
            "Chemical safety meeting discussing hazardous materials.",
    };

    let terms = high_priority_terms.join(", ");
    format!("{} {}", base, terms)
}
```

### 5.4 日/中/英多语言 prompt 示例

**日语会议时**：
```
危険化学品製造会議。以下の用語が含まれる可能性がある：
過酸化物, 引火性液体, 毒劇物, ポリウレタン, エポキシ樹脂,
トルエンジイソシアネート, メチルエチルケトン, 爆発性, 急性毒性,
特定化学物質, 有機溶剤, 作業環境測定, GHS分類, SDS, CAS番号,
PRTR法, 安衛法, 消防法
```

**中文会议时**：
```
危险化学品制造会议。以下术语可能出现：
甲苯二异氰酸酯, 二苯基甲烷二异氰酸酯, 苯乙烯, 环氧树脂, 聚氨酯,
过氧化物, 易燃液体, 急性毒性, 特定化学物质, 有机溶剂, 作业环境测定,
安全数据表, GHS分类, CAS编号, 危险货物编号, 重大危险源, 应急预案
```

**英语会议时**：
```
Hazardous chemical manufacturing meeting. Terms:
toluene diisocyanate, methylene diphenyl diisocyanate, styrene monomer,
epoxy resin, polyurethane, peroxide, flammable liquid, acute toxicity,
LD50, LC50, GHS hazard statements H225 H301 H311, CAS registry number,
UN number, Safety Data Sheet, threshold limit value, permissible exposure limit
```

### 5.5 代码集成位置

**文件位置**：`frontend/src-tauri/src/whisper_engine/whisper_engine.rs`

在 `transcribe_audio_with_confidence()` 函数中，`FullParams` 构造完成后、`state.full()` 之前插入：

```rust
// 现有代码: let mut params = FullParams::new(...);

// ===== 新增: L1 initial_prompt 注入（仅 Whisper 引擎） =====
let language = language.clone();
let initial_prompt = terminology::cache::get_initial_prompt(language.as_deref());
if !initial_prompt.is_empty() {
    // 注意：需在 Phase 0 验证 whisper-rs 0.13.x 是否暴露此 API
    params.set_initial_prompt(&initial_prompt);
    log::debug!(
        "L1 initial_prompt injected ({} chars, lang: {:?})",
        initial_prompt.len(),
        language
    );
}
// ===============================================================

// 继续原有流程:
let mut state = ctx.create_state()?;
state.full(params, &audio_data)?;
```

### 5.6 Prompt 缓存

```rust
// terminology/cache.rs

use std::sync::LazyLock;
use std::sync::RwLock;

/// 全局 initial_prompt 缓存 — 启动时从术语表生成，术语变更时通过
/// refresh_all_terminology_caches() 与正则缓存原子刷新。
/// 内容为逗号分隔的高优先级术语列表（按语言分组）。
static INITIAL_PROMPT_BY_LANG: LazyLock<RwLock<HashMap<String, String>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

/// 从已加载的术语条目列表构建 initial_prompt（由 refresh_all_terminology_caches 调用）
fn build_initial_prompts(entries: &[TerminologyEntry]) -> HashMap<String, String> {
    let mut by_lang: HashMap<String, Vec<String>> = HashMap::new();
    for entry in entries {
        if !entry.enabled || entry.priority != "high" {
            continue;
        }
        by_lang.entry(entry.language.clone())
            .or_default()
            .push(entry.replacement.clone());
    }

    let mut lang_map = HashMap::new();
    for (lang, mut terms) in by_lang {
        terms.sort();
        terms.dedup();
        // 控制在 200 token 以内（日语约 100 词，中文约 80 词，英语约 60 词）
        let joined = terms.join(", ");
        lang_map.insert(lang, joined);
    }
    lang_map
}

/// 获取指定语言的 initial_prompt
pub fn get_initial_prompt(language: Option<&str>) -> String {
    let lang_key = match language {
        Some("ja") | Some("jp") => "ja",
        Some("zh") | Some("cn") => "zh",
        Some("en") => "en",
        _ => "auto",
    };

    INITIAL_PROMPT_BY_LANG
        .read()
        .ok()
        .and_then(|c| c.get(lang_key).cloned())
        .unwrap_or_default()
}
```

---

## 6. 第二级：正则实时校正通道

### 6.1 实现原理

所有转录结果（无论 Whisper 还是 Parakeet）在 `worker.rs` 的 `transcribe_chunk_with_provider` 函数中汇聚。在引擎返回原始文本后、发送 `transcript-update` 事件前，插入一个 `apply_terminology_correction()` 后处理步骤。

**L1 → L2 的协同**：`initial_prompt`（仅 Whisper）让模型更大概率输出正确术语或其近形变体，正则则将这些残余偏差**确定性地修正**。L2 对所有引擎生效。

### 6.2 多语言正则的特殊处理

日语、中文的正则匹配与英语有本质差异：

| 语言 | 词边界 `\b` | 注意事项 |
|------|:---:|------|
| 英语 | ✅ 有效 | `\b` 基于 `\w` 与 `\W` 的边界 |
| 日语 | ❌ 不可用 | 片假名、平假名、汉字在 regex 中均为 `\w`，`\b` 无法正确判定词边界 |
| 中文 | ❌ 不可用 | 汉字之间无空格分隔，`\b` 无效 |

**日语的全词匹配替代方案**：

```rust
fn build_japanese_word_boundary(pattern: &str) -> String {
    format!(
        r"(?<![\p{{Han}}\p{{Hiragana}}\p{{Katakana}}ー]){}(?![\p{{Han}}\p{{Hiragana}}\p{{Katakana}}ー])",
        regex::escape(pattern)
    )
}
```

**中文的全词匹配替代方案**：

```rust
fn build_chinese_word_boundary(pattern: &str) -> String {
    format!(
        r"(?<![\p{{Han}}])({})(?![\p{{Han}}])",
        regex::escape(pattern)
    )
}
```

### 6.3 核心数据结构与实现

```rust
use regex::Regex;
use std::borrow::Cow;

/// 编译后的术语校正规则
struct TerminologyRule {
    regex: Regex,
    replacement: String,
    /// 匹配模式的字符数，用于排序（最长优先）
    original_len: usize,
}

/// 构建术语匹配模式（根据语言选择合适的词边界）
fn build_term_pattern(entry: &TerminologyEntry) -> String {
    let escaped = regex::escape(&entry.original);

    if !entry.whole_word {
        let case_flag = if entry.case_sensitive { "" } else { "(?i)" };
        return format!("{}{}", case_flag, escaped);
    }

    match entry.language.as_str() {
        "ja" => build_japanese_word_boundary(&entry.original),
        "zh" => build_chinese_word_boundary(&entry.original),
        _ => {
            let case_flag = if entry.case_sensitive { "" } else { "(?i)" };
            format!(r"{}\b{}\b", case_flag, escaped)
        }
    }
}

/// 刷新术语正则缓存。与 initial_prompt 缓存在同一函数中原子刷新。
pub fn rebuild_terminology_regex_cache(entries: &[TerminologyEntry]) -> Vec<TerminologyRule> {
    let mut entries: Vec<_> = entries.iter().filter(|e| e.enabled).collect();

    // 关键：按 original（匹配模式）的字符数降序排列，确保最长模式优先匹配
    // 避免 "酸" 在 "過酸化物" 之前匹配导致的错误截断
    entries.sort_by(|a, b| {
        b.original.chars().count()
            .cmp(&a.original.chars().count())
    });

    let mut rules = Vec::with_capacity(entries.len());
    for entry in entries {
        let pattern = build_term_pattern(entry);
        match Regex::new(&pattern) {
            Ok(re) => rules.push(TerminologyRule {
                regex: re,
                replacement: entry.replacement.clone(),
                original_len: entry.original.chars().count(),
            }),
            Err(e) => {
                log::warn!(
                    "Failed to compile terminology regex for '{}': {}",
                    entry.original, e
                );
            }
        }
    }

    log::info!("Terminology regex cache rebuilt with {} rules", rules.len());
    rules
}

/// 对转录文本应用术语校正。
/// 使用 Cow<str> 避免无匹配时的字符串分配。
/// 单次遍历所有规则，在首次匹配时才分配新字符串。
pub fn apply_terminology_correction<'a>(text: &'a str, rules: &[TerminologyRule]) -> Cow<'a, str> {
    if rules.is_empty() {
        return Cow::Borrowed(text);
    }

    let mut result = Cow::Borrowed(text);
    for rule in rules {
        // 仅在确实需要替换时才分配/修改
        if rule.regex.is_match(&result) {
            let owned = result.into_owned();
            result = Cow::Owned(rule.regex.replace_all(&owned, rule.replacement.as_str()).to_string());
        }
    }
    result
}
```

### 6.4 性能基准（开发中需验证）

| 规则数 | 文本长度 | 预估耗时 | 备注 |
|--------|----------|----------|------|
| 50 | 200 chars | < 1ms | 典型会议 chunk 场景 |
| 100 | 200 chars | < 2ms | |
| 200 | 200 chars | < 5ms | 预编译正则 + Cow 优化 |
| 500 | 200 chars | < 20ms | 上限场景，需实际测量 |

> **注意**：以上为预估值。Phase 1 实现后需在目标硬件上进行基准测试。如果超过 50ms，应考虑引入 Aho-Corasick 用于纯字面量匹配的子集，保留正则仅用于需要 Unicode 属性断言的术语。

### 6.5 调用位置

在 `worker.rs` 的 `transcribe_chunk_with_provider` 函数中，Whisper 分支和 Parakeet 分支的 `cleaned_text` 赋值后统一插入 L2 校正：

```rust
// 原有代码（以 Whisper 分支为例，line ~456）:
let cleaned_text = text.trim().to_string();

// 新增 L2 正则校正（对所有引擎统一处理）:
let rules = terminology::cache::get_terminology_rules();
let cleaned_text = apply_terminology_correction(&cleaned_text, &rules).into_owned();
```

---

## 7. 第三级：LLM 深度校正通道

### 7.1 触发时机

录音停止 → 转录保存到 DB（L1+L2 校正版）→ 异步触发 L3 LLM 校正建议 → 保存为 pending 状态 → 前端通知用户 → 用户逐条确认

**关键设计决策**：L3 校正默认**仅建议，不自动应用**。原因：
- 危化品行业转录用于合规审计，自动修改可能引入法律风险
- LLM 输出非确定性，无法保证 100% 正确
- 用户可通过设置开启自动接受（需明确知情同意）

### 7.2 LLM Provider 选择

推荐度排序：

| Provider | 多语言能力 | 延迟 | 成本 | 内存占用 | 推荐度 |
|----------|:---:|------|------|------|:---:|
| Ollama `qwen2.5:14b` | 日/中/英均优 | 3-8s | 免费 | ~9GB | ⭐⭐⭐ |
| Ollama `qwen2.5:7b` | 中/英优，日可接受 | 2-5s | 免费 | ~5GB | ⭐⭐⭐ |
| Ollama `qwen2.5:3b` | 中/英可接受，日弱 | 1-2s | 免费 | ~2.5GB | ⭐⭐ |
| Ollama `llama3.1:8b` | 英优，中/日弱 | 3-8s | 免费 | ~6GB | ⭐⭐ |
| Claude API (Sonnet) | 日/中/英均优 | 2-4s | ~$0.02/次 | N/A | ⭐⭐ |
| Built-in sidecar | 取决于模型 | 取决于硬件 | 免费 | 取决于模型 | ⭐⭐⭐ |

**推荐策略**：默认使用 `qwen2.5:7b`（平衡多语言能力与内存占用），允许用户切换到更高精度模型。低内存设备自动降级至 `qwen2.5:3b` 或禁用 L3。

**不需要 Thinking 模型**：术语校正本质是"跨语言匹配+替换"，不是多步推理链。

### 7.3 Prompt 设计

```markdown
## システムプロンプト / 系统提示 / System Prompt

あなたは Meetily の文字起こし校正器です。
你是 Meetily 的转录文本校正器。
You are Meetily's transcript corrector.

### 任務 / 任务 / Task
危険化学品製造会議の音声認識結果を修正してください。
请修正危险化学品制造会议的语音识别结果。
Correct the speech recognition output from a hazardous chemical manufacturing meeting.

### 会議言語 / 会议语言 / Meeting Languages
本会議では、日本語・中国語・英語が混在します。
本会议同时使用日语、中文、英语。
This meeting mixes Japanese, Chinese, and English.

### 処理対象のエラー / 需要处理的错误 / Error Types to Fix
1. 片仮名化学物質名の分割誤り（例：「ポリ ウレ たん」→「ポリウレタン」）
2. 中国語化学名の同音異字誤り（例：「甲本二亿情酸纸」→「甲苯二异氰酸酯」）
3. CAS番号・UN番号の誤認識（例：「k Ass one o eight」→「CAS 108-88-3」）
4. GHSコードの形式誤り（例：「H two twenty five」→「H225」）
5. 言語切替時の混同（例：日本語なのに中国語で出力される）

### 用語リファレンス / 术语参考 / Terminology Reference
%TERMINOLOGY_TABLE%

### 校正ルール / 校正规则 / Correction Rules
✅ 修正してよいもの:
   - 用語表にある用語への明らかな音声認識誤り
   - 化学物質名の表記ゆれ統一（IUPAC名優先）
   - CAS/UN番号・GHSコードの標準形式への修正
   - 言語切り替えの明らかな誤判定
   - 専門用語の大文字・小文字・全角・半角の正規化

❌ 絶対にしないこと:
   - 語順・文体・語態の変更
   - 文法の添削や表現の改善
   - 文章の分割や結合
   - 元にない情報の追加
   - 内容の削除や省略
   - 確信が持てない箇所の修正（見逃しは誤修正に勝る）

### 出力形式 / 输出格式 / Output Format
以下のJSON形式で修正提案を出力：
[{"original": "...", "corrected": "...", "reason": "...", "language": "ja/zh/en"}]
修正不要の場合は空リスト [] を出力。

### 元の文字起こし / 原始转录 / Original Transcript
%TRANSCRIPT_TEXT%
```

### 7.4 化学编码验证

LLM 校正建议在保存前经过确定性规则验证（不需要 LLM 推理）：

```rust
/// 化学编码格式验证与自动修复
/// 规则级兜底：无论 LLM 输出如何，这些格式修复总是安全的
fn validate_chemical_codes(text: &str) -> String {
    let mut result = text.to_string();

    // GHS 危险代码: H + 3位数字 → 紧凑格式
    if let Ok(re) = Regex::new(r"\bH\s*(\d)\s*(\d)\s*(\d)\b") {
        result = re.replace_all(&result, "H$1$2$3").to_string();
    }

    // UN 编号: UN + 4位数字 → 标准化空格
    if let Ok(re) = Regex::new(r"(?i)UN\s*(\d)\s*(\d)\s*(\d)\s*(\d)\b") {
        result = re.replace_all(&result, "UN $1$2$3$4").to_string();
    }

    // CAS RN 格式验证（仅日志告警，不自动修复 — CAS 格式多样，自动修复有风险）
    if let Ok(re) = Regex::new(r"(?i)cas\s*#?\s*(\d{2,7})\s*[-—–]\s*(\d{2})\s*[-—–]\s*(\d)") {
        if !re.is_match(&result) {
            log::warn!("No valid CAS RN format found in corrected text");
        }
    }

    result
}
```

### 7.5 实现代码规范

**调用链路**：

```
recording-stopped
  → 保存 L1+L2 转录到 DB (correction_level = "l1_l2")
  → tokio::spawn(async { llm_correction_task(meeting_id) })
    ├─► 加载术语表
    ├─► 构建多语言 prompt
    ├─► 调用 LLM
    ├─► 解析 JSON 校正建议列表
    ├─► validate_chemical_codes()
    ├─► 逐条写入 transcript_corrections (status = "pending")
    └─► emit("llm-corrections-ready", { meeting_id, correction_count })
```

### 7.6 LLM 不可用时的降级

```
L3 触发
  ├─► 尝试 Ollama 本地 (qwen2.5:7b)  → 成功 → 保存建议
  │                                    → 失败 ↓
  ├─► 尝试 Built-in sidecar          → 成功 → 保存建议
  │                                    → 失败 ↓
  ├─► 尝试降级模型 (qwen2.5:3b)      → 成功 → 保存建议（标注降级）
  │                                    → 失败 ↓
  └─► 静默跳过，记录日志。不阻塞主流程。
```

---

## 8. 数据库设计

### 8.1 新建表：`terminology`

**迁移文件**：`migrations/20260427000000_add_terminology.sql`

```sql
CREATE TABLE IF NOT EXISTS terminology (
    id               TEXT PRIMARY KEY,
    original         TEXT NOT NULL,
    replacement      TEXT NOT NULL,
    language         TEXT NOT NULL DEFAULT 'auto',  -- 'ja' | 'zh' | 'en' | 'auto'
    case_sensitive   INTEGER NOT NULL DEFAULT 0,
    whole_word       INTEGER NOT NULL DEFAULT 1,
    enabled          INTEGER NOT NULL DEFAULT 1,
    priority         TEXT NOT NULL DEFAULT 'normal', -- 'high' | 'normal' | 'low'
    category         TEXT NOT NULL DEFAULT 'general',
    description      TEXT,
    created_at       TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at       TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_terminology_language ON terminology(language);
CREATE INDEX IF NOT EXISTS idx_terminology_category ON terminology(category);
CREATE INDEX IF NOT EXISTS idx_terminology_priority ON terminology(priority);
CREATE INDEX IF NOT EXISTS idx_terminology_enabled ON terminology(enabled);
```

### 8.2 新建表：`transcript_corrections`

```sql
CREATE TABLE IF NOT EXISTS transcript_corrections (
    id               TEXT PRIMARY KEY,
    meeting_id       TEXT NOT NULL,
    original_text    TEXT NOT NULL,
    corrected_text   TEXT NOT NULL,
    correction_level TEXT NOT NULL DEFAULT 'l2',  -- 'l1_l2' | 'l3' | 'manual'
    correction_type  TEXT NOT NULL DEFAULT 'auto', -- 'regex' | 'llm' | 'manual'
    status           TEXT NOT NULL DEFAULT 'pending', -- 'pending' | 'accepted' | 'rejected'
    language         TEXT,
    reviewed_by      TEXT,
    created_at       TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (meeting_id) REFERENCES meetings(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_corrections_meeting ON transcript_corrections(meeting_id);
CREATE INDEX IF NOT EXISTS idx_corrections_level ON transcript_corrections(correction_level);
```

### 8.3 现有表扩展（需幂等处理）

SQLite 不支持 `ALTER TABLE ADD COLUMN IF NOT EXISTS`。迁移时需先检查列是否存在：

```sql
-- transcripts 表：存储各级校正版本快照（JSON 结构，支持部分接受/拒绝）
-- 仅在列不存在时添加
-- 格式: {"l1_l2": "text after L1+L2", "l3_raw": "LLM suggested full text", "final": "user-confirmed text"}

-- settings 表新增字段（同样需幂等检查）
-- terminology_enabled: 1 = 启用术语校正总开关
-- initial_prompt_enabled: 1 = 启用 L1 (仅对 Whisper 有效)
-- llm_correction_enabled: 1 = 启用 L3 LLM 校正建议
-- llm_correction_auto_accept: 0 = 默认仅建议, 1 = 自动接受（需用户明确开启）
```

**Rust 迁移代码示例**（幂等实现）：

```rust
// 在 setup.rs 的 migration 函数中
async fn add_column_if_not_exists(pool: &SqlitePool, table: &str, column: &str, definition: &str) -> Result<()> {
    let query = format!("SELECT COUNT(*) FROM pragma_table_info('{}') WHERE name = '{}'", table, column);
    let count: (i64,) = sqlx::query_as(&query).fetch_one(pool).await?;
    if count.0 == 0 {
        let alter = format!("ALTER TABLE {} ADD COLUMN {} {}", table, column, definition);
        sqlx::query(&alter).execute(pool).await?;
    }
    Ok(())
}
```

### 8.4 数据模型（Rust）

**文件位置**：`frontend/src-tauri/src/database/models.rs`（追加）

```rust
/// 术语条目
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

/// 转录校正记录（审计用）
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct TranscriptCorrection {
    pub id: String,
    pub meeting_id: String,
    pub original_text: String,
    pub corrected_text: String,
    pub correction_level: String,
    pub correction_type: String,
    pub status: String,
    pub language: Option<String>,
    pub reviewed_by: Option<String>,
    pub created_at: String,
}
```

### 8.5 仓库层（新增）

**文件位置**：`frontend/src-tauri/src/database/repositories/terminology.rs`（新建）

```rust
impl DatabaseManager {
    pub async fn get_all_terminology(&self) -> Result<Vec<TerminologyEntry>>;
    pub async fn get_terminology_by_language(&self, language: &str) -> Result<Vec<TerminologyEntry>>;
    pub async fn get_enabled_terminology(&self) -> Result<Vec<TerminologyEntry>>;
    pub async fn save_terminology(&self, entries: Vec<TerminologyEntry>) -> Result<()>;
    pub async fn delete_terminology(&self, id: &str) -> Result<()>;
    pub async fn create_correction(&self, correction: TranscriptCorrection) -> Result<()>;
    pub async fn update_correction_status(&self, id: &str, status: &str) -> Result<()>;
    pub async fn get_corrections_for_meeting(&self, meeting_id: &str) -> Result<Vec<TranscriptCorrection>>;
}
```

---

## 9. 后端实现规范

### 9.1 模块组织结构

```
frontend/src-tauri/src/
├── terminology/                     # 新建模块
│   ├── mod.rs                       # 模块入口
│   ├── cache.rs                     # 统一缓存管理
│   │   ├── INITIAL_PROMPT_BY_LANG   #   L1: initial_prompt 缓存（按语言）
│   │   ├── TERMINOLOGY_RULES         #   L2: 预编译正则缓存（按 original 长度降序）
│   │   ├── refresh_all_caches()     #   原子刷新两个缓存（从 DB 一次性加载）
│   │   ├── get_initial_prompt()     #   获取指定语言的 L1 prompt
│   │   └── get_terminology_rules()  #   获取 L2 正则规则列表
│   ├── commands.rs                  # Tauri 命令 (CRUD + 缓存刷新)
│   └── corrector.rs                 # 校正器
│       ├── apply_terminology_correction()    # L2 正则替换（Cow<str> 优化）
│       ├── validate_chemical_codes()         # L3 后规则级验证
│       └── llm_correct_terminology()         # L3 LLM 校正建议生成
│
├── whisper_engine/
│   └── whisper_engine.rs           # L1 集成 (仅对 Whisper 引擎注入 initial_prompt)
│
├── audio/transcription/worker.rs   # L2 集成 (对所有引擎统一调用 apply_terminology_correction)
│
├── summary/llm_client.rs           # L3: 新增 suggest_terminology_corrections() 函数
│
├── database/
│   ├── models.rs                   # 追加 TerminologyEntry, TranscriptCorrection
│   ├── repositories/
│   │   └── terminology.rs          # 新建仓库文件
│   └── setup.rs                    # 追加幂等迁移逻辑
│
├── lib.rs                          # 注册新命令
│
└── migrations/
    └── 20260427000000_add_terminology.sql
```

### 9.2 Tauri 命令注册

在 `lib.rs` 的 `invoke_handler` 中新增：

```rust
// ===== 术语管理 CRUD =====
terminology::commands::get_terminology_list,
terminology::commands::save_terminology_entry,
terminology::commands::delete_terminology_entry,
terminology::commands::import_terminology_csv,
terminology::commands::export_terminology_csv,

// 统一缓存刷新（L1+L2 原子刷新）
terminology::commands::refresh_all_terminology_caches,

// L3 LLM 校正
terminology::commands::run_llm_terminology_correction,
terminology::commands::get_corrections_for_meeting,
terminology::commands::accept_correction,
terminology::commands::reject_correction,

// 设置项
terminology::commands::get_terminology_settings,
terminology::commands::set_terminology_settings,
```

### 9.3 启动时初始化（统一缓存刷新）

在 `lib.rs` 的 `setup` 闭包中，数据库迁移完成后：

```rust
// 原子初始化 L1+L2 缓存（从 DB 单次加载，保证一致性）
let db = app.state::<state::AppState>().db_manager.clone();
tauri::async_runtime::spawn(async move {
    if let Err(e) = terminology::cache::refresh_all_caches(&db).await {
        log::warn!("Failed to initialize terminology caches: {}", e);
    } else {
        log::info!("Terminology caches initialized (L1 initial_prompt + L2 regex)");
    }
});
```

### 9.4 统一缓存刷新实现

```rust
// terminology/cache.rs

/// 原子刷新 L1 和 L2 缓存。从 DB 单次加载术语表，同时构建两个缓存。
/// 保证 L1 和 L2 基于完全相同的术语条目集合。
pub async fn refresh_all_caches(db: &DatabaseManager) -> Result<(), String> {
    let entries = db.get_enabled_terminology().await
        .map_err(|e| format!("Failed to load terminology: {}", e))?;

    // 同时构建 L1 prompt 和 L2 regex
    let prompts = build_initial_prompts(&entries);
    let rules = rebuild_terminology_regex_cache(&entries);

    // 原子写入（虽然有两个 RwLock，但基于同一批 entries）
    *INITIAL_PROMPT_BY_LANG.write().map_err(|e| e.to_string())? = prompts;
    *TERMINOLOGY_RULES.write().map_err(|e| e.to_string())? = rules;

    log::info!(
        "All caches refreshed: {} languages in L1, {} rules in L2",
        INITIAL_PROMPT_BY_LANG.read().unwrap().len(),
        TERMINOLOGY_RULES.read().unwrap().len()
    );
    Ok(())
}
```

---

## 10. 前端实现规范

### 10.1 新增组件

| 组件 | 文件路径 | 说明 |
|------|----------|------|
| `TerminologyManager` | `components/TerminologyManager/index.tsx` | 术语管理主面板：表格 + 操作栏 + 语言筛选 |
| `TerminologyEntryRow` | `components/TerminologyManager/EntryRow.tsx` | 单条术语编辑行（含语言选择器） |
| `TerminologyImportDialog` | `components/TerminologyManager/ImportDialog.tsx` | CSV 导入对话框（含编码检测预览） |
| `CorrectionDiffView` | `components/CorrectionDiff/DiffView.tsx` | 校正差异对比视图 |

### 10.2 会议详情页 — 校正差异视图

```
┌──────────────────────────────────────────────────────────────┐
│  文字起こし校正 / 转录校正                          [查看原文] │
│                                                              │
│  ☐ 显示 L1+L2 实时校正    ☑ 显示 L3 深度校正建议              │
│                                                              │
│  ┌─ L3 校正建议 (3 条, 待确认) ──────────────────────────┐   │
│  │                                                       │   │
│  │ 🟡 化学名: 当該物質 → 当該過酸化物        [接受] [拒绝] │   │
│  │    原文: 「当該物質の引火点は...」                     │   │
│  │    校正: 「当該過酸化物の引火点は...」                 │   │
│  │                                                       │   │
│  │ 🔵 略語展開: TDI → トルエンジイソシアネート (TDI)      │   │
│  │    原文: 「TDI と MDI の混合比は...」    [接受] [拒绝] │   │
│  │    校正: 「トルエンジイソシアネート (TDI) と MDI...」   │   │
│  │                                                       │   │
│  │ 🟢 安全: 保護具 → 保護具（手袋・保護メガネ）           │   │
│  │    原文: 「取り扱いには保護具が必要」     [接受] [拒绝] │   │
│  │    校正: 「取り扱いには保護具（手袋・保護メガネ）が必要」│   │
│  │                                                       │   │
│  └───────────────────────────────────────────────────────┘   │
│                                                              │
│  [全部接受]  [全部拒绝]    最后校正: 2026-04-27 14:32 JST    │
└──────────────────────────────────────────────────────────────┘
```

### 10.3 TypeScript 类型定义

**文件位置**：`frontend/src/types/terminology.ts`（新建）

```typescript
export type TerminologyLanguage = 'ja' | 'zh' | 'en' | 'auto';
export type TerminologyPriority = 'high' | 'normal' | 'low';

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
  originalText: string;
  correctedText: string;
  correctionLevel: 'l1_l2' | 'l3' | 'manual';
  correctionType: 'regex' | 'llm' | 'manual';
  status: 'pending' | 'accepted' | 'rejected';
  language?: TerminologyLanguage;
  reviewedBy?: string;
  createdAt: string;
}

export interface TerminologySettings {
  terminologyEnabled: boolean;
  initialPromptEnabled: boolean;
  llmCorrectionEnabled: boolean;
  llmCorrectionAutoAccept: boolean;  // 默认 false, 需用户明确开启
  llmProvider: string;
  llmModel: string;
}
```

---

## 11. 配置与存储

### 11.1 配置存储

术语表存储在 SQLite 数据库的 `terminology` 表中（参见第 8 节）。

各级开关存储在 `settings` 表的扩展字段中：
- `terminology_enabled`：总开关（0 = 禁用所有校正）
- `initial_prompt_enabled`：L1 开关（仅对 Whisper 有效）
- `llm_correction_enabled`：L3 开关
- `llm_correction_auto_accept`：L3 自动接受开关（默认 0，需用户明确开启）
- L2 正则**不可**单独关闭（它是确定性的，总是无害的）

### 11.2 CSV 导入/导出格式（多语言版）

导入格式（UTF-8 BOM, 逗号分隔, 带表头）：

```csv
original,replacement,language,case_sensitive,whole_word,enabled,priority,category,description
かさんかぶつ,過酸化物,ja,false,true,true,high,化学物質,片仮名→漢字修正
ポリ ウレ たん,ポリウレタン,ja,false,true,true,high,化学物質,分割誤り修正
甲本二亿情酸纸,甲苯二异氰酸酯,zh,false,true,true,high,化学物質,同音字修正
本以西,苯乙烯,zh,false,true,true,high,化学物質,同音字修正
two four six tri nitro toluene,2,4,6-Trinitrotoluene,en,true,false,true,high,化学物質,IUPAC命名修正
H two twenty five,H225,en,false,true,true,high,GHSコード,危険有害性コード
```

导入时自动检测编码（检测顺序：BOM → UTF-8 有效性 → Shift-JIS 解码尝试 → 用户手动选择）。前端预览前 5 行后再确认导入。

### 11.3 内置预置术语包

在 `frontend/src-tauri/templates/preset_terminology/` 下：

```
preset_terminology/
├── chemical_ja.csv            # 日本語 — 危険化学品製造用語（約50条）
├── chemical_zh.csv            # 中文 — 危险化学品制造术语（約50条）
├── chemical_en.csv            # English — Hazardous Chemical Manufacturing（約50条）
├── ghs_codes.csv              # GHS危険有害性情報コード（約30条）
├── cas_common.csv             # よく使われるCAS番号（約20条）
└── README.md                  # 利用方法と貢献ガイド
```

> **注意**：预置术语包从 V2.0 的 ~370 条缩减至 ~200 条，去除了冗余条目。
> 用户可按需扩展。预置包提供的是"高概率错误"的覆盖，而非穷举。

---

## 12. 硬件要求与降级策略

### 12.1 最低配置

| 组件 | 最低配置 | 推荐配置 |
|------|----------|----------|
| RAM | 8GB (L1+L2 only) | 16GB+ (含 L3 LLM) |
| VRAM/GPU | 不需要 (CPU STT) | 4GB+ (GPU STT + LLM 可共存) |
| 磁盘空间 | 500MB (STT 模型 + 术语表) | 3-5GB (含 LLM 模型) |
| OS | Windows 10+ / macOS 13+ / Linux | 同左 |

### 12.2 降级策略

系统启动时自动检测硬件，按以下优先级启用功能：

```
检测 RAM
  │
  ├─► < 8GB: 仅 L2 正则校正。L1/L3 禁用。
  │          提示用户：内存不足，LLM 校正不可用。
  │
  ├─► 8-12GB: L1 + L2 启用。
  │           L3 尝试 3B 模型（qwen2.5:3b），若不可用则跳过。
  │           提示用户：深度校正可用但精度受限。
  │
  └─► > 12GB: L1 + L2 + L3 全部启用。
             L3 默认 7B 模型，用户可切换至 14B。
```

### 12.3 电源策略（移动设备）

- 电池供电时：L3 默认禁用（LLM 推理功耗 ~20-50W，显著影响续航）
- 插电时：L1+L2+L3 全部可用
- 用户可在设置中覆盖此行为

---

## 13. 实施计划与 MVP 策略

### 13.1 MVP 定义

> **核心原则**：先交付价值最高的部分，尽快验证效果。

| 阶段 | 内容 | 交付价值 | 预估工作量 |
|------|------|----------|:---:|
| **Phase 0** | 基线测量（录音样本采集 + 错误标注 + WER 计算） | 数据驱动的优先级排序 | 1-2 人天 |
| **Phase 1A (MVP)** | L2 正则校正 + 数据库 + 基础术语管理 UI | ~50% 错误覆盖，确定性，零额外延迟 | 3-4 人天 |
| **Phase 1B** | L1 initial_prompt（仅 Whisper）+ CSV 导入导出 | 追加 ~15-20% 覆盖 | 2 人天 |
| **Phase 2** | L3 LLM 校正建议 + 差异对比 UI | 追加 ~10-20% 覆盖（未知变体） | 3-4 人天 |
| **Phase 3** | 完善体验：编码自动检测、审计报告、注音辅助 | 合规与易用性 | 2-3 人天 |

**总计：11-15 人天**

### 13.2 MVP（Phase 1A）的核心范围

如果资源有限，以下为最小可交付：

1. `terminology` 表 + 迁移
2. L2 正则缓存 + `apply_terminology_correction()` 集成到 `worker.rs`
3. 基础术语管理 UI（表格 + 新增/删除/筛选）
4. 预置术语包（精简版 50 条覆盖率最高的术语）

**MVP 不包含**：L1 initial_prompt、L3 LLM 校正、CSV 导入导出、差异对比视图。

### 13.3 Phase 依赖关系

```
Phase 0 (基线测量)
    ↓
Phase 1A (L2 正则, MVP)
    ↓
Phase 1B (L1 initial_prompt)
    ↓
Phase 2 (L3 LLM)
    ↓
Phase 3 (合规与体验)
```

**Phase 2 的实际启动条件**：基线测量确认 L1+L2 后仍有 > 10% 术语错误无法覆盖。

---

## 14. 测试策略

### 14.1 单元测试

| 测试目标 | 测试内容 | 优先级 |
|----------|----------|:---:|
| `build_term_pattern()` — 三语词边界 | 日语 `\p{Katakana}`、中文 `\p{Han}`、英语 `\b` 正确 | P0 |
| 正则排序 | `original` 字符数降序，长模式先于短模式执行 | P0 |
| `apply_terminology_correction()` | 三语混合文本替换正确 | P0 |
| `Cow<str>` 优化 | 无匹配时不分配新字符串 | P1 |
| 日语促音/长音符变体 | `けっこう→けこう` 等 6 种变体被正确替换 | P1 |
| 日语浊音/半浊音混淆 | `ポリ→ボリ/ホリ` 变体正确替换 | P1 |
| CAS/UN/GHS 编码格式恢复 | `H two twenty five→H225` 等 | P1 |
| CSV 编码自动检测 | UTF-8 / UTF-8 BOM / Shift-JIS 正确识别 | P1 |
| 幂等迁移 | 重复执行 migration 不会因列已存在而报错 | P1 |

### 14.2 集成测试

| 测试场景 | 方法 |
|----------|------|
| L2 正则三语替换 | Mock 日/中/英混合文本 → 验证输出正确 |
| L1 initial_prompt 生效 | 设置日语术语表 → 验证 whisper_engine 的 FullParams 包含 prompt |
| L3 LLM 校正触发 | 模拟录音停止 → 验证异步 task 被 spawn |
| 缓存刷新联动 | UI 增删术语 → L1+L2 缓存同步刷新 |
| 多语言 prompt 构建 | 日语会议 → 验证 prompt 中术语为日语形式 |
| LLM 不可用降级 | 模拟 Ollama 离线 → 验证静默跳过，不阻塞主流程 |

### 14.3 验收标准

- [ ] 用户可在设置页按语言（ja/zh/en/auto）管理术语条目
- [ ] high 优先级术语自动纳入 L1 initial_prompt（仅 Whisper 引擎）
- [ ] L2 正则替换在实时转录中生效，对所有引擎生效
- [ ] 日语长音符/促音/浊音变体被正确替换
- [ ] 中文同音字变体被正确替换
- [ ] CAS/UN/GHS 编码格式被正确恢复
- [ ] L3 LLM 校正异步执行，不阻塞 UI
- [ ] **L3 校正默认仅建议，需用户逐条确认后生效**
- [ ] 校正结果可逐条接受/拒绝
- [ ] CSV 导入时自动检测编码（UTF-8/Shift-JIS）
- [ ] 校正审计日志完整可追溯
- [ ] 低内存设备上自动降级（禁用 L3 或切换至 3B 模型）
- [ ] Phase 0 基线测量完成，术语准确率目标基于基线数据制定

---

## 15. 风险与应对

| 风险 | 影响 | 概率 | 应对措施 |
|------|------|:---:|----------|
| whisper-rs 0.13.x 未暴露 `set_initial_prompt` | L1 无法实现 | 中 | Phase 0 期间验证。若无此 API：评估升级 whisper-rs 版本；或评估通过 FFI 直接调用 whisper.cpp；或废弃 L1 仅依赖 L2+L3 |
| 日本語 `\p{Katakana}` 正则在旧版 regex crate 不工作 | L2 日语全词匹配失效 | 低 | Rust `regex` 1.0+ 已原生支持。项目当前使用 regex 1.11，已验证。CI 中锁定 regex >= 1.5 |
| initial_prompt 超过 224 token 限制 | 部分术语未注入 | 中 | 按 priority=high + 按语言过滤，严格控量。Prompt 预览面板显示实际 token 数 |
| 日/中/英混合时 LLM 语义错乱 | L3 校正产生错误建议 | 中 | L3 仅建议，用户逐条确认。Prompt 中三语并列表述任务 |
| Shift-JIS CSV 乱码 | 日企用户导入术语失败 | 中 | 自动检测编码 + 前端预览前 5 行 |
| 术语表膨胀至 500+ 条 | L2 正则性能退化 | 低 | Cow<str> 优化 + 最长模式优先。200 条以下实测无影响。超 500 条时评估 Aho-Corasick 用于纯字面量子集 |
| initial_prompt 对 Parakeet 无效 | Parakeet 用户无 L1 保护 | 中 | 文档 + UI 明确标注。Parakeet 用户至少有 L2+L3。Phase 0 测量 Parakeet 用户占比 |
| 危化品行业术语涉及合规风险 | 错误校正导致安全信息错误 | **高** | **L3 校正默认仅建议**，需人工确认后生效。审计日志完整。自动接受功能需用户明确开启并知情同意 |
| qwen2.5 模型在日企政策下不可用 | L3 无可用模型 | 低 | 支持多种 LLM backend（Ollama/Built-in/API），用户可替换。Gemma/Llama 作为备选 |
| 低内存设备无法运行 LLM | L3 不可用 | 中 | 自动降级策略（见 12.2 节）。8GB 以下仅 L1+L2 |
| 移动设备电池消耗过大 | 用户体验差 | 中 | 电池供电时默认禁用 L3（见 12.3 节） |

---

## 16. 合规与法务审查

> **本功能涉及转录文本的自动修改，转录用于内部审计与合规存档。
> 必须在 Phase 2 上线前完成法务审查。**

### 16.1 审查节点

| 节点 | 时机 | 审查内容 |
|------|------|----------|
| Gate A | Phase 1A 完成后 | 审查 L2 正则替换机制。由于 L2 是**确定性**替换且**可回放**，风险较低。确认是否需用户知情同意 |
| Gate B | Phase 2 上线前 | 审查 L3 LLM 校正机制。由于 LLM 输出非确定性，**必须**确认默认"仅建议"模式满足合规要求 |
| Gate C | Phase 3 完成后 | 审查审计报告格式是否符合行业监管要求 |

### 16.2 合规原则

1. **L2（确定性替换）**：可自动应用。每次替换在 `transcript_corrections` 表中记录，原始文本完整保留。
2. **L3（非确定性建议）**：默认**不**自动应用。用户逐条确认后生效。用户可选择开启"自动接受"但需二次确认知情同意。
3. **审计链**：所有校正操作（自动/手动）均记录操作者、时间戳、修改前后文本。
4. **原始文本保留**：`transcripts` 表的 `transcript` 字段存储 L1+L2 校正后的版本；`transcript_corrections` 表存储每次校正的详细记录；如需回溯原始模型输出，需在 Phase 1 添加 `raw_transcript` 字段。

### 16.3 推荐的合规设置（危化品行业默认）

| 设置项 | 推荐默认值 | 说明 |
|--------|:---:|------|
| `terminology_enabled` | 1 | 启用术语校正 |
| `initial_prompt_enabled` | 1 | 启用 L1（仅影响 Whisper 概率分布） |
| `llm_correction_enabled` | 1 | 启用 L3 校正建议生成 |
| `llm_correction_auto_accept` | **0** | **禁止自动接受 L3 建议** |
| L2 正则 | 始终启用 | 确定性的，可完全审计 |

---

## 17. 回滚与功能淘汰

### 17.1 运行时回滚

- **总开关**：`terminology_enabled = 0` → 禁用所有校正，转录管道恢复原始行为
- **分级回滚**：
  - L1：`initial_prompt_enabled = 0` → Whisper 引擎不再注入 prompt
  - L3：`llm_correction_enabled = 0` → 停止录音后不触发 LLM 校正
- **回滚不删除数据**：已存储的术语表和校正记录保留，仅停止应用新校正

### 17.2 数据回滚

- 术语表无自动回滚。CSV 导入前自动备份当前术语表，用户可手动恢复。
- `transcript_corrections` 表中被 rejected 的校正记录保留，标注 `status = 'rejected'`。
- 转录文本本身不做级联回滚——如果用户批量接受校正后又想撤销，需从 `transcript_corrections` 表手动重建。

### 17.3 性能紧急熔断

如果 L2 正则耗时超过 100ms（通过 `perf_debug!` 监控），自动降级：
- 仅应用 priority=high 的规则
- 在 UI 中显示警告

---

## 附录：与 V2.0 的主要变更

| 变更项 | V2.0 | V3.0 | 原因 |
|--------|------|------|------|
| 正则排序逻辑 | 按 `replacement` 长度 | 按 `original` 长度 | Bug 修复（审查问题 1） |
| 缓存刷新 | 两个独立 refresh 函数 | 单一 `refresh_all_caches()` | 一致性问题（审查问题 7） |
| L3 校正应用方式 | 自动保存校正后文本 | 默认仅建议，需用户确认 | 合规风险（审查问题 13） |
| 中间版本存储 | 单一 `corrected_text` 列 | JSON 快照 + corrections 表逐条记录 | 数据模型缺陷（审查问题 8） |
| SQLite 迁移 | 直接 ALTER TABLE | 幂等检查后 ALTER TABLE | Bug 修复（审查问题 6） |
| 新增 Phase 0 | 无 | 基线测量（1-2 人天） | 缺少基线（审查问题 11） |
| 新增 MVP 定义 | 无 | Phase 1A 为 MVP | 缺少裁剪讨论（审查问题 12） |
| 新增硬件要求 | 无 | 最低配置 + 降级策略 | 成本评估（审查问题 15） |
| 新增合规审查 | 无 | 三阶段审查节点 | 合规风险（审查问题 13） |
| 新增回滚策略 | 无 | 运行时开关 + 性能熔断 | 可运维性 |
| `Regex::unwrap` | 使用 unwrap | 使用 `if let Ok` | 防御性编程（审查问题 3） |
| 字符串分配 | 每次替换克隆 | `Cow<str>` 延迟分配 | 性能优化（审查问题 4） |
| Parakeet 标注 | 模糊提及 | 明确标注 L1 不生效，L2/L3 生效 | 能力边界清晰化 |
| 预置术语包规模 | ~370 条 | ~200 条精简版 | 减少维护负担，聚焦高概率错误 |

---

> **ドキュメントメンテナンス / 文档维护**：本 PRD V3.0 基于 V2.0 的技术审查重写，修正了发现的技术缺陷并补充了关键缺失内容。实现过程中如遇架构变更或行业适配需求变化，请同步更新本文档。
