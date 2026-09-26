---
name: AWCC-G16-7630-Linux
kind: private
author: Felix
summary: Dell G16 7630 适配版 AWCC（灯效下发 + imgui 版本）。只在 G16 7630 上验证过，其他机型不保证可用。fork 自 tr1xem/AWCC（GPL-3.0）。
stack:
  - C++
weight: 4
---

Alienware Command Center 的非官方 Linux 替代品，fork 自 [tr1xem/AWCC](https://github.com/tr1xem/AWCC)（GPL-3.0）。

**名字即范围**：这个 fork 只针对 Dell G16 7630（Intel、ACPI 前缀 `AMWW`、单区 RGB 键盘）
做过验证，其他机型不保证能用——手上没有别的设备可测，行为差异请自行判断。通用问题反馈
上游，fork 相关的问题在这个仓库提；如果你的机型不是 G16 7630，建议直接用上游版本。

在这个 fork 上做的是**灯效下发**与一个 **imgui 版本**。

本项目与 Dell 没有任何关联，也不代表上游作者的立场。
