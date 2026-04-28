# PRD：转录专业术语定制化功能

> **文档状态**：V2.0  
> **创建日期**：2026-04-27  
> **修订日期**：2026-04-27  
> **关联项目**：Meetily (meeting-minutes) v0.3.0  
> **目标行业**：危险化学品制造业（日系企业）  
> **支持语言**：日本語 / 中文 / English  
> **核心目标**：使 Meetily 转录引擎支持用户自定义专业术语词库，通过三级管道（模型内提示 → 正则后处理 → LLM 深度校正）智能识别并纠正语音识别（STT）产生的术语拼写/识别错误。

---

## 目录

1. [需求背景](#1-需求背景)
2. [核心问题分析](#2-核心问题分析)
3. [总体架构设计](#3-总体架构设计)
4. [第一级：Whisper initial_prompt 软引导](#4-第一级whisper-initial_prompt-软引导)
5. [第二级：正则实时校正通道](#5-第二级正则实时校正通道)
6. [第三级：LLM 深度校正通道](#6-第三级llm-深度校正通道)
7. [数据库设计](#7-数据库设计)
8. [后端实现规范](#8-后端实现规范)
9. [前端实现规范](#9-前端实现规范)
10. [配置与存储](#10-配置与存储)
11. [实施计划](#11-实施计划)
12. [测试策略](#12-测试策略)
13. [风险与应对](#13-风险与应对)

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

1. **跨语言术语混乱**：模型在日/中/英切换时，容易将一种语言的发音"听成"另一种语言的文字。例如日语「過酸化物（かさんかぶつ）」可能被识别为中文字符"卡三卡布茨"。
2. **化学物质名称识别率极低**：IUPAC 命名（如 2,4,6-Trinitrotoluene）和日文片假名化学名（如「ポリ塩化ビフェニル」）在通用 STT 训练语料中几乎不存在，模型必然拆分音节后错误重组。
3. **安全编码格式特殊**：CAS RN（如 108-88-3）、UN No.（如 UN 1203）、GHS 危险代码（H225, H301, H311）等编码格式在语音转写中极易出错。
4. **片假名/汉字/罗马字混合**：日语中化学术语同时使用汉字（爆発性）、片假名（ニトログリセリン）、罗马字缩略（PCB），增加了模型 token 预测难度。

### 1.3 用户故事

| 角色 | 需求 |
|------|------|
| 安全管理部门负责人 | 希望 `過酸化物`、`引火性液体`、`毒劇物` 等法定安全术语被 100% 正确转录，不可出现模糊或错误 |
| 工厂值班长（中文母语） | 希望中文术语 `甲苯二异氰酸酯`、`苯乙烯` 不会被识别为 `甲本二亿情酸纸`、`本以西` |
| 日本本社技术工程师 | 希望日语片假名术语 `ポリウレタン`、`エポキシ樹脂`、`メチルエチルケトン` 被正确转写 |
| 国际采购对接人 | 希望英语 CAS 编号 `CAS 101-68-8` 不会被转写成 `k Ass one o one sixty eight eight` |
| 合规审计员 | 希望所有转录文本可直接归档，术语准确度达到可审计标准（>99% 关键术语正确率） |

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

Whisper/Parakeet 等模型基于 subword token 級概率采样生成文本。专有名词/术语在训练数据中频率低，模型缺乏对其完整 token 序列的统计偏好。

#### 2.1.1 日语特有的错误模式

| 错误类型 | 示例 | 根因 |
|----------|------|------|
| 片假名 → 汉字误转 | `ポリウレタン` → `保利売れたん` | 模型对片假名化学术语的 token 覆盖不足，用常见汉字 token 替代 |
| 長音「ー」丢失 | `メチルエチルケトン` → `メチルエチルケトン` 的「ー」被省略 | whisper.cpp 对长音符 token 不敏感 |
| 促音「っ」丢失 | `引火性（いんかせい）` → `いんかせい` |   小さい「っ」的 token 在短音频段中易被丢弃 |
| 英语→片假名误回译 | 英文 `toluene` → 日文 `トルエン` 但模型输出 `トルーエン` | 过度音译 |

#### 2.1.2 中文特有的错误模式

| 错误类型 | 示例 | 根因 |
|----------|------|------|
| 同音字替换 | `甲苯二异氰酸酯` → `甲本二亿情酸纸` | 模型分不清化学术语的特殊汉字组合 |
| 数字+字母错位 | `H225` → `H二二五` / `h 2 2 5` | 中英数字混合 token 序列不稳定 |
| 多音字误读 | `重铬酸钾` → `重各酸钾` 或 `chóng gè suān jiǎ` | "重"和"铬"均是多音字 |
| 化学符号序列 | `NaOH` → `钠 O H` / `纳欧爱吃` | 英文缩写在中文语境中的 token 歧义 |

#### 2.1.3 英语特有的错误模式（化学语境）

| 错误类型 | 示例 | 根因 |
|----------|------|------|
| IUPAC 命名拆分 | `2,4,6-Trinitrotoluene` → `two four six tri nitro toluene` | 数字+前缀+后缀的 token 序列模型不熟悉 |
| CAS 编号格式 | `CAS 108-88-3` → `k Ass one o eight eighty eight three` | 连字符导致 token 边界错误 |
| GHS 代码 | `H301` → `H three hundred one` / `each three zero one` | H+数字组合不在模型的常见 token 表内 |
| MSDS 缩写 | `LD50` → `L D fifty` / `el dee fifty` | 无上下文时缩写被逐字母展开 |

#### 2.1.4 跨语言混合特有的错误模式

| 错误类型 | 示例 | 根因 |
|----------|------|------|
| 日语→中文误转 | `この物質は引火性があります` → `这个物质是银华星游戏吗` | 模型在语言切换点判断失误 |
| 中文→日语误转 | `闪点是负二十度` → `閃点は負にじゅうど`（日文假名化） | 中→日 tokenizer 路径泄漏 |
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
| 性能 (单次) | <10ms (参数注入) | <1ms | 2-10s |
| 成本 | 免费 | 免费 | 取决于模型 |

### 2.4 设计决策：三级校正管道

```
三级管道总览：

第一级: initial_prompt  →   推理时注入，软偏置 token 分布。作用于每个 chunk 的模型推理阶段。
                           例: prompt 中含「過酸化物」→ 模型在该 token 候选上 logit 值提高。

第二级: 正则后处理      →   模型输出后即时执行，确定性替换。作用于 worker.rs 的统一出口。
                           例: "かさんかぶつ" → "過酸化物" (已知变体精确替换)

第三级: LLM 深度校正    →   录音停止后异步执行，语义级修复。作用于整段转录文本。
                           例: "那个容器的温度超过了引火点" → 结合上下文确定"容器"应为"反応器"
```

| 维度 | L1：initial_prompt | L2：正则实时 | L3：LLM 异步 |
|------|:---|:---|:---|
| **执行时机** | 每次模型推理时 | 每次转录输出后 | 录音停止后 |
| **延迟** | <10ms (参数注入) | <1ms | 2-10s |
| **覆盖率** | 概率偏置 ≈ 30% | 已知变体 ≈ 60% | 未知变体+上下文 ≈ 10% |
| **确定性与审计** | 低（概率性） | 高（确定性、可回放） | 中（需人工确认） |
| **成本** | 免费 | 免费 | Ollama 免费/API 付费 |
| **是否阻塞实时显示** | 否 | 否 | 否（异步） |

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
│  │ - 增删改查术语条目        │    │ - 深度校正版本 (L3 校正后)                │ │
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
│  │ 术语 CRUD 命令           │    │ 缓存刷新命令                              │ │
│  │ get/save/delete          │    │ refresh_terminology_cache                 │ │
│  │ import/export            │    │ refresh_initial_prompt_cache              │ │
│  └───────────┬─────────────┘    └──────────────┬───────────────────────────┘ │
│              │                                  │                             │
│              ▼                                  ▼                             │
│  ┌──────────────────────────┐    ┌──────────────────────────────────────────┐ │
│  │ SQLite: terminology 表    │    │ 内存缓存 (2 个独立缓存)                    │ │
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
│  │                          │    │  │ 预编译正则状态机 (按 lang 分段)     │    │ │
│  │                          │    │  └──────────────────────────────────┘    │ │
│  └──────────────────────────┘    └──────────────┬───────────────────────────┘ │
│                                                 │                             │
│  ┌──────────────────────────────────────────────┴───────────────────────────┐ │
│  │              转录管道 (audio/transcription + whisper_engine)              │ │
│  │                                                                          │ │
│  │  每个音频 chunk:                                                          │ │
│  │       │                                                                  │ │
│  │       ├──► 【L1】initial_prompt 注入                                      │ │
│  │       │    WhisperEngine::transcribe_audio_with_confidence()              │ │
│  │       │    在 FullParams 中调用 params.set_initial_prompt(&cache)          │ │
│  │       │    效果: token 概率分布偏置，引导模型输出正确术语                      │ │
│  │       │                                                                  │ │
│  │       ├──► Whisper/Parakeet 模型推理                                       │ │
│  │       │                                                                  │ │
│  │       ├──► 【L2】apply_terminology_correction(raw_text)                    │ │
│  │       │    在 worker.rs 统一出口执行正则批量替换                              │ │
│  │       │    效果: 确定性纠正已知变体（片假名拼写、数字格式等）                    │ │
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
│  │  │      跨语言消歧 + 化学编码验证      │                                  │ │
│  │  └────────────────┬───────────────────┘                                  │ │
│  │                   │                                                      │ │
│  │                   ▼                                                      │ │
│  │            更新 DB 中的校正版转录 + 校正审计记录                               │ │
│  └──────────────────────────────────────────────────────────────────────────┘ │
└──────────────────────────────────────────────────────────────────────────────┘
```

### 3.2 数据流时序

```
时间轴 ────────────────────────────────────────────────────────────────────►

用户点击开始录音
  │
  ├─► Rust: 加载 initial_prompt 缓存，构建 Whisper FullParams
  │        params.set_initial_prompt("過酸化物, 引火性液体, 毒劇物, TDI, MDI, ...")
  │
  ├─► worker.rs: 转录循环
  │     │
  │     ├─► WhisperEngine::transcribe_audio_with_confidence()
  │     │     ├─► 【L1】FullParams 已含 initial_prompt → 推理时 token 偏置
  │     │     └─► 返回 raw_text (已受 prompt 影响)
  │     │
  │     ├─► 【L2】apply_terminology_correction(raw_text)
  │     │     └─► 正则确定性替换（如 "かさんかぶつ"→"過酸化物"）
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
  └─► 异步触发 【L3】LLM 校正 ←───── tokio::spawn, 不阻塞 UI
        │
        ├─► llm_correct_terminology(
        │       full_transcript,
        │       terminology_table(ja/zh/en),
        │       industry_context = "危化品制造业"
        │    )
        │     ├─► 加载术语表（按语言分组加载）
        │     ├─► 构造多语言 Prompt (system + user)
        │     ├─► 调用 LLM (Ollama qwen2.5:7b / local sidecar)
        │     └─► 解析校正后文本
        │
        ├─► 保存 L3 校正版到 DB (标记: correction_level = "l3")
        ├─► 记录 transcript_corrections 表（审计用）
        │
        └─► 前端 polling 检测到完成 → 展示差异对比
```

---

## 4. 第一级：Whisper initial_prompt 软引导

### 4.1 实现原理

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
- Parakeet **不支持**此参数，本级别仅对 Whisper 引擎生效

### 4.2 Prompt 构建策略

#### 4.2.1 按当前语言动态选择

从用户设置的录音语言偏好（已经是 `get_language_preference_internal()` 获取的）决定注入哪些术语：

```rust
fn build_initial_prompt(language: Option<&str>, term_cache: &str) -> String {
    // base_prompt: 告诉模型会议类型和行业语境
    let base = "This is a chemical safety meeting discussing hazardous materials, MSDS, GHS classification.";

    // term_prompt: 从术语缓存中提取高优先级术语
    // 按语言过滤 + 取前 N 个（token 预算控制在 200 以内）
    let terms = term_cache.to_string();

    format!("{} {}", base, terms)
}
```

#### 4.2.2 日/中/英多语言 prompt 示例

**日语会议时注入的 prompt**（日语术语用日文片假名+汉字原样）：
```
危険化学品製造会議。以下の用語が含まれる可能性がある：
過酸化物, 引火性液体, 毒劇物, ポリウレタン, エポキシ樹脂, トルエンジイソシアネート,
メチルエチルケトン, 爆発性, 急性毒性, 特定化学物質, 有機溶剤, 作業環境測定,
GHS分類, SDS, CAS番号, PRTR法, 安衛法, 消防法
```

**中文会议时注入的 prompt**：
```
危险化学品制造会议。以下术语可能出现：
甲苯二异氰酸酯, 二苯基甲烷二异氰酸酯, 苯乙烯, 环氧树脂, 聚氨酯, 过氧化物,
易燃液体, 急性毒性, 特定化学物质, 有机溶剂, 作业环境测定, 安全数据表,
GHS分类, CAS编号, 危险货物编号, 重大危险源, 应急预案
```

**英语会议时注入的 prompt**：
```
Hazardous chemical manufacturing meeting. Terms:
toluene diisocyanate, methylene diphenyl diisocyanate, styrene monomer, epoxy resin,
polyurethane, peroxide, flammable liquid, acute toxicity, LD50, LC50,
GHS hazard statements H225 H301 H311, CAS registry number, UN number,
Safety Data Sheet, threshold limit value, permissible exposure limit
```

### 4.3 代码集成位置

**文件位置**：`frontend/src-tauri/src/whisper_engine/whisper_engine.rs`

在 `transcribe_audio_with_confidence()` 函数（约第 516 行）中，`FullParams` 构造完成后：

```rust
// 现有代码构造 FullParams 之后，state.full() 之前插入:

// ===== 新增: L1 initial_prompt 注入 =====
let language = language.clone(); // 复用已有参数
let initial_prompt = terminology::cache::get_initial_prompt(language.as_deref());
if !initial_prompt.is_empty() {
    params.set_initial_prompt(&initial_prompt);
    log::debug!(
        "L1 initial_prompt injected ({} chars, lang: {:?})",
        initial_prompt.len(),
        language
    );
}
// =======================================

// 继续原有流程:
let mut state = ctx.create_state()?;
state.full(params, &audio_data)?;
```

### 4.4 Prompt 缓存

```rust
// terminology/cache.rs

use std::sync::LazyLock;
use std::sync::RwLock;

/// 全局 initial_prompt 缓存 — 启动时从术语表生成，术语变更时刷新
/// 内容为逗号分隔的高优先级术语列表，用于 Whisper initial_prompt 参数
static INITIAL_PROMPT_CACHE: LazyLock<RwLock<String>> =
    LazyLock::new(|| RwLock::new(String::new()));

/// 按语言分别缓存（因为不同语言会议术语不同）
static INITIAL_PROMPT_BY_LANG: LazyLock<RwLock<HashMap<String, String>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

/// 从术语表重建 initial_prompt
pub async fn refresh_initial_prompt_cache(
    db: &DatabaseManager
) -> Result<(), String> {
    let entries = db.get_all_terminology().await
        .map_err(|e| format!("Failed to load terminology: {}", e))?;

    // 按语言分组构建 prompt
    let mut by_lang: HashMap<String, Vec<String>> = HashMap::new();
    for entry in &entries {
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

    *INITIAL_PROMPT_BY_LANG.write().map_err(|e| e.to_string())? = lang_map;
    log::info!("Initial prompt cache refreshed for {} languages", lang_map.len());
    Ok(())
}

/// 获取指定语言的 initial_prompt
pub fn get_initial_prompt(language: Option<&str>) -> String {
    let lang_key = match language {
        Some("ja") | Some("jp") => "ja",
        Some("zh") | Some("cn") => "zh",
        Some("en") => "en",
        _ => "auto", // 自动检测时不注入语言特定术语，用通用术语
    };

    let cache = INITIAL_PROMPT_BY_LANG.read().ok();
    cache.and_then(|c| c.get(lang_key).cloned())
        .unwrap_or_default()
}
```

---

## 5. 第二级：正则实时校正通道

### 5.1 实现原理

**装饰器模式 + 预编译正则缓存**。

所有转录结果（无论 Whisper 还是 Parakeet）在 [worker.rs](file:///e:/ForkedRepo/meetily-forked/frontend/src-tauri/src/audio/transcription/worker.rs) 的 `transcribe_chunk_with_provider` 函数中汇聚。在引擎返回原始文本后、发送 `transcript-update` 事件前，插入一个 `apply_terminology_correction()` 后处理步骤。

**L1 → L2 的协同**：`initial_prompt` 让模型更大概率输出正确术语或其近形变体，正则则将这些残余偏差**确定性地修正**。二者不是替代关系，而是"概率引导 + 确定纠偏"的互补。

### 5.2 多语言正则的特殊处理

日语、中文的正则匹配与英语有本质差异：

| 语言 | 词边界 `\b` | 注意事项 |
|------|:---:|------|
| 英语 | ✅ 有效 | `\b` 基于 `\w` 与 `\W` 的边界，字母/数字为 `\w` |
| 日语 | ❌ 不可用 | 片假名、平假名、汉字在 regex 中均为 `\w`，`\b` 无法正确判定词边界。需关闭 `whole_word` 或使用**前瞻/后顾断言** |
| 中文 | ❌ 不可用 | 同上。汉字之间无空格分隔，`\b` 无效。中文"词边界"需要用分词器或使用 unicode 分段属性 |

**日语的全词匹配替代方案**：

```rust
// 日语"全词匹配" — 使用 unicode 属性断言
// 日文字符在 Unicode 中属于不同 block:
//   汉字 (CJK): \p{Han}
//   平假名: \p{Hiragana}
//   片假名: \p{Katakana}
//   片假名语音扩展: \p{Katakana_Phonetic_Extensions}

fn build_japanese_word_boundary(pattern: &str) -> String {
    // 前向断言: 前面不是日文字符 (即前面是行首/空格/标点/非日语)
    // 后向断言: 后面不是日文字符
    format!(
        r"(?<![\p{Han}\p{Hiragana}\p{Katakana}ー]){}(?![\p{Han}\p{Hiragana}\p{Katakana}ー])",
        pattern
    )
}
```

**中文的全词匹配替代方案**：

```rust
// 中文"全词匹配" — 前后不允许是 CJK 字符
fn build_chinese_word_boundary(pattern: &str) -> String {
    format!(
        r"(?<![\p{Han}])({})(?![\p{Han}])",
        pattern
    )
}
```

### 5.3 数据结构设计

```rust
/// 编译后的术语校正规则
struct TerminologyRule {
    regex: Regex,          // 预编译正则
    replacement: String,   // 替换目标
    language: String,      // 所属语言 (ja/zh/en/auto)
}
```

### 5.4 核心代码规范

**文件位置**：`frontend/src-tauri/src/audio/transcription/worker.rs`（在该文件末尾新增）

```rust
use regex::Regex;
use std::sync::{LazyLock, RwLock};

// ============================================================================
// 术语校正模块 (L2: Regex Post-Processing)
// ============================================================================

/// 全局术语校正缓存 — 启动时从 DB 加载并编译为正则状态机
/// RwLock 保证转录热路径（读）零争用，仅在用户修改术语表时短暂写锁定
static TERMINOLOGY_CACHE: LazyLock<RwLock<Vec<TerminologyRule>>> =
    LazyLock::new(|| RwLock::new(Vec::new()));

/// 编译后的术语校正规则
struct TerminologyRule {
    regex: Regex,
    replacement: String,
}

/// 根据语言选择合适的词边界实现
fn build_term_pattern(entry: &TerminologyEntry) -> String {
    let escaped = regex::escape(&entry.original);

    if !entry.whole_word {
        // 子串匹配：直接使用 pattern
        let case_flag = if entry.case_sensitive { "" } else { "(?i)" };
        return format!("{}{}", case_flag, escaped);
    }

    // 全词匹配：根据语言选择不同的词边界实现
    match entry.language.as_str() {
        "ja" => {
            // 日语: 前后不允许日文字符
            format!(
                r"(?<![\p{{Han}}\p{{Hiragana}}\p{{Katakana}}ー]){}(?![\p{{Han}}\p{{Hiragana}}\p{{Katakana}}ー])",
                escaped
            )
        }
        "zh" => {
            // 中文: 前后不允许 CJK 字符
            format!(
                r"(?<![\p{{Han}}])({})(?![\p{{Han}}])",
                escaped
            )
        }
        _ => {
            // 英语等: 使用标准 \b
            let case_flag = if entry.case_sensitive { "" } else { "(?i)" };
            format!(r"{}\b{}\b", case_flag, escaped)
        }
    }
}

/// 刷新术语缓存 — 从数据库重新加载术语表并重新编译正则
pub async fn refresh_terminology_cache(
    db: &crate::database::manager::DatabaseManager
) -> Result<(), String> {
    let entries = db.get_all_terminology().await
        .map_err(|e| format!("Failed to load terminology: {}", e))?;

    let mut rules = Vec::with_capacity(entries.len());

    // 按 replacement 长度降序排列，确保长术语优先匹配
    // 关键: 避免 "酸" 在 "過酸化物" 之前匹配导致的错误替换
    let mut entries: Vec<_> = entries.into_iter().collect();
    entries.sort_by(|a, b| {
        b.replacement.chars().count()
            .cmp(&a.replacement.chars().count())
    });

    for entry in entries {
        if !entry.enabled {
            continue;
        }

        let pattern = build_term_pattern(&entry);

        match Regex::new(&pattern) {
            Ok(re) => rules.push(TerminologyRule {
                regex: re,
                replacement: entry.replacement.clone(),
            }),
            Err(e) => {
                log::warn!(
                    "Failed to compile terminology regex for '{}': {}",
                    entry.original, e
                );
            }
        }
    }

    log::info!("Terminology cache refreshed with {} rules", rules.len());
    *TERMINOLOGY_CACHE.write().map_err(|e| e.to_string())? = rules;
    Ok(())
}

/// 对转录文本应用术语校正
/// 遍历所有预编译正则，逐条替换
fn apply_terminology_correction(text: &str) -> String {
    let cache = match TERMINOLOGY_CACHE.read() {
        Ok(guard) => guard,
        Err(_) => return text.to_string(),
    };

    if cache.is_empty() {
        return text.to_string();
    }

    let mut result = text.to_string();
    for rule in cache.iter() {
        result = rule.regex.replace_all(&result, rule.replacement.as_str()).to_string();
    }
    result
}
```

**调用位置**：在 `transcribe_chunk_with_provider` 函数中，Whisper 分支（约第 456 行）和 Parakeet 分支（约第 491 行）的 `cleaned_text` 赋值后：

```rust
// 原有代码:
let cleaned_text = text.trim().to_string();

// 新增 L2 正则校正:
let cleaned_text = apply_terminology_correction(&cleaned_text);
```

---

## 6. 第三级：LLM 深度校正通道

### 6.1 触发时机

录音停止 → 转录保存到 DB（L1+L2 校正版）→ 异步触发 L3 LLM 校正 → 更新 DB → 前端轮询展示

**不阻塞**录音停止后的 UI 导航和数据保存流程。

### 6.2 LLM Provider 选择（多语言场景）

由于需要同时理解日/中/英三种语言，模型选择比单一语言更苛刻：

| Provider | 多语言能力 | 延迟 | 成本 | 推荐度 |
|----------|:---:|------|------|:---:|
| Ollama `qwen2.5:14b` | 日/中/英均优 | 3-8s | 免费 | ⭐⭐⭐ |
| Ollama `qwen2.5:7b` | 中/英优，日可接受 | 2-5s | 免费 | ⭐⭐⭐ |
| Ollama `llama3.1:8b` | 英优，中/日弱 | 3-8s | 免费 | ⭐⭐ |
| Ollama `gemma3:4b` | 英优，中可接受，日弱 | 1-3s | 免费 | ⭐⭐ |
| Claude API (Sonnet) | 日/中/英均优 | 2-4s | ~$0.02/次 | ⭐⭐ |
| 内置 sidecar (`llama-helper`) | 取决于模型文件 | 取决于硬件 | 免费 | ⭐⭐⭐ |
| GPT-4o | 日/中/英最优 | 2-5s | ~$0.03/次 | ⭐⭐ |

**推荐**：日系企业场景首选 `qwen2.5:14b`（通义千问）——在日/中/英三语平衡性上表现最好，且支持 Ollama 本地部署零成本。

**不需要 Thinking/Reasoning 模型**：术语校正的任务本质是"跨语言匹配+替换"，不是多步推理链。普通 LLM 的语义理解能力已足够。Thinking 模型增加的延迟和 token 消耗在实时场景中不划算。

### 6.3 Prompt 设计（多语言化学行业版）

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
1. 片仮名化学物質名の分割誤り（例：「ポリウレタン」→「ポリ ウレ たん」→「ポリウレタン」）
   片假名化学物质名分割错误
2. 中国語化学名の同音異字誤り（例：「甲苯二异氰酸酯」→「甲本二亿情酸纸」）
   中文化学名称同音字错误
3. CAS番号・UN番号の誤認識（例：「CAS 108-88-3」→「k Ass one o eight」→「CAS 108-88-3」）
   CAS/UN 编号识别错误
4. GHSコードの形式誤り（例：「H225」→「H two twenty five」→「H225」）
   GHS 代码格式错误
5. 言語切替時の混同（例：日本語なのに中国語で出力される）
   语言切换时的混淆

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
校正後の完全なテキストのみを出力。説明・コメント・マークダウン不要。
只输出校正后的完整文本，不要任何解释、注释、格式标记。

### 元の文字起こし / 原始转录 / Original Transcript
%TRANSCRIPT_TEXT%
```

### 6.4 化学编码验证

LLM 除了语义校正，还需执行**规则级编码验证**（确定性检查，不需要 LLM 推理）：

```rust
/// 化学编码格式验证与自动修复 (在 LLM 校正后执行)
fn validate_chemical_codes(text: &str) -> String {
    let mut result = text.to_string();

    // CAS RN 格式: 数字-数字-数字 (如 108-88-3)
    let cas_pattern = Regex::new(
        r"(?i)cas\s*#?\s*(\d{2,7})\s*[-—–]\s*(\d{2})\s*[-—–]\s*(\d)"
    ).unwrap();
    // 暂不替换，仅验证格式存在：若 LLM 输出中 CAS 格式异常则日志告警

    // GHS 危险代码: H + 3位数字
    let ghs_pattern = Regex::new(
        r"\bH\s*(\d)\s*(\d)\s*(\d)\b"
    ).unwrap();
    result = ghs_pattern.replace_all(&result, "H$1$2$3").to_string();

    // UN 编号: UN + 4位数字
    let un_pattern = Regex::new(
        r"(?i)UN\s*(\d)\s*(\d)\s*(\d)\s*(\d)\b"
    ).unwrap();
    result = un_pattern.replace_all(&result, "UN $1$2$3$4").to_string();

    result
}
```

### 6.5 实现代码规范

**调用链路**：

```
recording-stopped
  → RecordingPostProcessingProvider (前端)
    → invoke("api_correct_transcript_terminology", { meeting_id })
      → Rust: terminology_correction_task()
        ├─► llm_client::correct_terminology(transcript, terminology_table)
        │     ├─► 按 meeting_language 选择多语言 prompt
        │     ├─► Ollama / BuiltIn / API LLM
        │     └─► llm_corrected_text
        ├─► validate_chemical_codes(llm_corrected_text)  ← 规则级兜底
        └─► db.update_transcript_corrected(meeting_id, final_corrected_text)
```

### 6.6 差异展示

前端在第三级校正完成后，提供原文与校正版的**三级差异对比视图**：

- L1+L2 校正版 vs L3 校正版逐词 diff 高亮
- 按校正类型标注（语言切换修正 / 化学名修正 / 编码格式修正 / 大小写修正）
- 用户可逐条接受/拒绝校正
- 接受全部 / 拒绝全部的快捷操作
- 校正结果持久化到 DB 并记录审计日志

---

## 7. 数据库设计

### 7.1 新建表：`terminology`

**迁移文件**：`migrations/20260427000000_add_terminology.sql`

```sql
CREATE TABLE IF NOT EXISTS terminology (
    id               TEXT PRIMARY KEY,
    original         TEXT NOT NULL,            -- 原始文本 / 匹配パターン / match pattern
    replacement      TEXT NOT NULL,            -- 期望替换为的术语 / 置換後の用語
    language         TEXT NOT NULL DEFAULT 'auto', -- 'ja' | 'zh' | 'en' | 'auto'
    case_sensitive   INTEGER NOT NULL DEFAULT 0,
    whole_word       INTEGER NOT NULL DEFAULT 1,
    enabled          INTEGER NOT NULL DEFAULT 1,
    priority         TEXT NOT NULL DEFAULT 'normal', -- 'high' | 'normal' | 'low'
                                                     -- 'high' 会被纳入 L1 initial_prompt
    category         TEXT NOT NULL DEFAULT 'general',
    description      TEXT,                     -- 备注 / 備考
    created_at       TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at       TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_terminology_language ON terminology(language);
CREATE INDEX IF NOT EXISTS idx_terminology_category ON terminology(category);
CREATE INDEX IF NOT EXISTS idx_terminology_priority ON terminology(priority);
CREATE INDEX IF NOT EXISTS idx_terminology_enabled ON terminology(enabled);
```

### 7.2 新建表：`transcript_corrections`

存储各级校正历史，支持审计和合规追溯：

```sql
CREATE TABLE IF NOT EXISTS transcript_corrections (
    id               TEXT PRIMARY KEY,
    meeting_id       TEXT NOT NULL,
    original_text    TEXT NOT NULL,            -- 校正前文本
    corrected_text   TEXT NOT NULL,            -- 校正后文本
    correction_level TEXT NOT NULL DEFAULT 'l2', -- 'l1_l2' | 'l3' | 'manual'
    correction_type  TEXT NOT NULL DEFAULT 'auto', -- 'regex' | 'llm' | 'manual'
    status           TEXT NOT NULL DEFAULT 'pending', -- 'pending' | 'accepted' | 'rejected'
    language         TEXT,                     -- 校正涉及的主要语言
    reviewed_by      TEXT,                     -- 审核人（手动校正时）
    created_at       TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (meeting_id) REFERENCES meetings(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_corrections_meeting ON transcript_corrections(meeting_id);
CREATE INDEX IF NOT EXISTS idx_corrections_level ON transcript_corrections(correction_level);
```

### 7.3 现有表扩展

```sql
-- transcripts 表新增字段
ALTER TABLE transcripts ADD COLUMN corrected_text TEXT;
-- 用于存储 L3 校正后的最终文本（校正被接受后回写）

-- settings 表新增字段
ALTER TABLE settings ADD COLUMN terminology_enabled INTEGER DEFAULT 1;
ALTER TABLE settings ADD COLUMN llm_correction_enabled INTEGER DEFAULT 1;
ALTER TABLE settings ADD COLUMN initial_prompt_enabled INTEGER DEFAULT 1;
```

### 7.4 数据模型（Rust）

**文件位置**：`frontend/src-tauri/src/database/models.rs`

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

/// 转录校正记录 (审计用)
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

### 7.5 仓库层（Repository）

**文件位置**：`frontend/src-tauri/src/database/repositories/terminology.rs`（新建）

```rust
impl DatabaseManager {
    /// 获取所有启用的术语
    pub async fn get_all_terminology(&self) -> Result<Vec<TerminologyEntry>>;

    /// 按语言获取术语
    pub async fn get_terminology_by_language(&self, language: &str) -> Result<Vec<TerminologyEntry>>;

    /// 获取高优先级术语 (用于 L1 initial_prompt)
    pub async fn get_high_priority_terminology(&self) -> Result<Vec<TerminologyEntry>>;

    /// 批量保存术语（upsert）
    pub async fn save_terminology(&self, entries: Vec<TerminologyEntry>) -> Result<()>;

    /// 删除单个术语
    pub async fn delete_terminology(&self, id: &str) -> Result<()>;

    /// 创建校正记录
    pub async fn create_correction(&self, correction: TranscriptCorrection) -> Result<()>;

    /// 更新校正状态
    pub async fn update_correction_status(&self, id: &str, status: &str) -> Result<()>;

    /// 获取会议的所有校正记录
    pub async fn get_corrections_for_meeting(&self, meeting_id: &str) -> Result<Vec<TranscriptCorrection>>;

    /// 更新转录的 corrected_text 字段
    pub async fn update_transcript_corrected(&self, meeting_id: &str, corrected_text: &str) -> Result<()>;
}
```

---

## 8. 后端实现规范

### 8.1 模块组织结构

```
frontend/src-tauri/src/
├── terminology/                     # 新建模块
│   ├── mod.rs                       # 模块入口
│   ├── cache.rs                     # 双缓存管理
│   │   ├── INITIAL_PROMPT_CACHE     #   L1: initial_prompt 缓存
│   │   ├── TERMINOLOGY_CACHE        #   L2: 预编译正则缓存
│   │   ├── refresh_initial_prompt_cache()
│   │   ├── refresh_terminology_cache()
│   │   └── get_initial_prompt()
│   ├── commands.rs                  # Tauri 命令 (CRUD + 缓存刷新)
│   └── corrector.rs                 # 校正器
│       ├── apply_terminology_correction()    # L2 正则替换
│       ├── validate_chemical_codes()         # 化学编码验证
│       └── llm_correct_terminology()         # L3 LLM 校正
│
├── whisper_engine/
│   └── whisper_engine.rs           # L1 集成 (在 transcribe_audio_with_confidence 中
│                                   #     调用 params.set_initial_prompt)
│
├── audio/transcription/worker.rs   # L2 集成 (在 transcribe_chunk_with_provider 中
│                                   #     调用 apply_terminology_correction)
│
├── summary/llm_client.rs           # L3: 新增 correct_terminology() 函数
│
├── database/
│   ├── models.rs                   # 新增 TerminologyEntry, TranscriptCorrection
│   └── repositories/
│       └── terminology.rs          # 新建仓库文件
│
├── lib.rs                          # 注册新命令
│
└── migrations/
    └── 20260427000000_add_terminology.sql
```

### 8.2 Tauri 命令注册

在 [lib.rs](file:///e:/ForkedRepo/meetily-forked/frontend/src-tauri/src/lib.rs) 的 `invoke_handler` 中新增：

```rust
// ===== 术语管理命令 =====
terminology::commands::get_terminology_list,
terminology::commands::save_terminology_entry,
terminology::commands::delete_terminology_entry,
terminology::commands::import_terminology_csv,
terminology::commands::export_terminology_csv,

// L1+L2 缓存刷新
terminology::commands::refresh_initial_prompt_cache_cmd,
terminology::commands::refresh_terminology_cache_cmd,

// L3 LLM 术语校正
terminology::commands::run_llm_terminology_correction,
terminology::commands::get_correction_status,
terminology::commands::accept_correction,
terminology::commands::reject_correction,
terminology::commands::get_corrections_for_meeting,

// 设置项
terminology::commands::get_terminology_settings,
terminology::commands::set_terminology_settings,
```

### 8.3 启动时初始化

在 [lib.rs](file:///e:/ForkedRepo/meetily-forked/frontend/src-tauri/src/lib.rs) 的 `setup` 闭包中，数据库初始化之后：

```rust
// 初始化术语校正系统（L1 initial_prompt 缓存 + L2 正则缓存）
let db = app.state::<state::AppState>().db_manager.clone();
tauri::async_runtime::spawn(async move {
    // L1: 构建 initial_prompt 缓存（按语言）
    if let Err(e) = terminology::cache::refresh_initial_prompt_cache(&db).await {
        log::warn!("Failed to initialize initial_prompt cache: {}", e);
    } else {
        log::info!("Initial prompt cache initialized from database");
    }

    // L2: 编译正则缓存
    if let Err(e) = terminology::cache::refresh_terminology_cache(&db).await {
        log::warn!("Failed to initialize terminology cache: {}", e);
    } else {
        log::info!("Terminology regex cache initialized from database");
    }
});
```

---

## 9. 前端实现规范

### 9.1 新增组件

| 组件 | 文件路径 | 说明 |
|------|----------|------|
| `TerminologyManager` | `components/TerminologyManager/index.tsx` | 术语管理主面板：表格 + 操作栏 + 语言筛选 |
| `TerminologyEntryRow` | `components/TerminologyManager/EntryRow.tsx` | 单条术语编辑行（含语言选择器） |
| `TerminologyImportDialog` | `components/TerminologyManager/ImportDialog.tsx` | CSV 导入对话框（含语言列） |
| `TerminologyPresetSelector` | `components/TerminologyManager/PresetSelector.tsx` | 预置术语包选择导入 |
| `CorrectionDiffView` | `components/CorrectionDiff/DiffView.tsx` | 三级校正差异对比视图 |

### 9.2 术语管理 UI 设计（多语言版）

```
┌──────────────────────────────────────────────────────────────────┐
│  専門用語管理 / 术语管理                            [导入] [导出] │
│                                                                  │
│  语言筛选: [全部 ▼] [日本語] [中文] [English]   优先级: [全部 ▼] │
│                                                                  │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │ 🔍 搜索术语...                                  [+ 新增]  │   │
│  └──────────────────────────────────────────────────────────┘   │
│                                                                  │
│  ┌────────┬──────────────┬──────────┬──────┬────┬──────┬────┐  │
│  │ 语言   │ 原文          │ 替换为    │ 优先级│全词│ 启用 │操作│  │
│  ├────────┼──────────────┼──────────┼──────┼────┼──────┼────┤  │
│  │ 🇯🇵 ja │かさんかぶつ   │過酸化物   │ high │ [✓]│ [✓]  │✕ ✎ │  │
│  │ 🇯🇵 ja │ポリ ウレ たん │ポリウレタン│ high │ [✓]│ [✓]  │✕ ✎ │  │
│  │ 🇨🇳 zh │甲本二亿情酸纸 │甲苯二异氰酸酯│high│ [✓]│ [✓]  │✕ ✎ │  │
│  │ 🇨🇳 zh │本以西         │苯乙烯     │ high │ [✓]│ [✓]  │✕ ✎ │  │
│  │ 🇬🇧 en │two four six…  │2,4,6-TNT │ high │ [ ]│ [✓]  │✕ ✎ │  │
│  │ 🇬🇧 en │H three hundred│H301      │ high │ [✓]│ [✓]  │✕ ✎ │  │
│  │ auto  │k Ass one o…   │CAS 108-88-3│normal│ [ ]│ [✓]  │✕ ✎ │  │
│  └────────┴──────────────┴──────────┴──────┴────┴──────┴────┘  │
│                                                                  │
│  L1 Prompt 预览: 日本語(12) 中文(15) English(10)                │
│  L2 Regex:   已编译 37 条规则                                    │
└──────────────────────────────────────────────────────────────────┘
```

### 9.3 会议详情页 — 三级差异对比视图

```
┌──────────────────────────────────────────────────────────────┐
│  文字起こし校正 / 转录校正                          [查看原文] │
│                                                              │
│  ┌─ 校正级别 ──────────────────────────────────────────┐    │
│  │ ☑ L1+L2 (实时)    ☐ L3 (深度)    ☐ 手动             │    │
│  └──────────────────────────────────────────────────────┘    │
│                                                              │
│  当該物質の引火点はマイナス 20 度であり、                          │
│          ^^^^^^  ← 過酸化物 (L2 regex)                          │
│  取り扱いには保護具が必要です。TDI と MDI の混合比は...              │
│  ^^^  ^^^  ← L1 prompt 影响                                      │
│                                                              │
│  ── L3 校正详情 ────────────────────────────────────        │
│  • 当該物質 → 当該過酸化物 [✓] [✕]   (化学物質名特定)        │
│  • 保護具 → 保護具（手袋・保護メガネ）[✓] [✕] (文脈補完)    │
│  • TDI → トルエンジイソシアネート (TDI) [✓] [✕] (略語展開)  │
│                                                              │
│  類型: 🟡化学名 🟢安全用語 🔵略語展開                         │
│  [一括承認]  [一括却下]                                       │
└──────────────────────────────────────────────────────────────┘
```

### 9.4 TypeScript 类型定义

**文件位置**：`frontend/src/types/terminology.ts`（新建）

```typescript
/** 支持的语言代码 */
export type TerminologyLanguage = 'ja' | 'zh' | 'en' | 'auto';

/** 术语优先级（high 会被纳入 L1 initial_prompt） */
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

export interface TerminologyImportResult {
  total: number;
  imported: number;
  skipped: number;
  errors: string[];
}

/** 术语校正设置 */
export interface TerminologySettings {
  terminologyEnabled: boolean;
  initialPromptEnabled: boolean;
  llmCorrectionEnabled: boolean;
  llmProvider: string;       // 'ollama' | 'builtin' | 'openai' | 'claude'
  llmModel: string;           // 'qwen2.5:14b' 等
}
```

---

## 10. 配置与存储

### 10.1 配置文件存储

术语表存储在 SQLite 数据库的 `terminology` 表中（参见第 7 节）。

各级开关存储在 `settings` 表的新增字段中：
- `terminology_enabled`：总开关（0 = 禁用所有校正）
- `initial_prompt_enabled`：L1 开关
- `llm_correction_enabled`：L3 开关
- L2 正则无法单独关闭（它是确定性的，总是无害的）

### 10.2 CSV 导入/导出格式（多语言版）

**导入格式**（UTF-8 BOM, 逗号分隔, 带表头）：

```csv
original,replacement,language,case_sensitive,whole_word,enabled,priority,category,description
かさんかぶつ,過酸化物,ja,false,true,true,high,化学物質,片仮名→漢字修正
ポリ ウレ たん,ポリウレタン,ja,false,true,true,high,化学物質,分割誤り修正
甲本二亿情酸纸,甲苯二异氰酸酯,zh,false,true,true,high,化学物質,同音字修正
本以西,苯乙烯,zh,false,true,true,high,化学物質,同音字修正
two four six tri nitro toluene,2,4,6-Trinitrotoluene,en,true,false,true,high,化学物質,IUPAC命名修正
H two twenty five,H225,en,false,true,true,high,GHSコード,危険有害性コード
k Ass one o eight eighty eight three,CAS 108-88-3,en,false,false,true,normal,CAS番号,CAS编号格式修正
```

### 10.3 内置预置术语包（危化品行业）

在 `frontend/src-tauri/templates/preset_terminology/` 下：

```
preset_terminology/
├── chemical_ja.csv            # 日本語 — 危険化学品製造用語（約100条）
├── chemical_zh.csv            # 中文 — 危险化学品制造术语（約100条）
├── chemical_en.csv            # English — Hazardous Chemical Manufacturing（約80条）
├── ghs_codes.csv              # GHS危険有害性情報コード（三語共通, 約50条）
├── cas_common.csv             # よく使われるCAS番号（三語共通, 約40条）
└── README.md                  # 利用方法と貢献ガイド
```

### 10.4 预置术语包内容（chemical_ja.csv 示例）

```csv
original,replacement,language,case_sensitive,whole_word,enabled,priority,category,description
かさんかぶつ,過酸化物,ja,false,true,true,high,危険性物質,発音→漢字
いんかせいえきたい,引火性液体,ja,false,true,true,high,危険性分類,消防法危険物
どくげきぶつ,毒劇物,ja,false,true,true,high,法規制,毒劇法対象
ポリ ウレ たん,ポリウレタン,ja,false,true,true,high,化学物質,PU原料
エポキシ じゅし,エポキシ樹脂,ja,false,true,true,high,化学物質,接着・塗料
トルエン ジ イソシアネート,TDI,ja,false,true,true,high,化学物質,TDI略称
メチル エチル ケトン,MEK,ja,false,true,true,high,化学物質,MEK略称
ばくはつせい,爆発性,ja,false,true,true,high,危険性分類,GHS区分
きゅうせいどくせい,急性毒性,ja,false,true,true,high,危険性分類,GHS区分
とくていかがくぶっしつ,特定化学物質,ja,false,true,true,high,法規制,特化則
ゆうきようざい,有機溶剤,ja,false,true,true,high,化学物質分類,有機則
さぎょうかんきょうそくてい,作業環境測定,ja,false,true,true,high,安全衛生,安衛法
ひあぶら,引火点,ja,false,true,true,high,物性,危険物判定
じこちゃっかおんど,自己発火温度,ja,false,true,true,high,物性,安全データ
じょうきあつ,蒸気圧,ja,false,true,true,high,物性,安全データ
ばくはつげんかい,爆発限界,ja,false,true,true,high,物性,LEL/UEL
ピーエル,PL,ja,false,true,true,high,法規制,製造物責任法
あんぜんデータシート,SDS,ja,false,true,true,high,安全情報,安全データシート
```

---

## 11. 实施计划

### 11.1 分阶段交付

```
Phase 1 ── L1 + L2 核心管道（约 5-6 人天）
├── 数据库 migration + models（支持 language/priority 字段）
├── 仓库层 CRUD（含按语言/优先级查询）
├── L1: initial_prompt 缓存模块 + whisper_engine 集成
├── L2: 正则缓存模块（含日/中/英三语词边界处理）+ worker.rs 集成
├── Tauri 命令注册（三级管道开关 + 缓存刷新）
├── 基础术语管理 UI（含语言筛选 + 优先级设置）
└── 危化品行业预置术语包 (chemical_ja/zh/en.csv)

Phase 2 ── L3 LLM 校正通道（约 3-4 人天）
├── llm_client.rs 新增 correct_terminology() + 多语言 prompt
├── 化学编码验证模块 (validate_chemical_codes)
├── 录音停止后异步触发 L3 校正流程
├── 校正结果存储到 DB（含 correction_level/audit）
├── 前端三级差异对比视图
└── 逐条接受/拒绝交互

Phase 3 ── 完善体验与合规（约 3-4 人天）
├── CSV 导入/导出（含语言列 + 编码检测 UTF-8/Shift-JIS）
├── 校正历史审计报告
├── 术语覆盖率统计分析面板
├── 日语特有 UI：注音（ふりがな）辅助编辑
├── 企业级：中央术语表下发 + 本地覆盖
└── 合规导出：校正日志 PDF/Excel
```

### 11.2 关键技术节点

| 节点 | 依赖 | 风险等级 |
|------|------|:---:|
| initial_prompt 缓存 | whisper.cpp `params.set_initial_prompt()` API | 低 |
| 日语 `\p{Katakana}` 正则 | Rust `regex` crate 1.x（支持 Unicode 属性） | 低 |
| 中文 `\p{Han}` 词边界 | 同上 | 低 |
| 多语言 LLM prompt | 需针对 qwen2.5 调优三语 prompt | 中 |
| CSV Shift-JIS 编码检测 | 日企常使用 Shift-JIS 编码 CSV | 中 |
| worker.rs 集成 | 对现有流程无侵入 | 低 |

---

## 12. 测试策略

### 12.1 单元测试

| 测试目标 | 测试内容 |
|----------|----------|
| `build_term_pattern()` 日语 | 片假名前后 `\p{Katakana}` 断言正确 |
| `build_term_pattern()` 中文 | CJK `\p{Han}` 前后断言正确 |
| `build_term_pattern()` 英语 | `\b` 标准词边界正确 |
| `apply_terminology_correction()` | 三语混合文本替换正确 |
| `refresh_initial_prompt_cache()` | 按语言分组 + priority 过滤 |
| CSV 解析 | UTF-8 / Shift-JIS / UTF-8 BOM |

**测试用例示例**：

```rust
#[test]
fn test_japanese_particle_not_matched() {
    // "過酸化物は危険です" — "は" 是助词不是术语的一部分
    // 验证 "過酸" 不会在 "過酸化物" 内错误匹配
}

#[test]
fn test_chinese_compound_word_boundary() {
    // "甲苯二异氰酸酯的生产工艺" — 全词匹配 "甲苯" 不应命中
    // 因为 "甲苯二异氰酸酯" 是一个完整化合物名
}

#[test]
fn test_cas_number_format_recovery() {
    // "CAS one o eight dash eighty eight dash three" → "CAS 108-88-3"
}

#[test]
fn test_ghs_code_format() {
    // "H two two five" → "H225"
}
```

### 11.2 集成测试

| 测试场景 | 方法 |
|----------|------|
| L1 initial_prompt 生效 | 设置日语术语表 → 验证 whisper_engine 的 FullParams 包含 prompt |
| L2 正则三语替换 | Mock 日/中/英混合文本 → 验证输出正确 |
| L3 LLM 校正触发 | 模拟录音停止 → 验证异步 task 被 spawn |
| 缓存刷新联动 | UI 增删术语 → L1+L2 缓存同步刷新 |
| 多语言 prompt 构建 | 日语会议 → 验证 prompt 中术语为日语形式 |

### 11.3 验收标准

- [ ] 用户可在设置页按语言（ja/zh/en/auto）管理术语条目
- [ ] high 优先级术语自动纳入 L1 initial_prompt，normal/low 仅作用于 L2/L3
- [ ] L1 initial_prompt 在 Whisper 引擎启动录音时自动注入，对 Parakeet 透明跳过
- [ ] L2 正则替换在实时转录中生效，延迟 < 1ms
- [ ] 日语长音符「ー」和促音「っ」的变体被正确替换
- [ ] 中文同音字变体被正确替换
- [ ] CAS/UN/GHS 编码格式被正确恢复
- [ ] L3 LLM 校正异步执行，不阻塞 UI
- [ ] 校正结果可逐条接受/拒绝/回滚
- [ ] CSV 导入 100 条术语（含日语 Shift-JIS 编码）耗时 < 3 秒
- [ ] 危化品关键术语准确率 > 99%（L1+L2+L3 合计）
- [ ] 校正审计日志完整可追溯

---

## 13. 风险与应对

| 风险 | 影响 | 概率 | 应对措施 |
|------|------|:---:|----------|
| 日本語 `\p{Katakana}` 正则在旧版 regex crate 不工作 | L2 日语全词匹配失效 | 低 | Rust `regex` 1.0+ 已原生支持。CI 中锁定 regex >= 1.5 |
| initial_prompt 超过 224 token 限制 | 部分术语未注入 | 中 | 按 priority=high + 按语言过滤，严格控量。Prompt 预览面板显示实际 token 数 |
| 日/中/英混合时 LLM 语义错乱 | L3 校正产生错误替换 | 中 | L3 校正后用户逐条确认。Prompt 中三语并列表述任务，避免模型混淆 |
| Shift-JIS CSV 乱码 | 日企用户导入术语失败 | 中 | 自动检测编码（BOM → 字节特征 → 用户手动选择）；前端预览前 5 行 |
| 术语表膨胀至 500+ 条 | L2 正则性能退化 | 低 | 300 条规则内无影响；超过 500 条时评估 Aho-Corasick 替代。按分类分组加载 |
| initial_prompt 对 Parakeet 无效 | Parakeet 用户无 L1 保护 | 中 | 文档明确标注 L1 仅 Whisper；Parakeet 用户至少享有 L2+L3 |
| 危化品行业术语涉及合规风险 | 错误校正导致安全信息错误 | 中 | L3 校正结果必须人工确认后生效。审计日志完整。默认关闭自动接受 |

---

## 附录 A：术语表构建指南（危化品行业版）

### A.1 建表优先级

| 优先级 | 内容 | 纳入管道 | 示例 |
|--------|------|:---:|------|
| **high** | 法定安全术语 + 高頻化学物質名 | L1 + L2 + L3 | `過酸化物`, `TDI`, `H225`, `甲苯二异氰酸酯` |
| **normal** | 一般化学物質名 + 業界略語 | L2 + L3 | `MEK`, `ポリオール`, `硬化剤` |
| **low** | 非クリティカルな用語 + 社内隠語 | L2 | `ライン`, `バッチ`, `釜（かま）` |

### A.2 日语化学术语的音近变体收集方法

1. **实际录音测试（最推荐）**：在工厂安全会议上录音，从 Whisper 输出中收集错误
2. **片假名音节分析**：日语化学术语多为片假名，每个片假名音节对应约 2-3 种易混淆 token。例如 `ポ` 可能被识别为 `ホ`、`ボ`、`ぽ`
3. **促音/长音符变体**：`メチルエチルケトン` 的「ー」可能被省略 → 同时录入 `メチルエチルケトン` 和 `メチルエチルケトン`
4. **拆分变体**：Whisper 将复合片假名词拆分为单词的倾向 → `ポリウレタン` 可能输出为 `ポリ ウレ タン`

### A.3 不建议的做法

- ❌ 试图穷举日语漢字的所有読み方（组合爆炸）
- ❌ 将「爆発性」的匹配规则设得太宽（会匹配到「爆発性物質」「爆発性ガス」等）
- ❌ 直接使用 `.*` 或过于宽泛的模糊匹配

---

## 附录 B：预置术语表（chemical_ja.csv 完整版）

```csv
original,replacement,language,case_sensitive,whole_word,enabled,priority,category,description
かさんかぶつ,過酸化物,ja,false,true,true,high,危険性物質,発音→漢字
かさんか,過酸化物,ja,false,true,true,high,危険性物質,短縮形
いんかせいえきたい,引火性液体,ja,false,true,true,high,危険性分類,消防法
どくげきぶつ,毒劇物,ja,false,true,true,high,法規制,毒劇法
とくていかがくぶっしつ,特定化学物質,ja,false,true,true,high,法規制,特化則
ゆうきようざい,有機溶剤,ja,false,true,true,high,化学物質分類,有機則
あんぜんデータシート,SDS,ja,false,true,true,high,安全情報,旧MSDS
ポリ ウレ たん,ポリウレタン,ja,false,true,true,high,化学物質,分割誤り
ポリ ウレタン,ポリウレタン,ja,false,true,true,high,化学物質,分割誤り
エポキシ じゅし,エポキシ樹脂,ja,false,true,true,high,化学物質,分割誤り
トルエン ジ イソシアネート,TDI,ja,false,true,true,high,化学物質,略称展開
メチル エチル ケトン,MEK,ja,false,true,true,high,化学物質,略称展開
にトロ グリセリン,ニトログリセリン,ja,false,true,true,high,化学物質,発音誤り
ばくはつせい,爆発性,ja,false,true,true,high,危険性分類,GHS
きゅうせいどくせい,急性毒性,ja,false,true,true,high,危険性分類,GHS
ひあぶら,引火点,ja,false,true,true,high,物性,消防法
はっかてん,発火点,ja,false,true,true,high,物性,
じこちゃっかおんど,自己発火温度,ja,false,true,true,high,物性,
じょうきあつ,蒸気圧,ja,false,true,true,high,物性,
ばくはつげんかい,爆発限界,ja,false,true,true,high,物性,LEL/UEL
さんそきゅうしゅう,酸素吸収,ja,false,true,true,normal,反応,
じゅうごう,重合,ja,false,true,true,normal,反応,
はっこう,発酵,ja,false,true,true,normal,反応,
ちゅうわ,中和,ja,false,true,true,normal,反応,
そくざい,促進剤,ja,false,true,true,normal,添加剤,
かたいざい,硬化剤,ja,false,true,true,normal,添加剤,
あんていざい,安定剤,ja,false,true,true,normal,添加剤,
かざい,可塑剤,ja,false,true,true,normal,添加剤,
ピーエル,PL,ja,false,true,true,high,法規制,製造物責任法
あんえいほう,安衛法,ja,false,true,true,high,法規制,労働安全衛生法
しょうぼうほう,消防法,ja,false,true,true,high,法規制,
ピーアールティーアール,PRTR法,ja,false,true,true,high,法規制,化学物質管理
かがくぶっしつかんり,化学物質管理,ja,false,true,true,high,法規制,
リスクアセスメント,リスクアセスメント,ja,false,true,true,high,安全衛生,RA
```

---

> **ドキュメントメンテナンス / 文档维护**：本 PRD V2.0 基于 Meetily v0.3.0 代码库编写，针对危化品制造业日企场景进行了全面改写。实现过程中如遇架构变更或行业适配需求变化，请同步更新本文档。  
> **言語 / 语言**：本文档正文使用中文，代码示例使用 Rust/TypeScript，行业术语使用日/中/英三语标注。