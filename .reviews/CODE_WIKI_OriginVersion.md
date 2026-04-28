# Meetily Code Wiki

> **项目名称**：Meetily (meeting-minutes)
> **版本**：0.3.0
> **许可证**：MIT
> **仓库**：https://github.com/Zackriya-Solutions/meeting-minutes
> **文档生成日期**：2026-04-27

---

## 目录

1. [项目概述](#1-项目概述)
2. [整体架构](#2-整体架构)
3. [技术栈](#3-技术栈)
4. [项目结构总览](#4-项目结构总览)
5. [前端模块 (Next.js + React)](#5-前端模块-nextjs--react)
   - 5.1 [应用入口与路由](#51-应用入口与路由)
   - 5.2 [上下文状态管理 (Contexts)](#52-上下文状态管理-contexts)
   - 5.3 [自定义 Hooks](#53-自定义-hooks)
   - 5.4 [服务层 (Services)](#54-服务层-services)
   - 5.5 [UI 组件体系](#55-ui-组件体系)
   - 5.6 [类型定义](#56-类型定义)
   - 5.7 [工具库](#57-工具库)
6. [后端 Rust 核心模块 (Tauri)](#6-后端-rust-核心模块-tauri)
   - 6.1 [应用入口与管理状态](#61-应用入口与管理状态)
   - 6.2 [音频模块 (audio)](#62-音频模块-audio)
   - 6.3 [转录引擎](#63-转录引擎)
   - 6.4 [摘要引擎 (summary)](#64-摘要引擎-summary)
   - 6.5 [数据库模块 (database)](#65-数据库模块-database)
   - 6.6 [API 与 LLM 客户端](#66-api-与-llm-客户端)
   - 6.7 [通知模块 (notifications)](#67-通知模块-notifications)
   - 6.8 [分析与遥测 (analytics)](#68-分析与遥测-analytics)
   - 6.9 [引导与初始化 (onboarding)](#69-引导与初始化-onboarding)
   - 6.10 [系统托盘与工具模块](#610-系统托盘与工具模块)
7. [Python 后端 (可选)](#7-python-后端-可选)
8. [数据库设计](#8-数据库设计)
9. [项目依赖关系](#9-项目依赖关系)
10. [运行与构建](#10-运行与构建)
11. [关键数据流](#11-关键数据流)
12. [配置项说明](#12-配置项说明)

---

## 1. 项目概述

Meetily 是一个**隐私优先的 AI 会议助手**桌面应用。它能在本地捕获、实时转录并总结会议内容，所有数据完全存储于用户本地设备，绝不发送到云端。

### 核心特性

| 特性 | 说明 |
|------|------|
| **本地转录** | 使用 Whisper/Parakeet 模型在本地 GPU/CPU 上实时转录音频 |
| **AI 摘要** | 对接 Ollama（本地）、Claude、Groq、OpenAI、OpenRouter 及自定义 OpenAI 兼容端点 |
| **GPU 加速** | 支持 CUDA、Vulkan、Metal、CoreML、OpenBLAS、HIPBLAS |
| **跨平台** | Windows、macOS、Linux 全平台支持 |
| **隐私优先** | 所有录音、转录、摘要数据仅存储在本地 SQLite 数据库 |

### 产品定位

- **社区版 (Community Edition)**：永久免费开源，包含本地转录、AI 摘要等核心功能
- **专业版 (Meetily PRO)**：独立代码库，提供增强准确度、自定义模板、PDF/DOCX 导出、说话人识别等高级功能

---

## 2. 整体架构

Meetily 采用 **Tauri 桌面应用架构**，由 Rust 后端核心 + Next.js 前端 UI 组成单一体应用。另有一个可选的 Python FastAPI 后端用于外部 API 调用场景。

```
┌─────────────────────────────────────────────────────────────┐
│                    Meetily 桌面应用                          │
│                                                             │
│  ┌─────────────────────┐    ┌─────────────────────────────┐ │
│  │   Next.js 前端       │    │      Rust 后端 (Tauri)       │ │
│  │   (React + TS)      │◄──►│                             │ │
│  │                     │IPC │  ┌───────────────────────┐  │ │
│  │  • 主页/侧边栏       │    │  │  音频引擎              │  │ │
│  │  • 会议详情页        │    │  │  (CPAL + FFmpeg)      │  │ │
│  │  • 设置页           │    │  ├───────────────────────┤  │ │
│  │  • BlockNote 编辑器  │    │  │  转录引擎              │  │ │
│  │  • 引导流程         │    │  │  (Whisper/Parakeet)    │  │ │
│  │                     │    │  ├───────────────────────┤  │ │
│  │                     │    │  │  摘要引擎              │  │ │
│  │                     │    │  │  (LLM Client/Sidecar)  │  │ │
│  │                     │    │  ├───────────────────────┤  │ │
│  │                     │    │  │  SQLite 数据库         │  │ │
│  │                     │    │  └───────────────────────┘  │ │
│  └─────────────────────┘    └─────────────────────────────┘ │
│                                                             │
│  ┌─────────────────────┐    ┌─────────────────────────────┐ │
│  │  Python 后端 (可选)   │    │  llama-helper sidecar       │ │
│  │  FastAPI :5167       │    │  (内置本地 LLM 推理)         │ │
│  └─────────────────────┘    └─────────────────────────────┘ │
└─────────────────────────────────────────────────────────────┘
```

### 架构层次

| 层次 | 职责 | 技术 |
|------|------|------|
| **表现层** | 用户界面、路由、状态管理 | Next.js 14, React 18, Tailwind CSS, Radix UI |
| **通信层** | 前后端 IPC 通信 | Tauri Commands, Tauri Events |
| **业务逻辑层** | 录音、转录、摘要、数据管理 | Rust (tokio async runtime) |
| **数据层** | 持久化存储 | SQLite (via SQLx), IndexedDB (前端缓存) |
| **基础设施层** | 音频采集、模型推理、GPU 加速 | CPAL, whisper-rs, ONNX Runtime, ffmpeg-sidecar |

---

## 3. 技术栈

### 前端技术栈

| 技术 | 用途 | 版本 |
|------|------|------|
| Next.js | React 框架 (SSR 基础) | 14.2.x |
| React | UI 组件库 | 18.2.x |
| TypeScript | 类型安全 | 5.7.x |
| Tailwind CSS | 原子化 CSS | 3.4.x |
| Radix UI | 无样式可访问组件 | 多版本 |
| BlockNote | 富文本块编辑器 | 0.36.0 |
| TanStack Virtual | 虚拟滚动 | 3.13.x |
| Framer Motion | 动画库 | 11.15.x |
| React Hook Form | 表单管理 | 7.59.x |
| Zod | 数据校验 | 3.25.x |
| date-fns | 日期处理 | 4.1.x |
| sonner | Toast 通知 | 2.0.x |

### Rust 后端技术栈

| Crate | 用途 |
|-------|------|
| tauri 2.6.2 | 桌面应用框架 |
| tokio 1.32 | 异步运行时 |
| sqlx 0.8 | SQLite ORM |
| whisper-rs 0.13.2 | Whisper 语音识别 |
| ort 2.0.0-rc.10 | ONNX Runtime (Parakeet) |
| cpal 0.15.3 | 跨平台音频采集 |
| symphonia 0.5.4 | 音频解码 |
| reqwest 0.11 | HTTP 客户端 |
| serde / serde_json | 序列化 |
| chrono 0.4.31 | 日期时间处理 |
| silero_rs | VAD 语音活动检测 |
| nnnoiseless 0.5 | RNNoise 噪声抑制 |
| ebur128 0.1 | EBU R128 响度标准化 |
| rubato 0.15.0 | 音频重采样 |

### Python 后端技术栈

| 包 | 用途 |
|----|------|
| FastAPI 0.115.x | Web API 框架 |
| uvicorn 0.34.0 | ASGI 服务器 |
| pydantic-ai 0.2.15 | AI Agent 框架 |
| aiosqlite 0.21.0 | 异步 SQLite |
| ollama 0.5.2 | Ollama 客户端 |

---

## 4. 项目结构总览

```
meetily-forked/
├── frontend/                        # Next.js 前端 + Tauri Rust 后端
│   ├── public/                      # 静态资源
│   ├── scripts/                     # 构建辅助脚本
│   ├── src/
│   │   ├── app/                     # Next.js App Router 页面
│   │   │   ├── _components/         # 页面级组件
│   │   │   ├── meeting-details/     # 会议详情页
│   │   │   ├── notes/              # 笔记页
│   │   │   ├── settings/           # 设置页
│   │   │   ├── layout.tsx          # 根布局
│   │   │   ├── page.tsx            # 主页
│   │   │   └── globals.css         # 全局样式
│   │   ├── components/             # 可复用 UI 组件
│   │   │   ├── AISummary/          # AI 摘要展示组件
│   │   │   ├── BlockNoteEditor/    # BlockNote 编辑器
│   │   │   ├── Sidebar/            # 侧边栏
│   │   │   ├── MainContent/        # 主内容布局
│   │   │   ├── MeetingDetails/     # 会议详情子组件
│   │   │   ├── ui/                 # 基础 UI 组件 (shadcn 风格)
│   │   │   ├── onboarding/         # 引导流程
│   │   │   └── ...                 # 其他功能组件
│   │   ├── config/                 # 配置常量
│   │   ├── constants/              # 常量定义
│   │   ├── contexts/               # React 上下文
│   │   ├── hooks/                  # 自定义 Hooks
│   │   ├── lib/                    # 工具函数库
│   │   ├── services/               # 服务层 (IPC 封装)
│   │   └── types/                  # TypeScript 类型定义
│   ├── src-tauri/                  # Tauri Rust 后端
│   │   ├── src/
│   │   │   ├── main.rs             # Rust 入口
│   │   │   ├── lib.rs              # Tauri Builder + 命令注册
│   │   │   ├── config.rs           # 常量配置
│   │   │   ├── state.rs            # 全局状态
│   │   │   ├── audio/              # 音频引擎
│   │   │   ├── api/                # 后端 API 命令
│   │   │   ├── analytics/          # 遥测分析
│   │   │   ├── anthropic/          # Claude API 客户端
│   │   │   ├── console_utils/      # 控制台工具
│   │   │   ├── database/           # 数据库层
│   │   │   ├── groq/               # Groq API 客户端
│   │   │   ├── notifications/      # 系统通知
│   │   │   ├── ollama/             # Ollama 管理
│   │   │   ├── onboarding/         # 引导状态
│   │   │   ├── openai/             # OpenAI API 客户端
│   │   │   ├── openrouter/         # OpenRouter API
│   │   │   ├── parakeet_engine/    # Parakeet 引擎
│   │   │   ├── summary/            # 摘要引擎
│   │   │   ├── tray.rs             # 系统托盘
│   │   │   ├── utils.rs            # 工具函数
│   │   │   └── whisper_engine/     # Whisper 引擎
│   │   ├── migrations/             # SQLite 迁移脚本
│   │   ├── templates/              # 摘要模板 (JSON)
│   │   ├── Cargo.toml              # Rust 依赖
│   │   └── tauri.conf.json         # Tauri 配置
│   ├── package.json                # Node.js 依赖
│   ├── tsconfig.json               # TypeScript 配置
│   └── tailwind.config.ts          # Tailwind CSS 配置
├── backend/                        # Python FastAPI 后端 (可选)
│   ├── app/
│   │   ├── main.py                 # FastAPI 应用
│   │   ├── db.py                   # SQLite 数据库管理
│   │   ├── transcript_processor.py # 转录处理
│   │   └── schema_validator.py     # Schema 验证
│   ├── docker/                     # Docker 配置
│   ├── whisper-custom/             # 自定义 Whisper C++ 服务器
│   └── requirements.txt            # Python 依赖
├── llama-helper/                   # 内置 LLM sidecar (Rust)
│   ├── src/main.rs
│   └── Cargo.toml
├── docs/                           # 项目文档和截图
├── scripts/                        # 构建发布脚本
├── Cargo.toml                      # Rust workspace 配置
└── README.md                       # 项目说明
```

---

## 5. 前端模块 (Next.js + React)

前端采用 Next.js 14 App Router 架构，主要页面包括主页（录音面板）和会议详情页。

### 5.1 应用入口与路由

| 路由 | 文件 | 说明 |
|------|------|------|
| `/` | `src/app/page.tsx` | 主页：录音面板、实时转录展示、状态覆盖层 |
| `/meeting-details?id=xxx` | `src/app/meeting-details/page.tsx` | 会议详情：转录文本、AI 摘要、编辑器 |
| `/notes/[id]` | `src/app/notes/[id]/page.tsx` | 会议笔记页 |
| `/settings` | `src/app/settings/page.tsx` | 设置页面 |

#### 根布局 (RootLayout)

[layout.tsx](file:///e:/ForkedRepo/meetily-forked/frontend/src/app/layout.tsx) 是应用的根组件，负责：

- 初始化所有 React 上下文 Provider（嵌套顺序即为依赖顺序）
- 管理引导流程 (OnboardingFlow) / 主应用 (Sidebar + MainContent) 的切换
- 处理全局事件（系统托盘录音切换、文件拖放导入）
- 禁用生产环境右键菜单

**Provider 嵌套层级**：
```
AnalyticsProvider
└─ RecordingStateProvider
   └─ TranscriptProvider
      └─ ConfigProvider
         └─ OllamaDownloadProvider
            └─ OnboardingProvider
               └─ UpdateCheckProvider
                  └─ SidebarProvider
                     └─ TooltipProvider
                        └─ RecordingPostProcessingProvider
                           └─ ImportDialogProvider
```

#### 主页 (Home)

[page.tsx](file:///e:/ForkedRepo/meetily-forked/frontend/src/app/page.tsx) 实现核心录音工作流：

- 管理录音状态（通过 `RecordingStateContext`）
- 挂载 `TranscriptPanel`（实时转录展示 + 滚动）
- 挂载 `RecordingControls`（开始/停止录音按钮 + 可视化）
- 启动时检查可恢复的会议（Transcription Recovery）
- 自动清理过期会议数据（7 天 + 24 小时保存后）

### 5.2 上下文状态管理 (Contexts)

| Context | 文件 | 职责 |
|---------|------|------|
| `RecordingStateContext` | `contexts/RecordingStateContext.tsx` | 全局录音状态机：IDLE → STARTING → RECORDING → STOPPING → PROCESSING → SAVING → COMPLETED/ERROR |
| `TranscriptContext` | `contexts/TranscriptContext.tsx` | 实时转录数据流管理，包括会议标题 |
| `ConfigContext` | `contexts/ConfigContext.tsx` | 全局配置（模型设置、设备选择、通知、存储位置、Beta 功能） |
| `OnboardingContext` | `contexts/OnboardingContext.tsx` | 新手引导状态管理 |
| `OllamaDownloadContext` | `contexts/OllamaDownloadContext.tsx` | Ollama 模型下载进度 |
| `RecordingPostProcessingProvider` | `contexts/RecordingPostProcessingProvider.tsx` | 录音停止后的转录完成等待和数据库保存流程 |
| `ImportDialogContext` | `contexts/ImportDialogContext.tsx` | 音频导入对话框触发 |

#### RecordingStatus 状态机

```
IDLE ──► STARTING ──► RECORDING ──► STOPPING ──► PROCESSING_TRANSCRIPTS ──► SAVING ──► COMPLETED
  ▲                                                                                           │
  └───────────────────────────── ERROR ◄──────────────────────────────────────────────────────┘
```

### 5.3 自定义 Hooks

| Hook | 文件 | 说明 |
|------|------|------|
| `useRecordingStart` | `hooks/useRecordingStart.ts` | 封装开始录音的完整流程：权限检查、设备选择、模型验证 |
| `useRecordingStop` | `hooks/useRecordingStop.ts` | 封装停止录音流程，触发转录完成等待和后处理 |
| `useRecordingStateSync` | `hooks/useRecordingStateSync.ts` | 前后端录音状态同步（防止页面刷新失步） |
| `usePermissionCheck` | `hooks/usePermissionCheck.ts` | 麦克风权限检查 |
| `useAudioPlayer` | `hooks/useAudioPlayer.ts` | 音频播放器控制（播放/暂停/跳转） |
| `useAutoScroll` | `hooks/useAutoScroll.ts` | 转录面板自动滚动 |
| `useImportAudio` | `hooks/useImportAudio.ts` | 音频导入功能 |
| `useModalState` | `hooks/useModalState.ts` | 模态框管理 |
| `useNavigation` | `hooks/useNavigation.ts` | 导航工具 |
| `usePaginatedTranscripts` | `hooks/usePaginatedTranscripts.ts` | 转录分页加载 |
| `useProcessingProgress` | `hooks/useProcessingProgress.ts` | 处理进度追踪 |
| `useTranscriptRecovery` | `hooks/useTranscriptRecovery.ts` | 转录恢复（崩溃后） |
| `useTranscriptStreaming` | `hooks/useTranscriptStreaming.ts` | 实时转录流监听 |
| `useTranscriptionModels` | `hooks/useTranscriptionModels.ts` | 转录模型管理 |
| `useUpdateCheck` | `hooks/useUpdateCheck.ts` | 应用更新检查 |
| `usePlatform` | `hooks/usePlatform.ts` | 平台检测 |

**会议详情页专用 Hooks** (`hooks/meeting-details/`):

| Hook | 说明 |
|------|------|
| `useMeetingData` | 会议数据加载和状态管理 |
| `useMeetingOperations` | 会议 CRUD 操作 |
| `useModelConfiguration` | 模型配置切换 |
| `useSummaryGeneration` | 摘要生成触发和轮询 |
| `useTemplates` | 摘要模板管理 |
| `useCopyOperations` | 复制/导出操作 |

### 5.4 服务层 (Services)

服务层将 Tauri IPC 调用封装为类型安全的 TypeScript 接口。

| Service | 文件 | 说明 |
|---------|------|------|
| `recordingService` | `services/recordingService.ts` | 录音生命周期 IPC 调用（开始/停止/暂停/恢复） |
| `storageService` | `services/storageService.ts` | 会议存储（保存/获取/删除） |
| `transcriptService` | `services/transcriptService.ts` | 转录流事件监听管理 |
| `configService` | `services/configService.ts` | 配置读写（模型/设备/通知等） |
| `indexedDBService` | `services/indexedDBService.ts` | 前端 IndexedDB 缓存层 |
| `updateService` | `services/updateService.ts` | 应用更新检查 |

### 5.5 UI 组件体系

#### 核心页面组件

| 组件 | 说明 |
|------|------|
| `Sidebar` | 侧边栏：会议列表、搜索、导航、设置入口 |
| `MainContent` | 主内容区容器，处理侧边栏折叠边距 |
| `RecordingControls` | 录音按钮组：开始/停止/暂停/恢复 |
| `RecordingStatusBar` | 录音状态栏：时长、音频电平可视化 |
| `TranscriptPanel` | 实时转录面板：流式展示转录文本 |
| `TranscriptView` | 转录文本视图 |
| `VirtualizedTranscriptView` | 虚拟滚动转录视图（TanStack Virtual） |
| `AudioPlayer` | 音频播放器控件 |

#### 会议详情组件

| 组件 | 说明 |
|------|------|
| `SummaryPanel` | AI 摘要面板 |
| `SummaryGeneratorButtonGroup` | 摘要生成/重新生成按钮 |
| `SummaryUpdaterButtonGroup` | 摘要更新操作按钮 |
| `TranscriptButtonGroup` | 转录操作按钮 |
| `RetranscribeDialog` | 重新转录对话框 |

#### AI 摘要渲染组件

| 组件 | 说明 |
|------|------|
| `AISummary/index.tsx` | 摘要主组件，支持 Legacy/Markdown/BlockNote 三种格式 |
| `AISummary/Block.tsx` | 摘要块渲染 |
| `AISummary/Section.tsx` | 摘要区域渲染 |
| `AISummary/BlockNoteSummaryView.tsx` | BlockNote 格式摘要渲染 |

#### BlockNote 编辑器

| 组件 | 说明 |
|------|------|
| `BlockNoteEditor/Editor.tsx` | 富文本编辑器主组件 |
| `BlockNoteEditor/BasicBlockNoteTest.tsx` | BlockNote 基础测试组件 |

#### 引导流程组件

| 组件 | 说明 |
|------|------|
| `OnboardingContainer` | 引导流程容器 |
| `OnboardingFlow` | 引导流程编排 |
| `WelcomeStep` | 欢迎页 |
| `PermissionsStep` | 权限设置步骤 |
| `SetupOverviewStep` | 设置概览 |
| `DownloadProgressStep` | 模型下载进度 |

#### 模型管理组件

| 组件 | 说明 |
|------|------|
| `WhisperModelManager` | Whisper 模型下载/管理 |
| `ParakeetModelManager` | Parakeet 模型下载/管理 |
| `BuiltInModelManager` | 内置 LLM 模型管理 |
| `ModelDownloadProgress` | 模型下载进度条 |
| `ModelSettingsModal` | 模型设置弹窗 |

#### 基础 UI 组件 (ui/)

`components/ui/` 目录包含基于 Radix UI 和 shadcn 风格的基础组件：
`accordion`, `alert`, `button`, `button-group`, `command`, `dialog`, `dropdown-menu`, `form`, `input`, `input-group`, `label`, `popover`, `progress`, `scroll-area`, `select`, `separator`, `sheet`, `switch`, `tabs`, `textarea`, `tooltip`, `visually-hidden`

### 5.6 类型定义

[types/index.ts](file:///e:/ForkedRepo/meetily-forked/frontend/src/types/index.ts) 定义了核心数据结构：

| 类型 | 说明 |
|------|------|
| `Transcript` | 转录段：id、text、timestamp、audio_start_time、audio_end_time、duration、confidence |
| `TranscriptUpdate` | 实时转录更新事件载荷 |
| `Block` | 摘要块：id、type、content、color |
| `Section` | 摘要区域：title、blocks[] |
| `Summary` | 完整摘要：Section 的键值映射 |
| `SummaryDataResponse` | 摘要 API 响应：支持 markdown/summary_json/legacy |
| `MeetingMetadata` | 会议元数据 |
| `PaginatedTranscriptsResponse` | 分页转录响应 |
| `TranscriptSegmentData` | 虚拟滚动转录段数据 |

### 5.7 工具库

| 文件 | 说明 |
|------|------|
| `lib/analytics.ts` | PostHog 遥测客户端封装 |
| `lib/builtin-ai.ts` | 内置 AI 模型 IPC 调用 |
| `lib/parakeet.ts` | Parakeet 引擎 IPC 调用 |
| `lib/whisper.ts` | Whisper 引擎 IPC 调用 |
| `lib/utils.ts` | 通用工具函数 |
| `lib/recordingNotification.tsx` | 录音通知工具 |
| `constants/languages.ts` | 支持语言列表 |
| `constants/audioFormats.ts` | 支持音频格式 |
| `constants/modelDefaults.ts` | 模型默认值 |

---

## 6. 后端 Rust 核心模块 (Tauri)

### 6.1 应用入口与管理状态

#### main.rs

[main.rs](file:///e:/ForkedRepo/meetily-forked/frontend/src-tauri/src/main.rs) — Rust 应用入口

- 设置 `RUST_LOG=info` 环境变量
- 初始化 `env_logger` 日志系统
- 调用 `app_lib::run()` 启动 Tauri 应用
- Windows 下设置 `windows_subsystem = "windows"` 隐藏控制台

#### lib.rs — Tauri Builder

[lib.rs](file:///e:/ForkedRepo/meetily-forked/frontend/src-tauri/src/lib.rs) 是应用的核心组装文件：

**模块声明**：
```rust
pub mod analytics;
pub mod api;
pub mod audio;
pub mod config;
pub mod console_utils;
pub mod database;
pub mod notifications;
pub mod ollama;
pub mod onboarding;
pub mod openai;
pub mod anthropic;
pub mod groq;
pub mod openrouter;
pub mod parakeet_engine;
pub mod state;
pub mod summary;
pub mod tray;
pub mod utils;
pub mod whisper_engine;
```

**`run()` 函数**负责：
1. 注册 Tauri 插件（通知、存储、对话框、更新器、进程）
2. 初始化全局状态（ParallelProcessor、NotificationManager、SystemAudio、ModelManager）
3. 在 `setup` 阶段进行启发式初始化：
   - 创建系统托盘
   - 异步初始化通知系统
   - 设置并初始化 Whisper 引擎
   - 设置并初始化 Parakeet 引擎
   - 初始化摘要引擎的 ModelManager
   - 初始化 SQLite 数据库
   - 初始化摘要模板目录
4. 注册 **100+ Tauri 命令** 到 `invoke_handler`
5. 在应用退出事件中执行清理（数据库 WAL checkpoint、sidecar 关闭）

**关键全局变量**：
- `RECORDING_FLAG`：原子bool，跟踪录音状态
- `LANGUAGE_PREFERENCE`：全局语言偏好（默认 `auto-translate`）

#### state.rs

```rust
pub struct AppState {
    pub db_manager: DatabaseManager,
}
```

简单的全局状态结构，持有数据库管理器实例，在应用关闭时用于数据库清理。

#### config.rs

[config.rs](file:///e:/ForkedRepo/meetily-forked/frontend/src-tauri/src/config.rs) 定义全局配置常量：

- `DEFAULT_WHISPER_MODEL`：`"large-v3-turbo"`
- `DEFAULT_PARAKEET_MODEL`：`"parakeet-tdt-0.6b-v3-int8"`
- `WHISPER_MODEL_CATALOG`：Whisper 模型目录（名称、文件名、大小、准确度、速度、描述）

### 6.2 音频模块 (audio)

音频模块是整个应用最复杂的子系统，负责音频采集、处理、混合和保存。

#### 模块结构

```
audio/
├── mod.rs                  # 模块入口，统一导出
├── audio_processing.rs     # 音频预处理（重采样、归一化）
├── batch_processor.rs      # 批量音频处理
├── buffer_pool.rs          # 音频缓冲池
├── common.rs               # 通用工具
├── constants.rs            # 音频常量
├── decoder.rs              # 音频解码器（支持多种格式）
├── device_detection.rs     # 设备检测（自适应缓冲）
├── device_monitor.rs       # 设备插拔监控（AirPods/蓝牙）
├── diagnostics.rs          # 诊断日志
├── encode.rs               # 音频编码
├── ffmpeg.rs               # FFmpeg 集成
├── ffmpeg_mixer.rs         # FFmpeg 自适应混音器
├── hardware_detector.rs    # 硬件性能检测（GPU/CPU）
├── import.rs               # 外部音频导入
├── incremental_saver.rs    # 增量保存 + 检查点
├── level_monitor.rs        # 音频电平监控
├── pipeline.rs             # 音频处理管道
├── playback_monitor.rs     # 播放设备检测（蓝牙警告）
├── post_processor.rs       # 后处理器
├── recording_commands.rs   # 录音 Tauri 命令层
├── recording_manager.rs    # 录音管理器（核心）
├── recording_preferences.rs# 录音偏好设置
├── recording_saver.rs      # 录音文件保存器
├── recording_state.rs      # 录音状态类型
├── retranscription.rs      # 重新转录功能
├── simple_level_monitor.rs # 简版电平监控
├── stream.rs               # 音频流管理
├── stt.rs                  # 语音转文本接口
├── system_audio_commands.rs# 系统音频命令
├── system_audio_stream.rs  # 系统音频流
├── system_detector.rs      # 系统音频检测
├── vad.rs                  # 语音活动检测 (VAD)
├── capture/                # 音频捕获
│   ├── backend_config.rs   # 后端配置
│   ├── core_audio.rs       # macOS CoreAudio
│   ├── microphone.rs       # 麦克风捕获
│   ├── system.rs           # 系统音频捕获
│   └── mod.rs
├── devices/                # 设备管理
│   ├── configuration.rs    # 设备配置
│   ├── discovery.rs        # 设备发现
│   ├── fallback.rs         # 回退策略
│   ├── microphone.rs       # 麦克风设备
│   ├── speakers.rs         # 扬声器设备
│   └── platform/           # 平台特定实现
│       ├── linux.rs
│       ├── macos.rs
│       ├── windows.rs
│       └── mod.rs
└── transcription/          # 转录引擎抽象
    ├── engine.rs           # 引擎管理
    ├── parakeet_provider.rs# Parakeet 提供者
    ├── whisper_provider.rs # Whisper 提供者
    ├── provider.rs         # 提供者 trait
    └── worker.rs           # 转录工作线程
```

#### 关键组件说明

**RecordingManager** (`recording_manager.rs`)：
核心录音管理器，协调音频采集、处理和转发的生命周期。管理麦克风输入和系统音频输入的双轨录音。

**FFmpegAudioMixer** (`ffmpeg_mixer.rs`)：
自适应音频混音器，实现：
- 麦克风和系统音频的混合
- RNNoise 噪声抑制
- EBU R128 响度标准化
- 智能 Ducking（语音时降低系统音频音量）
- 缓冲区统计和健康检查

**IncrementalSaver** (`incremental_saver.rs`)：
增量音频保存器，支持：
- 检查点机制，防止崩溃丢失数据
- 断点续传恢复
- 增量追加写入

**silero_rs VAD** (`vad.rs`)：
语音活动检测，使用 Silero VAD 模型识别有效语音段，过滤静音。

**RecordingCommands** (`recording_commands.rs`)：
Tauri 命令层，暴露给前端：
- `start_recording` / `stop_recording`
- `pause_recording` / `resume_recording`
- `is_recording_paused` / `get_recording_state`
- `get_meeting_folder_path` / `get_transcript_history`
- `get_recording_meeting_name`
- `poll_audio_device_events`（设备插拔事件轮询）

### 6.3 转录引擎

#### Whisper 引擎 (whisper_engine/)

```
whisper_engine/
├── mod.rs
├── whisper_engine.rs      # Whisper 引擎核心
├── commands.rs            # Tauri 命令
├── system_monitor.rs      # 系统资源监控
├── parallel_processor.rs  # 并行处理器
└── parallel_commands.rs   # 并行处理命令
```

基于 `whisper-rs` (whisper.cpp 的 Rust 绑定) 实现：

**WhisperEngine**：
- 模型加载/卸载
- 模型下载和验证
- 音频转录（支持 GPU 加速）
- 模型目录管理

**ParallelProcessor**：
- 多线程并行转录
- 根据系统资源（CPU/内存）自动调整工作线程数
- 音频分块和合并
- 资源约束检查

**Tauri 命令**：
- `whisper_init` / `whisper_load_model`
- `whisper_get_available_models` / `whisper_download_model`
- `whisper_transcribe_audio`
- `whisper_cancel_download` / `whisper_delete_corrupted_model`
- 并行处理命令：`initialize_parallel_processor` / `start_parallel_processing` 等

#### Parakeet 引擎 (parakeet_engine/)

```
parakeet_engine/
├── mod.rs
├── parakeet_engine.rs     # Parakeet 引擎核心
├── model.rs               # ONNX 模型封装
└── commands.rs            # Tauri 命令
```

基于 ONNX Runtime (`ort`) 实现，使用 NVIDIA NeMo Parakeet 模型：

**ParakeetEngine**：
- ONNX 模型加载和推理
- Int8 量化支持
- 模型下载和管理
- 高吞吐量转录（M4 Max 上可实时，Zen 3 上 20x）

**命令**：
- `parakeet_init` / `parakeet_load_model`
- `parakeet_get_available_models` / `parakeet_download_model`
- `parakeet_transcribe_audio`
- `parakeet_retry_download` / `parakeet_cancel_download`

#### 转录提供者抽象 (transcription/)

| 文件 | 说明 |
|------|------|
| `provider.rs` | `TranscriptionProvider` trait 定义 |
| `whisper_provider.rs` | Whisper 实现 |
| `parakeet_provider.rs` | Parakeet 实现 |
| `engine.rs` | 引擎管理和调度 |
| `worker.rs` | 异步转录工作线程 |

### 6.4 摘要引擎 (summary)

```
summary/
├── mod.rs                     # 模块入口，导出所有类型
├── commands.rs                # Tauri 命令（处理/获取/取消/保存摘要）
├── llm_client.rs              # LLM 客户端（统一接口）
├── processor.rs               # 文本分块 + 摘要生成
├── service.rs                 # 摘要服务（编排层）
├── template_commands.rs       # 模板管理命令
├── templates/                 # 摘要模板系统
│   ├── mod.rs
│   ├── defaults.rs            # 默认模板
│   ├── loader.rs              # 模板加载器
│   └── types.rs               # 模板类型
└── summary_engine/            # 内置 LLM 引擎 (sidecar)
    ├── mod.rs
    ├── client.rs              # sidecar 通信客户端
    ├── commands.rs            # 内置 AI Tauri 命令
    ├── model_manager.rs       # 模型下载和管理
    ├── models.rs              # 模型定义
    └── sidecar.rs             # llama-helper sidecar 管理
```

#### LLM 客户端 (llm_client.rs)

`LLMProvider` 枚举统一了所有 AI 提供者的接口：

| Provider | 实现位置 | 说明 |
|----------|---------|------|
| `Ollama` | `ollama/` | 本地 LLM（推荐），通过 HTTP API 调用 |
| `Claude` | `anthropic/` | Anthropic Claude API |
| `Groq` | `groq/` | Groq 快速推理 API |
| `OpenAI` | `openai/` | OpenAI API |
| `OpenRouter` | `openrouter/` | OpenRouter 聚合 API |
| `CustomOpenAI` | `summary/mod.rs` | 自定义 OpenAI 兼容端点 |
| `BuiltIn` | `summary_engine/` | 内置本地 LLM（llama-helper sidecar） |

#### 文本处理 (processor.rs)

关键函数：

| 函数 | 说明 |
|------|------|
| `chunk_text()` | 将长文本按 token 数量分块（支持 overlap） |
| `rough_token_count()` | 粗略估算 token 数量 |
| `generate_meeting_summary()` | 调用 LLM 生成单块摘要 |
| `clean_llm_markdown_output()` | 清理 LLM 输出的 Markdown 格式 |
| `extract_meeting_name_from_markdown()` | 从摘要中提取会议名称 |

#### 摘要模板系统 (templates/)

摘要模板是 JSON 文件，定义了摘要的结构化格式。位于：
- 内置模板：`src-tauri/templates/*.json`
- 用户模板：存储在数据库/文件系统中

**内置模板**：
- `standard_meeting.json`：标准会议
- `daily_standup.json`：每日站会
- `project_sync.json`：项目同步
- `retrospective.json`：回顾会议
- `sales_marketing_client_call.json`：销售/市场客户通话
- `psychatric_session.json`：心理咨询

#### 内置 AI 引擎 (summary_engine/)

通过 `llama-helper` sidecar 进程实现本地 LLM 推理：

- **Sidecar 管理**：启动/停止/健康检查/优雅关闭
- **模型管理**：下载、删除、状态查询
- **推理**：通过 HTTP 与 sidecar 通信

Sidecar 路径：`llama-helper/src/main.rs`，编译为独立二进制，随应用打包分发。

### 6.5 数据库模块 (database)

```
database/
├── mod.rs
├── manager.rs              # DatabaseManager 核心类
├── models.rs               # 数据模型 (ORM)
├── setup.rs                # 数据库初始化
├── commands.rs             # Tauri 命令
└── repositories/           # 数据访问层
    ├── mod.rs
    ├── meeting.rs          # 会议 CRUD
    ├── setting.rs          # 设置读写
    ├── summary.rs          # 摘要 CRUD
    ├── transcript.rs       # 转录 CRUD
    └── transcript_chunk.rs # 转录块管理
```

#### 数据模型 (models.rs)

| 模型 | 对应表 | 说明 |
|------|--------|------|
| `MeetingModel` | `meetings` | 会议：id、title、created_at、updated_at、folder_path |
| `Transcript` | `transcripts` | 转录段：id、meeting_id、transcript、timestamp、audio_start_time、audio_end_time、duration |
| `SummaryProcess` | `summary_processes` | 摘要处理记录：meeting_id、status、result、start_time、end_time、result_backup |
| `TranscriptChunk` | `transcript_chunks` | 转录块：meeting_id、transcript_text、model、chunk_size、overlap |
| `Setting` | `settings` | 模型设置：provider、model、whisper_model、各种 API Key |
| `TranscriptSetting` | `transcript_settings` | 转录设置 |

#### 仓库层 (repositories/)

每个仓库封装对特定表的 CRUD 操作，使用 SQLx 进行异步 SQL 查询。

**MeetingRepository**：
- `create_meeting()` / `get_meeting()`
- `get_all_meetings()` / `update_meeting_title()`
- `delete_meeting()` / `get_meeting_summary()`
- `update_meeting_summary()`

**TranscriptRepository**：
- `save_transcript()` / `get_transcripts_by_meeting()`
- `save_transcripts_batch()` / `search_transcripts()`
- `get_paginated_transcripts()` / `get_transcript_count()`

**SettingRepository**：
- `get_model_config()` / `save_model_config()`
- `get_api_key()` / `save_api_key()`（分 provider）
- `get_custom_openai_config()` / `save_custom_openai_config()`

**SummaryRepository**：
- `create_process()` / `update_process()` / `get_process()`
- `backup_result()` / `restore_backup()`

#### 数据库迁移 (migrations/)

迁移文件位于 `src-tauri/migrations/`，按时间戳排序：

| 迁移 | 说明 |
|------|------|
| `20250916100000_initial_schema.sql` | 初始表结构（meetings, transcripts, settings 等） |
| `20250920155811_add_openrouter_api_key.sql` | 添加 OpenRouter API Key 字段 |
| `20251006000000_add_audio_sync_fields.sql` | 添加音频同步字段 |
| `20251010153942_add_ollama_endpoint.sql` | 添加 Ollama 自定义端点 |
| `20251101000000_add_summary_backup.sql` | 添加摘要备份机制 |
| `20251105120000_add_pro_license_custom_openai.sql` | 添加 PRO 授权和自定义 OpenAI 配置 |
| `20251110000000_add_grace_period_to_licensing.sql` | 添加授权宽限期 |
| `20251110000001_add_speaker_field.sql` | 添加说话人字段 |
| `20251223000000_add_meeting_notes.sql` | 添加会议笔记功能 |
| `20251229000000_add_gemini_api_key.sql` | 添加 Gemini API Key |

### 6.6 API 与 LLM 客户端

#### API 命令模块 (api/)

[api/commands.rs](file:///e:/ForkedRepo/meetily-forked/frontend/src-tauri/src/api/commands.rs) 暴露数据库操作的 Tauri 命令：

- `api_get_meetings` / `api_get_meeting` / `api_get_meeting_metadata`
- `api_get_meeting_transcripts` / `api_search_transcripts`
- `api_save_transcript` / `api_save_meeting_title`
- `api_delete_meeting` / `api_get_model_config` / `api_save_model_config`
- `api_get_api_key` / `api_get_custom_openai_config`
- `open_meeting_folder` / `open_external_url`
- `test_backend_connection` / `debug_backend_connection`

#### LLM Provider 模块

| 模块 | 文件 | 说明 |
|------|------|------|
| `ollama` | `ollama/ollama.rs` | Ollama HTTP API 客户端，支持模型列表/拉取/删除 |
| `openai` | `openai/openai.rs` | OpenAI API 模型列表获取 |
| `anthropic` | `anthropic/anthropic.rs` | Anthropic Claude 模型列表获取 |
| `groq` | `groq/groq.rs` | Groq 模型列表获取 |
| `openrouter` | `openrouter/openrouter.rs` | OpenRouter 模型列表获取 |

### 6.7 通知模块 (notifications)

```
notifications/
├── mod.rs
├── commands.rs       # Tauri 通知命令
├── manager.rs        # NotificationManager 核心
├── settings.rs       # 通知设置
├── system.rs         # 系统通知 API
└── types.rs          # 通知类型定义
```

功能：
- 录音开始/停止/暂停/恢复通知
- 转录完成通知
- 会议提醒
- 勿扰模式检测
- 通知偏好管理
- 用户同意管理

### 6.8 分析与遥测 (analytics)

```
analytics/
├── mod.rs
├── analytics.rs      # PostHog 集成
└── commands.rs       # 遥测 Tauri 命令
```

基于 PostHog 的用户行为分析（可禁用）：
- 页面浏览追踪
- 事件追踪（录音开始/停止、会议删除、设置变更）
- 会话管理
- 用户首次启动追踪
- 每日活跃用户统计

### 6.9 引导与初始化 (onboarding)

`onboarding.rs` 管理新手引导状态：
- `get_onboarding_status`：检查引导是否完成
- `save_onboarding_status_cmd`：保存引导状态
- `complete_onboarding`：标记引导完成

### 6.10 系统托盘与工具模块

#### 系统托盘 (tray.rs)

- 创建系统托盘图标和菜单
- 支持"开始/停止录音"切换
- 显示应用状态
- 点击托盘图标恢复窗口

#### 工具模块 (utils.rs)

通用工具函数，包含：
- `open_system_settings`（macOS 系统设置跳转）
- 文件路径处理
- 字符串工具

---

## 7. Python 后端 (可选)

Meetily 包含一个可选的 Python FastAPI 后端，主要用于外部 API 调用场景。

### 入口文件：main.py

[main.py](file:///e:/ForkedRepo/meetily-forked/backend/app/main.py) 定义 FastAPI 应用：

**核心类**：

| 类 | 说明 |
|----|------|
| `SummaryProcessor` | 摘要处理核心：线程安全、支持分块处理 |
| `DatabaseManager` (db.py) | SQLite 数据库操作：异步 CRUD |

**API 端点**：

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/get-model-config` | 获取模型配置 |
| POST | `/save-model-config` | 保存模型配置 |
| POST | `/save-transcript-config` | 保存转录配置 |
| POST | `/get-api-key` | 获取 API Key |
| POST | `/save-meeting-summary` | 保存会议摘要 |
| POST | `/search-transcripts` | 搜索转录文本 |

**服务端口**：`5167`

**运行方式**：
```bash
cd backend
pip install -r requirements.txt
python app/main.py
```

### transcript_processor.py

转录处理器，基于 `pydantic-ai`：
- 将转录文本分块处理
- 调用 AI 模型生成结构化摘要
- 支持多种 AI 提供者（Ollama, Claude, Groq, OpenAI）
- 自定义提示词和分块参数

### db.py

异步 SQLite 数据库管理器：
- `DatabaseManager` 类封装所有数据库操作
- `save_meeting()` / `get_all_meetings()` / `get_meeting()`
- `save_transcript()` / `save_model_config()`
- `create_process()` / `update_process()` / `get_transcript_data()`
- API Key 管理（分 provider）

### whisper-custom/

自定义 Whisper C++ 服务器，基于 `whisper.cpp`：
- `server.cpp`：HTTP 服务器实现（使用 `httplib.h`）
- 支持 CPU 和 GPU Docker 部署
- 可作为独立转录服务运行

---

## 8. 数据库设计

### ER 图（简化）

```
┌──────────────┐       ┌──────────────────┐       ┌─────────────────────┐
│   meetings   │       │   transcripts     │       │  summary_processes  │
├──────────────┤       ├──────────────────┤       ├─────────────────────┤
│ id (PK)      │◄──────│ meeting_id (FK)  │       │ meeting_id (FK)     │
│ title        │       │ id (PK)          │       │ status              │
│ created_at   │       │ transcript (TEXT)│       │ result (JSON)       │
│ updated_at   │       │ timestamp        │       │ result_backup       │
│ folder_path  │       │ audio_start_time │       │ start_time          │
│ summary_json │       │ audio_end_time   │       │ end_time            │
└──────────────┘       │ duration         │       │ chunk_count         │
                       │ confidence       │       │ processing_time     │
                       │ summary          │       │ error               │
                       └──────────────────┘       └─────────────────────┘

┌──────────────┐       ┌──────────────────┐
│   settings   │       │ transcript_chunks │
├──────────────┤       ├──────────────────┤
│ id (PK)      │       │ meeting_id (FK)  │
│ provider     │       │ meeting_name     │
│ model        │       │ transcript_text  │
│ whisperModel │       │ model            │
│ *ApiKey      │       │ model_name       │
│ ollamaEndpoint│      │ chunk_size       │
│ customOpenAI │       │ overlap          │
└──────────────┘       └──────────────────┘
```

### 关键设计决策

| 决策 | 说明 |
|------|------|
| **SQLite** | 单文件数据库，零配置，内嵌于桌面应用，支持 WAL 模式 |
| **SQLx** | 编译时 SQL 检查，异步 I/O，无 ORM 开销 |
| **迁移管理** | 时间戳命名，按序执行，保证数据库版本一致性 |
| **摘要备份** | `result_backup` 字段保存前次摘要，允许撤销重新生成 |
| **音频同步字段** | `audio_start_time`/`audio_end_time` 实现音频-转录同步播放 |

---

## 9. 项目依赖关系

### Rust Workspace 依赖图

```
Cargo.toml (workspace)
├── frontend/src-tauri (meetily 主程序)
│   ├── tauri 2.6.2 (桌面框架)
│   ├── whisper-rs 0.13.2 (语音识别)
│   ├── ort 2.0.0-rc.10 (ONNX Runtime)
│   ├── cpal 0.15.3 (音频捕获)
│   ├── sqlx 0.8 (SQLite ORM)
│   ├── silero_rs (VAD 语音检测)
│   ├── nnnoiseless 0.5 (噪声抑制)
│   ├── symphonia 0.5.4 (音频解码)
│   ├── rubato 0.15.0 (重采样)
│   ├── ebur128 0.1 (响度标准化)
│   ├── ffmpeg-sidecar (FFmpeg 集成)
│   ├── reqwest 0.11 (HTTP 客户端)
│   ├── posthog-rs 0.3.7 (分析)
│   └── ... (100+ 依赖)
└── llama-helper (内置 LLM sidecar)
    └── (独立二进制，随应用打包)
```

### Node.js 依赖分层

```
核心框架
├── next 14.2.x + react 18.2.x
├── @tauri-apps/api 2.6.0
│
UI 库
├── @radix-ui/* (无障碍原语组件)
├── tailwindcss 3.4.x + tailwindcss-animate
├── framer-motion 11.15.x
├── lucide-react 0.469.x (图标)
├── @blocknote/* 0.36.0 (富文本编辑器)
│
工具库
├── react-hook-form 7.59.x + zod 3.25.x
├── date-fns 4.1.x
├── lodash 4.17.x
├── sonner 2.0.x (toast 通知)
├── clsx + tailwind-merge
│
Tauri 插件
├── @tauri-apps/plugin-fs/store/notification/process/updater/os
└── cmdk 1.1.x (命令面板)
```

### 前后端通信架构

```
┌─────────────────────────────────────────────────────┐
│                   Tauri IPC 层                       │
│                                                     │
│  前端 (TypeScript)          后端 (Rust)              │
│  ┌─────────────┐          ┌────────────────┐       │
│  │ invoke()    │──CMD──► │ #[tauri::command]│       │
│  │ 调用命令     │◄─RESULT─│ 命令处理函数      │       │
│  └─────────────┘          └────────────────┘       │
│                                                     │
│  ┌─────────────┐          ┌────────────────┐       │
│  │ listen()    │◄─EVENT──│ app.emit()      │       │
│  │ 监听事件     │          │ 发送事件         │       │
│  └─────────────┘          └────────────────┘       │
│                                                     │
│  主要命令调用路径                                    │
│  ────────────────                                   │
│  recordingService → start_recording (cmd)           │
│  storageService   → api_save_transcript (cmd)       │
│  configService    → api_save_model_config (cmd)     │
│  transcriptService ← transcript-update (event)      │
│  RecordingPostProcessing ← recording-stopped(event) │
└─────────────────────────────────────────────────────┘
```

---

## 10. 运行与构建

### 开发环境要求

| 工具 | 版本要求 |
|------|---------|
| Node.js | 18+ |
| pnpm | 推荐 (npm/yarn 也可) |
| Rust | 1.77+ |
| FFmpeg | 用于音频处理 |
| CMake | 用于编译 whisper.cpp |

### 快速启动开发

```bash
# 克隆仓库
git clone https://github.com/Zackriya-Solutions/meeting-minutes
cd meeting-minutes/frontend

# 安装依赖
pnpm install

# 开发模式启动（自动检测 GPU）
pnpm tauri:dev

# 指定 GPU 后端
pnpm tauri:dev:cuda      # NVIDIA GPU
pnpm tauri:dev:vulkan    # AMD/Intel GPU
pnpm tauri:dev:metal     # Apple Silicon (Metal)
pnpm tauri:dev:coreml    # Apple CoreML
```

### 构建生产版本

```bash
# CPU 构建
pnpm tauri:build:cpu

# GPU 加速构建
pnpm tauri:build:cuda    # NVIDIA
pnpm tauri:build:vulkan  # AMD/Intel
pnpm tauri:build:metal   # macOS
pnpm tauri:build:coreml  # macOS CoreML
```

### 构建脚本说明

| 脚本 | 用途 |
|------|------|
| `build-gpu.sh` / `build-gpu.ps1` / `build-gpu.bat` | 自动检测 GPU 并选择最优构建配置 |
| `build.ps1` / `build.bat` | 通用构建脚本 |
| `dev-gpu.sh` / `dev-gpu.ps1` / `dev-gpu.bat` | 开发模式自动检测 GPU |
| `clean_build.sh` / `clean_build_windows.bat` | 清理构建 |
| `package-app.sh` | macOS 打包脚本 |

### Python 后端运行

```bash
cd backend
pip install -r requirements.txt
python app/main.py
# 服务运行在 http://localhost:5167
```

### Docker 部署 (Python 后端 + Whisper)

```bash
cd backend
docker-compose up -d
```

支持三种 Whisper 服务配置：
- `Dockerfile.server-cpu`：CPU 版本
- `Dockerfile.server-gpu`：NVIDIA GPU 版本
- `Dockerfile.server-macos`：macOS 版本

---

## 11. 关键数据流

### 11.1 录音 → 转录 → 保存流程

```
1. 用户点击"开始录音"
   │
2. Frontend: RecordingControls → useRecordingStart()
   ├── 检查麦克风权限
   ├── 验证转录模型已加载
   │
3. Tauri Command: start_recording(mic_device, system_device, meeting_name)
   │
4. Rust: recording_commands::start_recording_with_devices_and_meeting()
   ├── 初始化 RecordingManager
   ├── 启动音频流（麦克风 + 系统音频）
   ├── 启动转录 Worker
   │
5. Rust: FFmpegAudioMixer (持续运行)
   ├── RNNoise 噪声抑制
   ├── VAD 语音活动检测
   ├── 麦克风 + 系统音频混合
   ├── EBU R128 响度标准化
   │
6. Rust: WhisperEngine / ParakeetEngine (实时转录)
   ├── 音频分块 (chunk)
   ├── 并行转录处理
   ├── 通过 Tauri Event 发送 transcript-update
   │
7. Frontend: TranscriptContext 接收 transcript-update 事件
   ├── 追加到转录列表
   ├── TranscriptPanel 实时渲染
   │
8. 用户点击"停止录音"
   │
9. Frontend: useRecordingStop()
   │
10. Rust: 停止录音 → 保存 WAV 文件 → IncrementalSaver 增量写入
    │
11. Frontend: RecordingPostProcessingProvider
    ├── 等待最终转录完成
    ├── 保存转录到 SQLite (storageService.saveMeeting)
    ├── 导航到会议详情页
    │
12. 可选：自动生成摘要 (auto-summary)
    ├── 调用 LLM 生成摘要
    ├── 轮询摘要状态
    └── 展示结构化的 AI 摘要
```

### 11.2 摘要生成流程

```
1. 用户点击"生成摘要"
   │
2. Frontend: useSummaryGeneration()
   ├── 调用 api_process_transcript 命令
   │
3. Rust: summary::processor
   ├── 加载摘要模板（默认/用户自定义）
   ├── chunk_text()：按 token 数分块
   │
4. Rust: summary::llm_client
   ├── 根据配置选择 Provider:
   │   ├── Ollama (本地)    → ollama API
   │   ├── Claude           → Anthropic API
   │   ├── Groq             → Groq API
   │   ├── OpenAI           → OpenAI API
   │   ├── OpenRouter       → OpenRouter API
   │   ├── CustomOpenAI     → 自定义端点
   │   └── BuiltIn          → llama-helper sidecar
   │
5. 逐块调用 LLM 生成摘要
   ├── clean_llm_markdown_output()：清理输出
   ├── extract_meeting_name_from_markdown()：提取会议名
   │
6. 合并所有块的摘要
   ├── 去重、聚合
   ├── 保存到 summary_processes 表
   │
7. Frontend: 轮询 api_get_summary
   ├── 状态：processing → completed/failed
   ├── 渲染摘要（支持 Legacy/Markdown/BlockNote 三种格式）
```

### 11.3 音频导入流程

```
1. 用户拖放音频文件到窗口 / 点击导入按钮
   │
2. Frontend: ImportAudioDialog
   ├── 验证音频格式
   │
3. Tauri Command: validate_audio_file_command
   ├── 检查文件格式（通过 symphonia 解码器）
   │
4. Tauri Command: start_import_audio_command
   ├── 解码音频文件
   ├── 调用 Whisper/Parakeet 转录
   ├── 创建新会议记录
   ├── 保存转录到数据库
   │
5. 导航到新创建的会议详情页
```

---

## 12. 配置项说明

### Tauri 配置 (tauri.conf.json)

| 配置项 | 值 | 说明 |
|--------|-----|------|
| `productName` | meetily | 产品名 |
| `version` | 0.3.0 | 版本号 |
| `identifier` | com.meetily.ai | 应用标识符 |
| `frontendDist` | ../out | Next.js 构建输出目录 |
| `devUrl` | http://localhost:3118 | 开发服务器地址 |
| `windows[0].width/height` | 1100×700 | 窗口尺寸 |

### CSP 安全策略

```
default-src: 'self'
style-src: 'self' 'unsafe-inline'
img-src: 'self' asset: https://asset.localhost data:
connect-src: 'self' http://localhost:11434 http://localhost:5167 http://localhost:8178 https://api.ollama.ai
```

### Tauri 插件

| 插件 | 用途 |
|------|------|
| `tauri-plugin-fs` | 文件系统访问 |
| `tauri-plugin-store` | 持久化键值存储 |
| `tauri-plugin-dialog` | 原生对话框 |
| `tauri-plugin-notification` | 系统通知 |
| `tauri-plugin-updater` | 应用自动更新 |
| `tauri-plugin-process` | 进程管理 |

### Rust Feature Flags (GPU 加速)

| Feature | 平台 | 说明 |
|---------|------|------|
| `metal` | macOS | Apple Metal GPU |
| `coreml` | macOS | Apple CoreML |
| `cuda` | Win/Linux | NVIDIA CUDA |
| `vulkan` | Win/Linux | AMD/Intel Vulkan |
| `hipblas` | Linux | AMD ROCm |
| `openblas` | Win/Linux | CPU 优化 BLAS |

### 摘要模板结构

每个模板 JSON 文件定义：

```json
{
  "name": "模板名称",
  "description": "模板描述",
  "sections": [
    {
      "title": "区域标题",
      "prompt": "区域生成提示词",
      "type": "block_type"
    }
  ]
}
```

### 环境变量 (.env.example)

| 变量 | 说明 |
|------|------|
| `OLLAMA_ENDPOINT` | Ollama API 端点（默认 http://localhost:11434） |
| 各种 `*_API_KEY` | 各 AI 提供者的 API Key（在数据库 settings 表中存储） |

---

## 附录：关键文件快速索引

### Rust 后端核心文件

| 文件 | 职责 |
|------|------|
| [lib.rs](file:///e:/ForkedRepo/meetily-forked/frontend/src-tauri/src/lib.rs) | Tauri Builder，应用组装入口，注册所有命令 |
| [main.rs](file:///e:/ForkedRepo/meetily-forked/frontend/src-tauri/src/main.rs) | Rust 入口 |
| [config.rs](file:///e:/ForkedRepo/meetily-forked/frontend/src-tauri/src/config.rs) | 全局配置常量（模型目录等） |
| [state.rs](file:///e:/ForkedRepo/meetily-forked/frontend/src-tauri/src/state.rs) | 应用状态 |
| [audio/recording_manager.rs](file:///e:/ForkedRepo/meetily-forked/frontend/src-tauri/src/audio/recording_manager.rs) | 录音管理器核心 |
| [audio/recording_commands.rs](file:///e:/ForkedRepo/meetily-forked/frontend/src-tauri/src/audio/recording_commands.rs) | 录音 Tauri 命令 |
| [audio/ffmpeg_mixer.rs](file:///e:/ForkedRepo/meetily-forked/frontend/src-tauri/src/audio/ffmpeg_mixer.rs) | FFmpeg 自适应混音器 |
| [whisper_engine/whisper_engine.rs](file:///e:/ForkedRepo/meetily-forked/frontend/src-tauri/src/whisper_engine/whisper_engine.rs) | Whisper 引擎核心 |
| [parakeet_engine/parakeet_engine.rs](file:///e:/ForkedRepo/meetily-forked/frontend/src-tauri/src/parakeet_engine/parakeet_engine.rs) | Parakeet 引擎核心 |
| [summary/llm_client.rs](file:///e:/ForkedRepo/meetily-forked/frontend/src-tauri/src/summary/llm_client.rs) | 统一 LLM 客户端 |
| [summary/processor.rs](file:///e:/ForkedRepo/meetily-forked/frontend/src-tauri/src/summary/processor.rs) | 文本分块与摘要生成 |
| [database/models.rs](file:///e:/ForkedRepo/meetily-forked/frontend/src-tauri/src/database/models.rs) | 数据模型定义 |
| [database/manager.rs](file:///e:/ForkedRepo/meetily-forked/frontend/src-tauri/src/database/manager.rs) | 数据库管理器 |

### 前端核心文件

| 文件 | 职责 |
|------|------|
| [app/layout.tsx](file:///e:/ForkedRepo/meetily-forked/frontend/src/app/layout.tsx) | 根布局，Provider 嵌套，引导/主应用切换 |
| [app/page.tsx](file:///e:/ForkedRepo/meetily-forked/frontend/src/app/page.tsx) | 主页：录音面板 + 实时转录 |
| [app/meeting-details/page.tsx](file:///e:/ForkedRepo/meetily-forked/frontend/src/app/meeting-details/page.tsx) | 会议详情页 |
| [contexts/RecordingStateContext.tsx](file:///e:/ForkedRepo/meetily-forked/frontend/src/contexts/RecordingStateContext.tsx) | 录音状态机 |
| [contexts/ConfigContext.tsx](file:///e:/ForkedRepo/meetily-forked/frontend/src/contexts/ConfigContext.tsx) | 全局配置管理 |
| [components/Sidebar/index.tsx](file:///e:/ForkedRepo/meetily-forked/frontend/src/components/Sidebar/index.tsx) | 侧边栏组件 |
| [types/index.ts](file:///e:/ForkedRepo/meetily-forked/frontend/src/types/index.ts) | TypeScript 类型定义 |

### Python 后端核心文件

| 文件 | 职责 |
|------|------|
| [backend/app/main.py](file:///e:/ForkedRepo/meetily-forked/backend/app/main.py) | FastAPI 应用入口 |
| [backend/app/db.py](file:///e:/ForkedRepo/meetily-forked/backend/app/db.py) | 异步 SQLite 数据库管理 |
| [backend/app/transcript_processor.py](file:///e:/ForkedRepo/meetily-forked/backend/app/transcript_processor.py) | 转录文本处理器 |

---

> **文档维护说明**：本文档基于 Meetily v0.3.0 代码库生成。随着项目的演进，建议在重大版本更新时同步更新此文档。如需更新，重点关注 `lib.rs` 中新增的模块声明、`tauri.conf.json` 中的配置变更、以及数据库迁移文件的增补。