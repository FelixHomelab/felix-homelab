---
name: 天枢 Cosmic Hub
kind: private
author: Felix
summary: 被动、极简的「核心 + 接口」——模块通过 C ABI 把自身投影进核心。
stack:
  - Rust
weight: 3
---

天枢（Cosmic Hub）是一个很小的 Rust 库：它持有一批动态加载模块的**投影**。

**核心刻意做得小**，对模块具体做什么没有任何意见。模块可以是任何能导出 C ABI 的语言
——C、C++、Rust、Zig、Go（cgo）、Python（Cython）都行；IDE 组件、模拟器的 CPU 核、
浏览器的 JS 引擎，都可以是一个模块。核心管的是生命周期、线程模型与 ABI 稳定性。

C ABI 冻结在 `v1`，靠 Miri 与 cargo-fuzz 验证，自 v0.3.0 起没有破坏性变更。当前是
2026.06.26 LTS（v0.4.0），最低 Rust 版本 1.75。

许可证 MIT 或 Apache-2.0，任选。
