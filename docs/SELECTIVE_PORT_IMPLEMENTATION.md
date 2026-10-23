# Agent Note: Claudeix 选择性移植与可靠性改进实施开发文档

Status: proposed

编制日期：2026-10-23（Asia/Shanghai）
执行对象：Sol 6.1。本文为可实施方案，不表示代码已经修改或目标行为已经验收。
仓库：https://github.com/Luxciax/claudeix
审查分支：cloud-build/0.2.3
固定基线：cfdb8181e829669c4d5da29543ec3981466e7a78
默认 main：77ed38122b8798b7c4262402bc744c6fac5bbfe9（v0.1.2），不是本轮功能基线。

## Problem

本轮目标是吸收上游通用改进，同时修复 Claudeix 现有 Provider 管理的正确性风险。原方案的方向成立，但混合了已经存在的功能、需要加强的行为以及未实现的发布能力，且 Claude-only 描述与 v0.2.3 的实际双引擎架构不一致。

最高优先级根因：
1. 移动模型把来源 Provider 的凭据当成目标 Provider 的默认配置，可能导致跨地址发送错误凭据。
2. 配置加载失败和可编辑状态没有隔离；旧快照乐观回滚、无版本写入可造成覆盖。
3. 明文凭据随整个 catalog 返回前端、回填输入框，模型发现也由前端传入 Secret。
4. 旧 Release 工作流还引用 upstream，而运行中的 updater 只有检查版本能力，没有安装闭环。
5. 旧 docs/ARCHITECTURE.md、IMPLEMENTATION_PLAN.md、ROADMAP.md 保留一期 Claude-only 叙述，不能当成当前代码事实。

本轮只编写文档，未修改源码、未读取用户真实凭据、未操作用户 Session、未 push、未运行新 CI 或发布。

## Proposal

### 1. 执行边界与交付物

保留 Claude Code 和当前已接入的新 Pi Engine。以 Claude Provider 和 Windows 可靠性为主线；共享基础设施可服务双引擎，但不得改变 Pi 现有配置协议和运行行为。

核心交付：B0～B6。B7 为可选后续项，不阻塞核心交付，也不与核心提交混合。每批以用户可观察行为闭环为终点，不要求逐文件形成提交。

执行者首先核对实际 checkout：
- git status --short、当前分支、HEAD、remote；保留所有无关及未提交修改。
- 若 HEAD 比固定基线更新，只核对本轮涉及文件的增量，不能 reset 到本文 SHA。
- 若只有 main，从已核实的开发分支取得对应源码；不得在旧 main 上重新造一套 Provider 系统。
- 阅读实际 checkout 的 AGENTS.md；CLAUDE.md 当前只是 @AGENTS.md。
- 既有云构建成功不证明新改动正确，不证明 EXE 用户交互通过。

建议把本文作为 docs 下的新专项计划（例如 docs/SELECTIVE_PORT_IMPLEMENTATION.md），并在旧 IMPLEMENTATION_PLAN.md、ROADMAP.md 增加清晰的适用版本与专项入口。不要把旧历史全文改写成另一项决定。当前交付的外部文档是交接稿，尚未写入仓库。

### 2. 不允许触碰的范围

- 不 merge upstream/main，不整段 cherry-pick 版本区间。
- 不删除或停用 src-tauri/src/engines/pi.rs、新 Pi runtime、EngineRegistry 或现有 Agent 选择功能。
- 不重新注册旧 pi_bridge.rs、claude_bridge.rs、WSL 项目运行时、Pet、Android、Pi Remote Launcher。
- 不迁移 Claude production editor 到 generic Mutation；不改 Session 身份算法、原始 JSONL 结构或数据库表结构，除非 B5 的具体回归证明必须做最小修复。
- 不重写主界面，不更换 UI 库、Next 静态导出方式、主题 token 或现有运行时事件协议。
- 不替换用户 Claude 登录配置；不迁走全局 Claude settings 中用户自行配置的凭据。
- 不自动清理用户备份、Vault 条目、Session、回收站或旧安装数据。
- 不重命名应用 identifier、修改现有安装升级身份或版本号以“顺便统一命名”。
- 不新增 SSH、远端凭据同步、云账户、大型基础设施、常驻服务。
- 不因目录名含 pi 就一律迁移或删除：model-scope.ts 是共享纯算法，pi_settings.rs 在当前 lib.rs 中是活跃模块。
- 外部写入、push、触发 CI、上传和发布仍遵守当前会话的预览与确认合同；这份开发文档不预授权这些动作。

### 3. 源码事实、根因和证据级别

下面链接均固定到审查 SHA；“静态确认”表示代码可见，不等于用真实凭据/生产数据复现。

| ID | 事实与触发条件 | 根因及实施结论 | 证据 |
|---|---|---|---|
| F01 | A 模型移动到 B，B 没有 apiKey 时使用 A 的 apiKey；来源空 Provider 被自动删除 | updateModel 将模型移动与 Provider 配置复制混在一起；拆开语义 | src/lib/claude/models.ts updateModel，约 L195 |
| F02 | 后端配置解析错误通过 read 命令失败返回；前端只设置 lastError，但修改多只判断 parseError | loaded 与可写混用；加载失败后可能拿空/旧 data 保存 | 同文件 load、各 mutation；claude/models.rs claude_models_read |
| F03 | save 先乐观 set(next)，失败恢复 previous；多个请求可互相回滚 | 前端无写队列/操作身份；后端无 expectedRevision | 同文件 save；claude/models.rs claude_models_write |
| F04 | 临时路径仅含 PID，发现已有文件直接删除 | 同进程多个操作共享临时文件；不能把“存在”直接视为 stale | claude/models.rs write |
| F05 | Provider 编辑读取 apiKey 填入 password input | UI 隐藏显示不等于 write-only；read DTO 本身含 Secret | src/app/models/page.tsx ProviderDialog，约 L674 |
| F06 | fetch 对所有支持的两种认证同时发送 x-api-key 和 Bearer，仅解析一次响应 | discovery API 丢失 provider type，未表示分页/完整性 | claude/models.rs claude_models_fetch；tauri-api.ts L112 起 |
| F07 | handleSaveProvider 已 await 保存后关闭 | 原方案“先关再写”不是当前事实；缺的是 Dialog pending/error/捕获拒绝 | models/page.tsx L112、ProviderDialog submit L685 |
| F08 | 当前 fetch 筛选新增项，addModels 追加，并非全量替换 | 保留追加默认，仅扩展预览与完整性合同 | models/page.tsx L160 起 |
| F09 | Runtime 已接 providerId，并清理一组宿主认证环境变量 | 复用现有接线；补测试和缺项，而非重建 runtime | ClaudeAgentPanel L245、runtime-store、RuntimeStartRequest、runtime_registry L600、configure_runtime_provider |
| F10 | 清理列表漏了 AWS_BEARER_TOKEN_BEDROCK；明确的 Bedrock Provider 无自身 token 时可能继承宿主 token | 分清应用托管 token 与原生身份链，补显式认证策略 | claude/models.rs configure_runtime_provider |
| F11 | Claude Trash 已覆盖 stale preview、删除中断、交换恢复；EngineTrash 有往返/交换测试 | 先映射缺口，再移植行为测试；不可覆盖成熟实现 | trash/service.rs 测试 L1041 起；trash/engine_service.rs L991 起 |
| F12 | Windows cloud-build 已包含 tsc、Rust tests、EXE/NSIS/MSI 构建 | 复用既有工作流，不另起相同流水线 | .github/workflows/windows-cloud-build.yml |
| F13 | release.yml 下载与 manifest repo 仍指 upstream；updater repo 已正确但 apply 返回未实现；createUpdaterArtifacts=false | 构建、发布和更新三个阶段未闭环 | release.yml L136；updater.rs；tauri.conf.json |
| F14 | Claude runtime_registry 与活跃 pi_settings 仍手写 flags；新 Pi runtime、updater、terminal helper 已用 hide_console | 只处理活跃调用，保留交互进程策略 | 对应文件的 creation_flags/hide_console |

事实索引：
- [当前项目入口](https://github.com/Luxciax/claudeix/blob/cfdb8181e829669c4d5da29543ec3981466e7a78/AGENTS.md)
- [模型 store](https://github.com/Luxciax/claudeix/blob/cfdb8181e829669c4d5da29543ec3981466e7a78/src/lib/claude/models.ts)
- [模型后端](https://github.com/Luxciax/claudeix/blob/cfdb8181e829669c4d5da29543ec3981466e7a78/src-tauri/src/claude/models.rs)
- [实际模型页面](https://github.com/Luxciax/claudeix/blob/cfdb8181e829669c4d5da29543ec3981466e7a78/src/app/models/page.tsx)
- [已成功的基线 CI](https://github.com/Luxciax/claudeix/actions/runs/39959609187)

注意 src/features/pi-shell/ClaudeModelsPage.tsx 仍在树中，但当前 /models 页面直接使用 useClaudeModels；不得因文件名推定旧页面是活跃入口。实施者按 import/call graph 判断其他残留入口。

### 4. 文件改动地图

| 批次 | 允许修改/新增 | 不要扩散 |
|---|---|---|
| B0 | 专项文档、docs/upstream-porting.md；旧计划只加适用性说明 | 不重写项目全局规则，不执行发布 |
| B1 | src/lib/claude/models.ts；新纯函数/测试；src-tauri/src/claude/models.rs；src/lib/tauri-api.ts；models page/Picker/runtime model-store 必要接线；lib.rs 管理状态注册（若需要） | 不修改 Session editor，不改 Pi models 配置 |
| B2 | src/components/ConfirmDialog.tsx、src/hooks/useModalFocus.ts（或复用已有等价 primitive）；models/page.tsx；必要 i18n | 不一口气替换所有编辑/恢复弹窗，不丢弃危险操作的额外确认 |
| B3 | src-tauri/src/credentials/{mod.rs,windows.rs}；claude/models.rs；lib.rs；Cargo.toml/lock 的必要 Windows feature；tauri-api.ts；Claude model store/page；runtime provider 接线；迁移测试 | 不建 Secret 管理服务，不让前端调用任意 credential target，不改 Pi Secret 生命周期 |
| B4 | process_utils.rs；claude/runtime_registry.rs；pi_settings.rs；经活跃调用图确认的其他后台命令；测试 | 不机械修改 PTY/终端 flags，不清理旧未注册模块 |
| B5 | claude/scanner.rs、parser.rs、commands.rs 与现有两套 trash、mutation/editor 的测试及必要局部修复 | 无失败证据不改事务架构、不移植 upstream backend |
| B6 | .github/workflows/windows-cloud-build.yml、release.yml；generate_updater_manifest.py；updater.rs；tauri.conf.json；Cargo.toml/lock、lib.rs、更新 UI/权限配置中真正需要部分 | 不触发真实 Release，不更换签名密钥，不恢复 macOS/Linux 矩阵 |
| B7 | 经定位的 diff consumer 与通用纯解析 helper、TerminalDrawer 及 UI store | 不导入完整 upstream Terminal 或 Pi tool pipeline |

### 5. B0 — 基线、上游证据与文档校正

1. 记录实际 checkout SHA、工作区改动、现有命令、现有 CI。
2. 核对 upstream 仓库真实地址、tag 对应 SHA、候选提交及许可证。本次工具未成功读取 upstream，不能把 v0.15/v0.16/v0.18.1 与 987524c 写成已验证事实。
3. 上游不可读不阻塞本地已确认缺陷的 B1～B4；以 Claudeix-native 修复交付，不声称“原样移植通过”。
4. 建 docs/upstream-porting.md：候选行为、来源 SHA/文件、Claudeix 实现、Port/Adapt/Already Have/Skip、测试、剩余差异、最后审查 SHA。未经核实来源标为 Unverified。
5. 旧 architecture/implementation/roadmap 限定为一期历史语义；本轮以当前双引擎边界为准。仓库目前未发现 .agents/notes/adr 决策记录，不制造重复文档体系。

B0 完成标准：执行者能定位正确代码，不会删除新 Pi Engine，也不会根据旧 Release 流程误发布。

### 6. B1 — 模型正确性、持久化与状态隔离

#### 6.1 模型和 Provider 是两种操作

移动模型只接收 oldProviderId、oldModelId、newProviderId、model，不允许携带来源认证字段。目标 Provider 必须存在；新建 Provider 使用单独流程。默认拒绝目标重名模型，不隐式覆盖。移动后保留空来源 Provider。

同 Provider 修改模型名也要检查重名。Provider ID 本轮编辑已有项时保持不可变，不顺便支持重命名。修正已有页面是否允许更名而遗留旧条目的情况。

新选择使用结构化 (providerId, modelId)。旧 enabledModels 的 provider/id 字符串仅作兼容表示；不要用 split('/') 拆模型 ID。禁止通过字符串前缀误删另一个 Provider 的 enabled 引用。若现有 Provider ID 含 '/' 导致歧义，保留原配置并显示需要显式修复，不能自动猜测重写。新建 Provider ID 禁止 '/'；模型 ID 可以含 '/'。不能改变 Pi 的 shared model-scope 语义来修 Claude 专属问题。

#### 6.2 持久化合同

建议先保留整份 catalog 保存接口，以最少改动增加版本保护；不要为了此事引入通用 Session Mutation 框架。

~~~ts
type CatalogSnapshot = {
  catalog: ClaudeModelsCatalog;
  revision: string; // 后端计算的不透明内容版本；缺失文件有独立标记
  path: string;
};
type SaveRequest = {
  expectedRevision: string;
  catalog: ClaudeModelsCatalog;
};
~~~

要求：
- read 成功才进入 ready；missing file 是正常初始态，读取失败/损坏不是空配置。
- 所有修改入口在非 ready、parse/load error、保存中拒绝提交；失败保留用户草稿。
- 对本应用同一 catalog 的写入串行化；采用稳定锁对象，不锁马上被替换的临时文件。优先已有可用平台锁，未存在则小范围实现，避免无理由大型依赖。
- 锁内读磁盘、校验 expectedRevision、校验候选、写独立临时文件、sync、提交前复查、原子替换；临时文件由本次操作唯一拥有，只清理自身临时物。
- revision 冲突返回结构化 Conflict，不自动 last-write-wins，不自动重试覆盖。
- 多实例使用同一锁约定；外部编辑器不配合锁时无法承诺通用文件系统 CAS。通过提交前复查缩短窗口，明确该能力边界，不能写“任意外部竞争绝不丢失”。
- 写成功返回该次提交的状态与 revision；不要混入另一次读到的版本冒充本次结果。
- 保留配置未知字段，或明确拒绝不支持的数据。当前前端 normalize 固定 version=1、后端反序列化再序列化会丢未知字段；必须测试 schema 演进，不能静默降级。
- 不在 B1 新增明文凭据备份。失败前旧文件保持，成功后仅发生预期字段差异。

#### 6.3 前端状态

加载状态与 mutation 状态分开：
- status: unloaded/loading/ready/loadError。
- activeMutation/requestId、lastError。
- loadGeneration 按实际 catalog/工作区作用域隔离；只有当前请求可以更新数据或结束 busy。
- 第一版直接禁用并发修改，不做复杂乐观回滚；后端仍必须防护，不能仅靠按钮 disabled。
- 保存成功再更新 store；保存失败不回退别人已经成功提交的快照。
- runtime/model-store 的工作区 load 也需代次保护，不能在晚返回时把用户已选 Provider 清为 null。
- 正在运行的进程继续使用启动时快照；删除 Provider/Model 不热改运行中环境。
- 选中项被删除/移动后显式显示失效并要求重新选择；不得悄悄回退到另一 Provider 的同名模型。

#### 6.4 模型同步合同

维持当前“只追加所选新增模型”的默认。分类：added、unchanged、metadataChanged、notReturned、localOnly。未返回不能标成“Provider 已删除”。

- metadata 更新默认不勾选，不覆盖本地显示名等定制字段。
- discovery 不支持元数据时，不凭空生成修改。
- 分页完成才可视为完整；不明确列表完整性的 gateway 永远不能触发自动删除。
- 空列表、401/403、超时、JSON 错误、截断或部分分页失败都不得产生删除。
- notReturned 可展示并让用户逐项主动移除；手动模型不参加自动移除。新拉取模型记录 provenance；旧记录无法判断来源时保守视为手动/未知。
- 预览绑定 catalog revision、providerId、Provider 地址/认证配置版本；提交时重新核对，失效则要求重新预览。
- 每次发现设合理超时、数量/响应大小上限和取消失效处理；不新增大规模重试机制。
- 应用所选变更一次提交，删除选中模型前列出影响，不提交半份 Diff。

### 7. B2 — Modal 基础设施与保存交互

当前 models page 底部有局部 Dialog；删除使用 Tauri confirm/browser window.confirm。先统一本页，其他高风险页面逐步接入，保留原有 preview fingerprint 与确认短语。

公共 ConfirmDialog 支持 title/message/detail/confirmLabel/cancelLabel/danger/confirmDisabled/onConfirm。onConfirm 明确 Promise<void> 或等价异步结果；本地 pending、错误、重复触发保护。

行为：
- 打开聚焦适当元素；危险操作默认取消。
- Tab/Shift+Tab 在最上层弹窗内循环；Escape 在未提交时取消。
- Enter 尊重焦点和表单语义；中文组合输入、textarea 不触发危险确认。
- 异步期间禁止重复提交和无反馈关闭；失败保留草稿/错误，成功后关闭。
- role=dialog、aria-modal、标题/说明关联；关闭恢复焦点，原触发元素消失时落到合理备用。
- 嵌套 modal 只由最上层处理快捷键；不要同时启用两个焦点陷阱。
- 可复用当前依赖提供的合格 primitive；如果使用其焦点管理，不再叠加第二个 useModalFocus 实现。

ProviderDialog/ModelDialog 必须 await/catch 保存，onSave 不再声明成 void 掩盖异步。保留现有父级 await 后关闭；不是重写页面。

### 8. B3 — Credential Vault 与运行时认证

#### 8.1 存储与 IPC 边界

Windows Credential Manager，应用内部 trait CredentialStore(get/set/delete)，生产实现与 fake 测试实现分开。优先现有 windows-sys 加必要 feature；避免引入全套跨平台 Secret 服务。非 Windows 明确返回 Unsupported，绝不退回明文。

catalog 新 schema version=2，Provider 仅存 credentialId 与认证方式。credentialId 是稳定不透明标识，不和可编辑名称绑定；底层 target 包含 Claudeix 命名空间。前端不传任意 Vault target，后端验证引用归属。

~~~ts
type CredentialUpdate =
  | { kind: "keep" }
  | { kind: "replace"; value: string }
  | { kind: "delete" };
type CredentialStatus = "saved" | "missing" | "unavailable" | "notRequired";
~~~

前端读取只得到 status，不返回 Secret 长度/原值。输入仅存在短生命周期草稿，成功或取消后清除，不进入持久化 store、日志、错误、分析事件。Secret Rust 类型不派生可能泄露值的 Debug，不记录整个请求/Command 环境。避免声称操作系统 Vault 能防御同用户恶意进程。

#### 8.2 分离配置提交与 Vault 写入

不宣称 JSON 与 Vault 有原子跨存储事务。采用新引用 + 原子配置提交：
1. 在配置协调锁内校验 revision，提前完成候选校验。
2. replace 先生成新引用、写 Vault 并在后端确认可读。
3. 原子提交新 catalog 引用；失败时旧引用仍有效。
4. 旧条目仅在确认不再被引用后处理，失败不把已提交操作误报为完全失败。
5. 无 Secret 的小型恢复记录标记本次新旧引用和阶段；中断重启后按 catalog 实际引用判定。不得在日志写 Secret；不得自动枚举/删除其他应用或用户凭据。
6. 运行时加载配置和读取 Secret 使用同一协调范围得到稳定启动快照，避免替换/删除与启动竞争。
7. 未明确授权的历史孤立条目只报告，不“顺手清理”。UI 显式删除 Provider/凭据要展示关联清理范围。

#### 8.3 旧明文迁移

- 自动检测 version=1 但不在普通 read 时静默改磁盘。返回无 Secret 的迁移状态，提供显式迁移入口。
- 用户确认后：旧 catalog 内各凭据写 Vault → 后端读取验证 → 一次性提交 version=2 且去除 apiKey。
- 任一步失败仍保留可恢复的原文件，不把空 key 当成成功；迁移恢复记录不含 Secret。
- 重试复用已验证的迁移结果或安全建立新引用，不能创建无界孤立条目。
- 不复制整份密钥库，不额外写明文备份；本任务不清理用户既有备份。
- 不扫描并搬走 ~/.claude/settings.json 的全局/项目认证；那是原生 Claude 的独立配置。
- 迁移后旧版本降级不受支持：旧版本可能覆盖新 schema。必须测试旧版本读失败路径并在发布说明明确禁止覆盖式降级；恢复方法是保留新格式或由用户重新配置，不能把 Secret 自动导出明文“回滚”。

#### 8.4 模型发现和 runtime

持久 Provider 的 discovery 改为 providerId + expectedRevision，后端解析 Vault。未保存的新 Provider 先保存再拉取，避免另造一套前端带 key 的网络接口。

认证策略区分：
- anthropic-messages：按 endpoint 协议发送 API key 及必要版本头。
- anthropic-auth-token：按对应协议发送 Bearer。
- Bedrock/Vertex/Foundry：保留已支持类型；凭据托管与原生身份链显式区分，不把所有类型强制当单个 API key。
- 无自定义 Provider：保留现有 Claude settings/原生认证语义。
- 显式自定义托管凭据缺失：启动失败，不回退到宿主账号。
- 原生身份链模式：允许该类型需要的 AWS/Google/Azure 环境；不要“安全清理”整个环境破坏原生登录。
- 补足应用管理的环境变量隔离，包括 AWS_BEARER_TOKEN_BEDROCK；不修改父进程全局 env。
- model 仍只通过 --model 参数；不把 Secret 放命令行。
- 错误只返回状态与脱敏上下文；不要原样把远端错误正文带回前端，测试响应回显 key 的情况。
- 明确 HTTP/HTTPS 与重定向策略；跨 origin 重定向不得携带认证，不能依赖自定义 x-api-key 自动剥离。

### 9. B4 — Windows 后台进程策略

已确认需要处理：claude/runtime_registry.rs 启动处、pi_settings.rs。
已使用 helper：新 Pi runtime、updater、terminal.rs 后台 helper。不要为满足“全仓没有 CREATE_NO_WINDOW”而动 legacy 文件。

helper 的 creation_flags 会设置 flags；先检查调用点是否还有其他进程 flags，若有需显式组合而不是覆盖。std::process::Command 与其他 Command 类型分别按实际 API 处理，不能用 unsafe 转换。

测试通过生产 hide_console helper 启动无副作用测试子进程/PowerShell -NoProfile，子进程查询 GetConsoleWindow 返回 NULL。保持退出码与 stdout/stderr 可读。仅 Windows 测试启用；真实桌面验证启动/模型相关后台操作无闪窗。这个测试不证明所有孙进程永远无窗，报告范围要准确。

终端默认关闭。交互终端和 PTY 不按普通后台进程机械套 flags；不改变终端输入、resize、关闭后进程策略。

### 10. B5 — Session 生命周期补缺口

先列出当前测试与需求对应表，再新增缺失项。现有 Claude 测试包括 previews_moves_and_restores_main_and_subagent_files、stale_delete_preview_does_not_move_changed_source、incomplete_delete_staging_rolls_moved_files_back_on_recovery、recovery_conflict_becomes_visible_trash_entry、exchange_restore_preserves_conflicting_current_version、purge_requires_exact_confirmation_and_preview_fingerprint；不要复制一套同义测试。

必须按当前真实路径分别验证：
- Claude scanner/parser/catalog + legacy TranscriptEditor + Claude Trash。
- Pi adapter + generic Mutation + EngineTrash。
不能只测试 generic 层便声称 Claude production path 覆盖。

新增目标：
- delete→rescan 不复活；restore→rescan 不重复。
- 相同路径重建不同身份、相同身份路径变化、旧 trash 与新 session 并存。
- 读取列表、详情、历史不改源文件字节/mtime、用户排序、tombstone。允许索引缓存更新，但不能用扫描时间替换会话更新时间。
- 运行中、外部进程可能写入、pending recovery 时，后端拒绝 edit/delete/restore。
- 预览后目标改变、交换恢复中断、移动/数据库失败，旧新内容都可找到且可恢复。
- 重复请求不能创建多个副作用；拒绝或返回同一已完成结果均可。
- Windows 文件占用、路径大小写及明确支持的路径边界。

保留既有“安全恢复 / 交换恢复 / 取消”。原方案的“另存副本”涉及内部 session identity 重建，移出本轮；没有完整 adapter 原生验证前不做。

只对红测证明的问题修根因；不扩展新引擎能力，不升级表结构来“统一一切”。

### 11. B6 — Windows 发布与更新闭环

分为 B6a 离线代码与产物校验，B6b 经授权的外部发布。没有发布授权时 B6a 可完成，B6b 明确待执行。

复用 windows-cloud-build：冻结依赖、tsc、cargo test --lib、Tauri Windows build、EXE/NSIS/MSI。补 Git diff/版本一致性和产物验证；不要创建另一条内容相同的工作流。

release.yml：
- Windows-only；真实 release 目标 Luxciax/claudeix。
- 显式限制 manual dispatch 必须选择有效 tag；校验 tag 与 package.json/Cargo.toml/tauri.conf.json 版本一致。
- 所有分发资产关联唯一 SHA；构建一次，后续提升同一批已验证产物。
- 验证通过才创建/上传 Draft；检查资产清单、大小、SHA256、manifest 与签名对应关系；发布为单独受控阶段。
- 只在需要写 Release 的 job 授 contents: write；构建默认只读。
- 删除活动发布逻辑中的 upstream repo；保留文档中明确标注的来源引用。
- generate_updater_manifest.py 收窄到真实 Windows updater 资产，不残留 Pi 文件名模式。

更新器：
- 采用 Tauri 2 支持的签名更新路径，替换“列 git tags + apply 返回未接通”的占位行为，不手写后台下载后任意覆盖 EXE。
- 区分 Tauri updater 签名、Windows 代码签名、SHA256 的用途；三者不能互相替代。
- 从可信发布渠道取 stable release，不把 beta tag 当 stable。若安装更新暂未接通，UI 不显示可执行的一键安装。
- createUpdaterArtifacts、公钥、插件注册/权限、endpoint 同步接入；实际密钥可用性未核验，禁止编造或自动重置生产密钥。
- NSIS、MSI 与 portable 分别定义支持策略。第一版可只为明确验证的安装形式启用自动更新，其余跳转真实下载页；不能对不支持的形式显示“已完成自动更新”。
- 裸 EXE 是否便携需验证 WebView2/运行依赖和数据目录；不因为上传文件叫 portable 就认为验收完成。
- 回滚以保留用户数据为前提；Vault schema v2 之后不承诺直接覆盖安装旧版可用。
- 首个可信自动更新链路需要已安装客户端包含正确验证公钥；旧版本没有 updater 时通过手动安装引导，不能假设它会自动升级。

### 12. B7 — 可选 Diff 与 Terminal

本次未完整审计活跃 diff consumer，不能直接从文件名猜入口。B7 开始前先定位 Claude tool event → inspector/diff → file navigation 调用链。

pathless diff：
- 纯解析器返回文件集合 {oldPath,newPath,kind}，处理多文件、rename、/dev/null、quoted path、Windows path。
- 明确 path 优先，但和 patch 冲突要报告，不静默跳到错误文件。
- tool args 作为最后可用来源，无法唯一确定则显示未知；导航再次校验工作区边界。
- 不改变 Pi tool output 协议，不把上游整个 tool renderer 拷进来。

Terminal：
- 只修具体 layout/resize/focus/open-close 问题；默认关闭。
- 先复现再修，不把“可选改善”演变为 UI Rewrite。
- 该批须独立证据、独立提交；若没有已确认问题，可保持 TODO。

## Alternatives considered

1. 整段 merge/cherry-pick upstream：能最快获得整套上游行为，但与双引擎边界、Claude 生产事务路径、Windows 目标不匹配；拒绝，选择按行为适配。
2. 只加前端 generation：改动小，可消除部分 UI 旧响应；无法阻止后端覆盖和跨 Provider 凭据复制；不足以解决本轮根因。
3. 把 Models 纳入 generic Session Mutation：可复用审计/恢复框架，但该框架服务原生 Session 与 adapter，配置的多存储语义不同；本轮选局部配置协调与窄恢复记录。
4. 维持乐观保存并做复杂回滚队列：交互即时，但 catalog 小、修改频率低，复杂度与收益不匹配；本轮选提交成功后更新 authoritative state。
5. 全平台 keyring：能提前覆盖非 Windows，但当前只有 Windows 交付目标；选 CredentialStore 抽象和 Windows 实现，未来按真实平台需求扩展。
6. 重做 Session 恢复并新增副本策略：功能更完整，但已有交换恢复能保留冲突双方；优先补回归，副本身份重建不在本轮。

## Acceptance criteria

### A. 回归用例清单（执行者逐项记录证据，不能只打总勾）

| ID | 用例 | 必须观察到的结果 |
|---|---|---|
| M01 | A/X 移到无凭据 B | B 不获得 A 凭据，A 空 Provider 保留，目标冲突明确处理 |
| M02 | A/X 与 B/X 独立选择、启用、删除 | 另一个 Provider 的项及运行请求不变 |
| M03 | 损坏 JSON、无读权限、unsupported version 后点击保存 | 原文件字节未变；UI 不进入 ready；可重试读取 |
| M04 | 保存 A 挂起，触发 B；或旧 load 晚到 | UI 拒绝并发写或安全串行；旧请求不能覆盖新状态/解除新 busy |
| M05 | 两个客户端持相同 revision 写入 | 至多一个成功；另一个 Conflict；无混合文件/临时文件互删 |
| M06 | 保存时外部编辑发生在复查前 | 返回冲突，外部内容保留；明确非配合外部编辑的剩余极短竞争窗口 |
| M07 | 配置有未知字段、未来 version、含斜杠模型 ID | 字段保留或明确拒绝；不静默丢失/降级/错误拆分 |
| M08 | 模型发现超时、401、空结果、分页中断 | 本地 catalog 不变，不能推导删除 |
| M09 | 预览后修改 Provider/凭据/catalog | 旧预览应用失败，要求重新计算 |
| M10 | 保存拒绝/失败与双击 | 草稿留存、错误可见、只产生一次有效提交 |
| M11 | 工作区加载未结束时用户选择 B/X | 旧响应不清除新选择；删除当前项不自动换 Provider |
| C01 | Vault set/get 失败及 migration 各阶段崩溃 | 唯一可用凭据不丢失，重试可恢复，日志无 Secret |
| C02 | keep/replace/delete | 空草稿不会误删；replace 成功更新引用；delete 影响范围明确 |
| C03 | read/list/UI reload | 响应与持久前端状态不含真实 Secret，编辑框不回填 |
| C04 | discovery + runtime A/X、B/X | 假服务/假子进程确认目标及身份各自正确；无凭据不偷用别家 |
| C05 | 旧宿主 token、跨 origin redirect、错误正文回显 key | 应用托管 token 不串线；不跨地址泄露；错误脱敏 |
| C06 | 默认/原生认证链模式 | 原有 Claude 原生登录、选定云提供商身份链仍可使用 |
| D01 | Modal Tab/Shift+Tab/Escape/Enter/中文输入 | 焦点和触发符合 B2；危险按钮不被 IME Enter 触发 |
| D02 | 异步失败、关闭、触发元素被删 | 弹窗保留反馈，不能重复执行，焦点恢复合理 |
| W01 | 生产 hide_console 启动测试进程 | GetConsoleWindow NULL，输出/退出码正常，桌面冒烟无可见闪窗 |
| S01 | delete/scan/restore/repeat | 不复活、不重复、不丢源身份 |
| S02 | read/history/scan | 源字节/mtime/排序/tombstone 不改变 |
| S03 | 运行中/外部占用/pending recovery | 后端拒绝破坏性操作，不能靠 UI 才挡住 |
| S04 | 冲突与恢复中断 | 新旧内容可恢复；安全恢复不覆盖；交换恢复保留当前版 |
| R01 | 同一 SHA 的 EXE/NSIS/MSI/manifest | 命名、版本、hash、签名及 URL 一致 |
| R02 | 篡改更新包或签名，stable 遇 prerelease | 拒绝篡改，稳定渠道不误选预发布 |
| R03 | 干净安装、支持路径升级、取消/失败 | 用户数据保留，失败反馈准确，支持矩阵与 UI 一致 |
| P01 | 双引擎基础冒烟 | Claude 与 Pi 的读取、继续、停止及原有回收站行为无退化 |

用假 Secret、隔离临时目录和 fake CredentialStore；不在测试中读取真实 ~/.claudeix、Vault 或会话。Windows 原生 Vault 集成测试使用明确测试 namespace，只清理本次生成的测试条目。

### B. 自动验证

每批：
- git diff --check。
- pnpm exec tsc --noEmit。
- 受影响 Rust 测试；Rust 变更执行 cargo check（src-tauri 下）。
- 前端 store、Diff 和失败/并发测试必须可重复执行。当前 package.json 没有通用 unit test 脚本，不能宣称现有 Playwright 残留就是可用测试基建。建议 B1 添加一个与现有 Node 20 兼容且锁版本的 Vitest 开发依赖，node environment 下 mock tauri-api；不用引入整套浏览器测试依赖。增加 pnpm test:unit，并接入现有 Windows CI。
- 云构建批次门槛复用现有工作流：冻结安装、typecheck、unit tests、cargo test --lib、Tauri build。push/dispatch 执行需满足外部写入授权；未授权则记录待云验收，不冒称已通过。

正式发布前完整 Rust 测试、前端生产构建、Windows 打包、签名产物检查和桌面冒烟。相关改动使证据失效才重跑；无关历史失败只记录，不扩修。

### C. 桌面手工验收

使用对应 SHA 的 EXE：浅色/深色、最小窗口/缩放、键盘、错误态、慢请求、重复点击；测试 Provider 页面、选择器、真实请求接线（经授权测试身份）、已有 Claude/Pi 会话继续与停止。Web 预览不能替代 Tauri invoke 与 Windows Vault 集成。

### D. 交付证据格式

每批记录：基线/结果 SHA、修改文件、用例 ID、命令、结果、失败或未验证项、恢复边界。区分静态通过、单测通过、云构建通过、桌面实测通过。未经证据不得写“全部完成”。

## Risks

- 上游具体版本/提交与许可证尚未核实：B0 补证，未核实前只做本地原生修复，不复制上游代码。
- 当前 docs 与代码存在版本漂移：入口声明版本，保留双引擎，不按一期文档裁剪。
- Vault 无法和 JSON 原子提交：需要少量无 Secret 恢复状态；不扩展为通用事务平台。
- Credential schema v2 有降级限制：发布说明必须明确，不能靠自动导出明文绕过。
- 跨进程锁只约束参与者：原子替换与 revision 不能等同任意外部编辑的系统级 CAS。
- 签名密钥/证书、真实安装升级行为未核实：属于发布验收前置，不允许伪造通过。
- Session 和 Windows 测试涉及 OS 行为：CI 成功不替代桌面实测；只在隔离 fixture 内制造失败。
- 收益是凭据与数据边界明确、恢复可验证；代价是 schema 迁移、额外少量测试基础设施、Vault 恢复与发布支持矩阵需要长期维护。

## Sol 6.1 执行入口

可直接使用以下任务说明：

> 读取本实施文档和实际仓库 AGENTS.md，以 cloud-build/0.2.3 的 cfdb8181e829669c4d5da29543ec3981466e7a78 为审查参照，先核对当前 HEAD 增量。按 B0→B1→B2→B3→B4→B5→B6 顺序实施，保留当前 Claude/Pi 双引擎及用户未提交修改。先修已有模型移动凭据串用、加载失败覆盖与并发持久化，再扩展 Diff、Modal 和 Vault。遵守逐批文件边界及用例验收；B7 暂不实施。不要重新设计 UI、迁移 Claude editor 或复制 upstream session backend。代码与离线验证持续完成；push、云构建触发、上传和发布按当前授权合同处理。最终按用例 ID 报告实际证据与未完成项，不把本方案文字当成验收结果。

## 文档自检与既有记录关系

仓库树中未发现活跃 .agents/notes/ 或 ADR 决策记录；已读取 AGENTS.md、CLAUDE.md、ARCHITECTURE.md、IMPLEMENTATION_PLAN.md、ROADMAP.md。后面三份与本轮部分重叠且包含历史一期描述；本文不覆盖它们，实施时增加版本限定与专项链接。

本文是 proposed 架构/实施记录。已有代码证据来自固定 GitHub SHA，基线 CI 已成功；未运行本轮测试、未验证本机 EXE、未验证上游提交。不新增形式化看板或永久监工流程。

