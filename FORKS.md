# 第三方件：fork 与部署规则

凡是**我们修改过**的第三方插件 / Agent / 组件，一律遵守以下规则，
避免「上游更新把我们的修复覆盖掉」：

1. 在 `FelixHomelab` 组织下建立 fork（仓库名尽量与上游一致），**修改只发生在 fork**；
2. 部署从 fork 的**固定提交**安装：`github:FelixHomelab/<repo>#<sha>`；
   monorepo 里的子包用 `&path:/<子目录>`（pnpm 支持）；
3. 上游发版包含我们的修复后，撤掉 fork 安装、回到上游包，并在下表更新；
4. 每个 fork 里写 `felix/PATCHES.md`：改了什么、为什么、如何与上游同步（rebase 步骤）；
5. **没有修改**的第三方件不建 fork，跟随版本号升级即可。

> 禁止的做法：安装上游包后在构建期/运行期改包内文件（历史遗留已全部清除，
> 两个构建期补丁脚本已删除）。

## 现状清单

| 组件 | 上游 | fork / 分支 | 我们的改动 | 部署来源 |
| --- | --- | --- | --- | --- |
| opencode2dsh | FishBottle7/opencode2dsh | `FelixHomelab/opencode2dsh` 分支 `felix/configforms` | 客户端 `settingsScope` → `configForms`（DSH 0.1.7 移除该服务，界面卡 `pending` 起不来；上游 issue #20/#24，修复 PR #23 未发版） | `github:...#d5e866ae` + `path:/packages/plugin` |
| dsh-my-guardian | baosfeng/my-dsh-plugins | `FelixHomelab/my-dsh-plugins`（**未改代码**） | 无：npm 0.4.4 缺「宿主提供 peer 不误报缺失」修复，上游 `main` 已修但未发版，直接固定 main 提交 | `github:...#1bca78ed` + `path:/plugins/dsh-my-guardian` |
| dsh-ego-browser | Fisfzy/dsh-ego-browser | `FelixHomelab/dsh-ego-browser` 分支 `felix/main` | 暂未改代码：为「优化尝试」预建（观察窗帧回传/Chromium 进程树/冷启动等，见分支内 `felix/PATCHES.md`） | `github:FelixHomelab/dsh-ego-browser#e20e90a1` |
| DeepSeek Harness | deepseek-ai/deepseek-harness | `FelixHomelab/deepseek-harness` 分支 `felix/patches`（仅跟踪） | 两处客户端门控补丁：远程（非 localhost）允许编辑设置、允许绑定 `0.0.0.0`（门控环境变量） | **例外（见下）** |

### DSH 主程序的例外

DSH 是 monorepo，发布的 `@deepseek-ai/dsh` 只是薄 CLI，真正的代码在大量
`@deepseek-ai/*` 子包里；从 fork 部署需要重建并发布整套子包（既不可行也不
可持续——上游每天在推）。因此：

- fork 仅用于**跟踪**补丁：`felix/patches` 分支包含补丁脚本与说明；
- 部署仍是「npm 固定版本 + 本仓库构建期补丁脚本」；
- 缓解措施：补丁目标找不到时**构建直接失败**（强制人工复核），DSH 版本固定
  在 `.env`，升级必须显式改版本号并重跑构建。

## 升级流程（fork 件）

1. 在 fork 上 rebase 上游 → 重新构建产物 → 提交（每个 fork 的 `felix/PATCHES.md` 有命令）；
2. 把新提交 SHA 写进 `.env` 的 `OPENCODE2DSH_REF` / `DSHGUARDIAN_REF`（或 Containerfile 默认值）；
3. `make agent-build-dsh`（`PROFILE_REV` 有变化时，老数据卷会非破坏合并刷新 node_modules）；
4. 真实环境验证（启动页无插件报错、模型可用）后再推送。

## 未修改的第三方插件（不建 fork）

`dshmarket`、`dsh-cost-meter`、`dsh-better-sidebar`
（`dsh-dev-rules` 本身就是我们维护的上游仓库）。
