<p align="center">
  <img src="apps/desktop/src-tauri/icons/128x128@2x.png" alt="Vibe Bar Desktop" width="128">
</p>

<h1 align="center">Vibe Bar Desktop</h1>

<p align="center">
  <strong>同一个 Vibe Bar，跑在 Windows、Linux 和 macOS 上。</strong><br>
  <sub>AI 订阅配额、agent 会话、花费和 skills —— 托盘图标、小窗和 Workbench，与原生版共用同一份数据根。</sub>
</p>

<p align="center">
  <a href="https://github.com/AstroQore/vibe-bar-desktop/actions/workflows/ci.yml"><img src="https://github.com/AstroQore/vibe-bar-desktop/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <a href="https://github.com/AstroQore/vibe-bar-desktop/releases"><img src="https://img.shields.io/github/v/release/AstroQore/vibe-bar-desktop?display_name=tag&sort=semver&include_prereleases" alt="最新版本"></a>
  <img src="https://img.shields.io/badge/macOS%20%C2%B7%20Windows%20%C2%B7%20Linux-000000?logo=tauri&logoColor=white" alt="macOS、Windows、Linux">
  <img src="https://img.shields.io/badge/Tauri-2-24C8D8?logo=tauri&logoColor=white" alt="Tauri 2">
  <img src="https://img.shields.io/badge/Rust-stable-000000?logo=rust" alt="Rust">
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-AGPL--3.0--only-blue" alt="AGPL-3.0-only"></a>
</p>

<p align="center">
  <a href="https://github.com/AstroQore/vibe-bar-desktop/releases"><strong>下载</strong></a>
  · <a href="#它做什么">它做什么</a>
  · <a href="#workbench">Workbench</a>
  · <a href="#功能对齐">功能对齐</a>
  · <a href="#从源码构建">从源码构建</a>
  · <a href="#agents-mcp">Agents (MCP)</a>
  · <a href="README.md">English</a>
</p>

Vibe Bar Desktop 是 [Vibe Bar](https://github.com/AstroQore/vibe-bar) 的跨平台客户端。
Vibe Bar 是给整天跑 coding agent 的人准备的本地容量控制台：每个 AI 订阅还剩多少、撑不撑得到重置，
这台机器上的 agent 花了多少，有哪些会话和 skills 在这里；同样的问题，你的 agent 也能通过 MCP 来问。

它是一个产品、两套客户端实现。macOS 版是原生 AppKit 和 SwiftUI；Desktop 是 Tauri 2、Rust 和 React，
也跑在 Windows 和 Linux 上。两者都装的 Mac 上，它们读同一个 `~/.vibebar`——一套 provider 账号、一套设置、
没有第二份要维护；在从未装过原生版的机器上，Desktop 独立运行。

> **预览版（0.x）。** Desktop 正在向原生版看齐，到齐之前用自己的 `0.x` 版本号；
> [功能对齐表](#功能对齐)如实记录哪些已经移植、哪些还没有。对齐之后两端采用同一个
> `MAJOR.MINOR`，每个功能版本从两个仓库同时发布。

![Overview：顶部是花费和服务状态，下面每个 provider 一张配额卡，每根条都带着自己的预测](docs/screenshots/popover-overview-light.png)

<details>
<summary>深色外观下的同一个 Overview</summary>

![深色外观下的 Overview](docs/screenshots/popover-overview.png)

</details>

## 它做什么

| 你的问题 | Desktop 的回答 |
| --- | --- |
| **还剩多少，撑得到重置吗？** | 每张配额卡都带着重置倒计时和一份由本机记录的观测算出的个人预测：会用光还是有富余，并且在数据还不够时如实说"还在学习"。不会凭空制造——只有累积了足够的周期，预测才会出现。 |
| **哪些数字是新鲜的？** | Codex 和 Claude 用它们的 CLI 已经保存的凭据抓取，另外八个套餐用显式的 key 或各自的 CLI，其余 provider 从共享缓存读取并标注为 `shared data`。UI 从不夸大一个数字的新鲜度。 |
| **钱花到哪里去了？** | Workbench 把本机的 Codex、Claude Code 和 Gemini CLI 会话日志按本地价格表算成用量视图，按 harness、按模型、按计费公司分组；并为所有本地 agent 会话建立索引，支持全文搜索、阅读和一键 resume。 |
| **我的 agent 能用这些上下文吗？** | 同一个二进制加 `--mcp-stdio` 启动，就在 stdin/stdout 上用 JSON-RPC 回答配额、会话、状态、价格和花费的问题——不占端口、不开 socket、不碰凭据。 |

## 配额，每家公司一页

每家公司一页：它的套餐和窗口、重置历史条、用文字解释的预测，以及这家 provider 的公开服务状态。
Misc providers——各自有 dashboard 的 coding 和 token 套餐——和远程机器各有单独的页。

<table>
  <tr>
    <td width="50%"><img src="docs/screenshots/popover-openai-light.png" alt="OpenAI 页：ChatGPT Agentic 和 Codex Spark 窗口，带重置历史和预测"><br><sub><strong>OpenAI</strong> —— ChatGPT Agentic 和 Spark，每个窗口带自己的预测</sub></td>
    <td width="50%"><img src="docs/screenshots/popover-anthropic-light.png" alt="Anthropic 页：5 Hours、Weekly 和 Fable 窗口及其预测"><br><sub><strong>Anthropic</strong> —— 5 Hours、Weekly 和按模型的窗口</sub></td>
  </tr>
  <tr>
    <td width="50%"><img src="docs/screenshots/popover-google-light.png" alt="Google AI 页：Gemini Web 和 AntiGravity 配额"><br><sub><strong>Google AI</strong> —— Gemini Web 和 AntiGravity</sub></td>
    <td width="50%"><img src="docs/screenshots/popover-spacexai-light.png" alt="SpaceXAI 页：Grok、Cursor 和 Grok Bot 配额"><br><sub><strong>SpaceXAI</strong> —— Grok、Cursor 和 Grok Bot</sub></td>
  </tr>
  <tr>
    <td width="50%"><img src="docs/screenshots/popover-misc-light.png" alt="Misc providers 页：coding 和 token 套餐，各带配额和重置"><br><sub><strong>Misc providers</strong> —— 各自有 dashboard 的 coding 和 token 套餐</sub></td>
    <td width="50%"><img src="docs/screenshots/popover-machines-light.png" alt="Machines 页，说明端到端加密的远程用量"><br><sub><strong>Machines</strong> —— 端到端加密到本机的远程用量</sub></td>
  </tr>
</table>

<details>
<summary>深色外观下的公司页</summary>

<table>
  <tr>
    <td width="50%"><img src="docs/screenshots/popover-openai.png" alt="深色外观下的 OpenAI 页"></td>
    <td width="50%"><img src="docs/screenshots/popover-anthropic.png" alt="深色外观下的 Anthropic 页"></td>
  </tr>
  <tr>
    <td width="50%"><img src="docs/screenshots/popover-google.png" alt="深色外观下的 Google AI 页"></td>
    <td width="50%"><img src="docs/screenshots/popover-spacexai.png" alt="深色外观下的 SpaceXAI 页"></td>
  </tr>
</table>

</details>

## Mini 窗

一个随时可见的小窗，显示你选的字段，原生版的七种布局都有：regular、compact、ledger、tile、focus、rail，
以及 strip 的 roomy、两行和窄三种形态。窗口按所画内容自适应大小，并跟随共享的 `miniWindow` 设置——
在一端选好的布局，在另一端就是那个布局。

<table>
  <tr>
    <td width="33%"><img src="docs/screenshots/mini-regular-light.png" alt="regular 布局：配额环"><br><sub><strong>Regular</strong></sub></td>
    <td width="33%"><img src="docs/screenshots/mini-compact-light.png" alt="compact 布局"><br><sub><strong>Compact</strong></sub></td>
    <td width="33%"><img src="docs/screenshots/mini-ledger-light.png" alt="ledger 布局：每个字段一行"><br><sub><strong>Ledger</strong></sub></td>
  </tr>
  <tr>
    <td width="33%"><img src="docs/screenshots/mini-tile-light.png" alt="tile 布局"><br><sub><strong>Tile</strong></sub></td>
    <td width="33%"><img src="docs/screenshots/mini-focus-light.png" alt="focus 布局：一个字段，放大"><br><sub><strong>Focus</strong></sub></td>
    <td width="33%"><img src="docs/screenshots/mini-rail-light.png" alt="rail 布局：带刻度和标记的轨道"><br><sub><strong>Rail</strong></sub></td>
  </tr>
  <tr>
    <td width="33%"><img src="docs/screenshots/mini-strip-roomy-light.png" alt="strip 布局，roomy"><br><sub><strong>Strip</strong> —— roomy</sub></td>
    <td width="33%"><img src="docs/screenshots/mini-strip-twoLine-light.png" alt="strip 布局，两行"><br><sub><strong>Strip</strong> —— 两行</sub></td>
    <td width="33%"><img src="docs/screenshots/mini-strip-narrow-light.png" alt="strip 布局，窄"><br><sub><strong>Strip</strong> —— 窄</sub></td>
  </tr>
</table>

<details>
<summary>深色外观下的 mini 布局</summary>

<table>
  <tr>
    <td width="33%"><img src="docs/screenshots/mini-regular.png" alt="Regular，深色"></td>
    <td width="33%"><img src="docs/screenshots/mini-compact.png" alt="Compact，深色"></td>
    <td width="33%"><img src="docs/screenshots/mini-ledger.png" alt="Ledger，深色"></td>
  </tr>
  <tr>
    <td width="33%"><img src="docs/screenshots/mini-tile.png" alt="Tile，深色"></td>
    <td width="33%"><img src="docs/screenshots/mini-focus.png" alt="Focus，深色"></td>
    <td width="33%"><img src="docs/screenshots/mini-rail.png" alt="Rail，深色"></td>
  </tr>
  <tr>
    <td width="33%"><img src="docs/screenshots/mini-strip-roomy.png" alt="Strip roomy，深色"></td>
    <td width="33%"><img src="docs/screenshots/mini-strip-twoLine.png" alt="Strip 两行，深色"></td>
    <td width="33%"><img src="docs/screenshots/mini-strip-narrow.png" alt="Strip 窄，深色"></td>
  </tr>
</table>

</details>

## Workbench

一个更大的窗口，回答扫一眼答不了的问题。五个页面，一套[设计语言](docs/DESIGN.md)——
原生 Workbench 所用的那套 porcelain，色值、字号阶梯、圆角和 provider 强调色都是原样搬过来的，不是目测近似。

### Usage Stats

本机 agent 花了多少，用它们自己的会话日志按本地价格算出来：这一期的 hero 卡、趋势图、
按 harness / 模型 / 计费公司的分布环图，以及明细表。harness 筛选用的是共享的 pill；周期和分组各是一个控件。

![Usage Stats：hero 卡、趋势图、分布和明细](docs/screenshots/workbench-usage-light.png)

<details>
<summary>深色外观下的 Usage Stats</summary>

![深色外观下的 Usage Stats](docs/screenshots/workbench-usage.png)

</details>

### Sessions

所有本地 agent 会话，可全文搜索并选择搜索范围，按目录、公司、harness 和时间过滤。
transcript 在列表旁打开，工具调用和结果默认折叠，带分页的 transcript 内查找，以及对应 harness 的 resume 命令。
会话可以从这里删除——先删 sidecar，从不穿越符号链接，只在 session kit 认得的根目录之下。

![Sessions：筛选工具栏、会话列表和打开的 transcript](docs/screenshots/workbench-sessions-light.png)

<details>
<summary>深色外观下的 Sessions</summary>

![深色外观下的 Sessions](docs/screenshots/workbench-sessions.png)

</details>

### Resets

容量什么时候回来。Refill horizon 把接下来七天画成柱子，每个窗口一张卡带着自己的预测，
日历按天排出各周期，run-out risk 列表把预测说撑不到的窗口排在前面。

![Resets：refill horizon、周期卡、重置日历和 run-out risk](docs/screenshots/workbench-resets-light.png)

<details>
<summary>深色外观下的 Resets</summary>

![深色外观下的 Resets](docs/screenshots/workbench-resets.png)

</details>

### Skills

一个 skill 库在 `~/.agents/skills`，投影到六个 agent CLI。页面列出每个 skill 和它在每个 app 里的槽位——
已链接、已复制、外来或缺失——可以切换投影、从文件夹安装、登记已经在那里的 skill、
先快照再卸载、以及从备份恢复。原生同步引擎的规则原样执行：名字是一个安全的路径段，
每次写入都在允许的根之下，从不穿越符号链接删除任何东西。

![Skills：skill 库，每个 app 一个投影槽位](docs/screenshots/workbench-skills-light.png)

<details>
<summary>深色外观下的 Skills</summary>

![深色外观下的 Skills](docs/screenshots/workbench-skills.png)

</details>

## 设置，两端共享

设置从原生版用的同一个 `settings.json` 读取，本客户端展示的那些也写回去——加锁、只放回你改过的键、
保留这个构建不认识的每个键。当原生版覆盖了在这里做的选择，页面会说明。首次运行时一个简短的助手
带你走完订阅、浏览器 cookie、其他套餐、模型价格和开机启动，并把完成标记写进共享设置，原生版不会再问一遍。

<table>
  <tr>
    <td width="50%"><img src="docs/screenshots/settings-system-light.png" alt="设置 › System：刷新节奏、更新通道、开机启动、本客户端写什么"><br><sub><strong>System</strong> —— 刷新、更新、开机启动，以及本客户端往哪写什么</sub></td>
    <td width="50%"><img src="docs/screenshots/settings-menubar-light.png" alt="设置 › Menu bar：托盘显示的字段和标签"><br><sub><strong>Menu bar</strong> —— 托盘显示的字段，用你自己的叫法</sub></td>
  </tr>
  <tr>
    <td width="50%"><img src="docs/screenshots/settings-costData-light.png" alt="设置 › Cost data：隐私开关和扫描范围"><br><sub><strong>Cost data</strong> —— 一个开关停掉所有扫描</sub></td>
    <td width="50%"><img src="docs/screenshots/onboarding-welcome-light.png" alt="Workbench 之上的首次运行助手"><br><sub><strong>首次运行</strong> —— 七个小步骤，完成标记两端共享</sub></td>
  </tr>
</table>

<details>
<summary>深色外观下的设置</summary>

<table>
  <tr>
    <td width="50%"><img src="docs/screenshots/settings-system.png" alt="System 设置，深色"></td>
    <td width="50%"><img src="docs/screenshots/settings-menuBarHealth.png" alt="Menu Bar Health 设置，深色"></td>
  </tr>
</table>

</details>

## Agents (MCP)

用 `--mcp-stdio` 启动 Desktop 二进制，它就在 stdin/stdout 上用 JSON-RPC 提供 `quota.get`、
`sessions.list`、`sessions.search`、`status.get`、`pricing.effective` 和 `cost.snapshot`。
这个模式从不刷新 provider、不扫描用量、不写配置、不连接原生版；它用上一次运行记录下的数据作答。
会话调用接受原生的 provider 和 harness 过滤；列表支持 RFC 3339 的 `since`、`offset` 和有上限的 `limit`。
家目录里的 Unix socket 属于原生版。

## Desktop 读什么，以及它写的五样东西

Desktop **不依赖原生版的任何部分**：不依赖它的进程、它的 MCP socket、它的二进制。
它显示的一切都读自机器上已有的文件——各 CLI 自己的凭据和会话日志，以及共享的 `~/.vibebar` 根。

| 面 | 配额与状态 | 花费与活动 |
| --- | --- | --- |
| ChatGPT / Codex | Codex 订阅窗口、OpenAI 状态 | `~/.codex/sessions/**/*.jsonl` |
| Claude Code | 5 Hours、Weekly、按模型的 weekly、Anthropic 状态 | `~/.claude/projects/**/*.jsonl` |
| Gemini + AntiGravity | 读自共享缓存 | Gemini CLI 会话日志 |
| Grok + Cursor | 读自共享缓存；Cursor 状态 | —— |
| coding 和 token 套餐 | Alibaba、Copilot、Z.ai、MiniMax、Kilo、Kiro、OpenRouter、Warp 用显式 key 或各自 CLI；其余读自共享缓存 | —— |

Vibe Bar 的数据属于人，不属于某一个客户端；而第二个客户端往第一个客户端拥有的存储里写，正是数据丢失的方式。
所以 Desktop 只往共享根写恰好五样东西，每样都经由一个有文档的写入者、按原生版的规则：

1. **`settings.json`** —— 加 advisory 锁，合并时只放回本客户端改过的键、保留它不认识的每个键
   （[契约](docs/contracts/settings-write-v1.md)）。
2. **配额缓存** —— 它自己抓到的新观测，用原生版读取的文件布局。
3. **Control Center 允许列表修复** —— 与原生版在 macOS 26 藏起菜单栏图标时运行的同一个脚本。
4. **整个会话的删除** —— 通过 session kit 的删除器，只在你要求时、只在它认得的根之下、从不穿越符号链接。
5. **skill 库** —— `~/.agents/skills`、受管的各 app 目录、注册表及其备份，只经由 skills service。

其余一切——会话索引、用量账本、花费历史——在这里只读，由原生版写入，直到它们也有了同样规则的写入者。
读不了或版本未知的存储降级为"不可用"并给出解释；Desktop 从不修复、迁移或重建另一个客户端的数据。
完整理由见 [docs/SHARED-STORAGE.md](docs/SHARED-STORAGE.md)，规则见 [AGENTS.md](AGENTS.md)。

## 关于截图

本页每张图都是这个 app 真实的 UI：用 Tauri 窗口加载的同一份 React 代码，在浏览器里渲染在平底色之上，两种外观各一张。
数据是 app 内置的夹具——数字的形状来自一位维护者的真实用量，而每个账号、路径、机器和会话都被替换或专为此写就——
所以这里没有任何东西能指向某个人，也没有任何刷新离开过这台机器。
[`apps/desktop/scripts/capture-screenshots.mjs`](apps/desktop/scripts/capture-screenshots.mjs)
生成整套图（`pnpm screenshots`），所以画廊是重新生成的，不是修出来的。原生版的截图来自它自己的 demo 模式；
两套画廊是同一份夹具透过两个客户端看到的样子。

## 功能对齐

一个产品，两个客户端。约束规则是 **minor** 版本号：相同的 `MAJOR.MINOR` 意味着相同的功能。
patch 版本可以自由分叉——每个客户端按自己的节奏修自己的 bug——build 号永远各自独立。

只有两样东西豁免于对齐：

- **Bug 修复。**
- **在另一个平台上完全没有对应物的功能。** 需要不同的实现不算豁免：Keychain 变成 DPAPI 或 libsecret，
  Sparkle 变成 Tauri updater，`SMAppService` 变成各平台的自启动。那是同一个功能，只是造法不同。

**这张表只列两端有差异的地方。** 不在表里的都已对齐——配额层级、带你自己标签的托盘字段、会话搜索和 transcript、
会话删除、mini 窗的各布局、重置日历、首次运行助手、应用内更新、开机启动。任何一端的新功能都必须出现在这里，直到两端都有。

**在那之前。** Desktop 是 `0.x`，这份契约尚未生效：原生版自由发布功能 minor，Desktop 逐项关闭下表。
当 Desktop 追平当日原生版的 minor 时，两端一起发布下一个 minor 作为第一个联合版本——从那时起，
任何一端都不再发布另一端没有的功能 minor。

图例：● 完整 · ◐ 部分 · ○ 尚无 · —— 豁免

| 功能 | macOS 原生 | Desktop | 说明 |
| --- | :---: | :---: | --- |
| **配额** |
| 实时抓取的 provider | ● 25 | ◐ 10 | Codex、Claude、Alibaba、Copilot、Z.ai、MiniMax、Kilo、Kiro、OpenRouter、Warp。其余读自共享缓存并如此标注 |
| 浏览器 cookie 类 provider | ● | ○ | Gemini、Grok、Cursor 和 cookie 槽位类套餐需要 cookie 读取器；Windows 禁止读第三方 cookie，那里会是显式导入 |
| 观测与预测历史 | ● | ◐ | Desktop 记录观测并画出重置历史条；带刷选的配额历史图尚未移植 |
| 服务状态来源 | ● 5 | ● 4 | OpenAI、Anthropic、Google、Cursor；xAI 的页面靠抓取，尚未移植 |
| **菜单栏 / 托盘** |
| 富文本和两行标题 | ● | —— | Windows 和 Linux 的托盘根本没有标题，只有图标；macOS 托盘显示一行 |
| 带样式作用域的字段编辑器 | ● | ○ | 字段和标签读自共享设置，在 Menu bar 页编辑，没有样式作用域 |
| 合并同组窗口 | ● | ○ | 原生版可以把一个组的 5 小时和周窗口折成菜单栏里的一条：组名只出现一次，百分比共用它，各自保留自己的颜色。分组规则在 `VibeBarCore` 里，所以这是一份要对齐的契约，不是照着样子画 |
| 菜单栏拼装器 | ● | ○ | 原生版正在改成可自由排列的菜单栏：先选模板，再往里放 logo、任意文字和任意可用配额作为元素，每个元素可自定颜色或跟随某个配额的预测/实际色，同时保留现有固定布局作为默认模式 |
| Control Center 允许列表看门狗 | ● | ◐ | Desktop 运行同一个修复脚本；发现图标消失的看门狗只在原生版 |
| **主窗口** |
| provider 详情页 | ● 4 | ◐ | 每家公司一页，有配额、预测解释、重置历史和状态；原生版还带该 provider 的花费卡和历史图 |
| 可排列的模块瀑布流 | ● 11 | ◐ | Overview 按共享的顺序画出各模块；排列它们只在原生版 |
| 带预设的布局编辑器 | ● | ○ | |
| **Mini 窗** |
| 多个独立窗口 | ● | ○ | 一个 mini 窗，七种布局 |
| 半透明表面 | ● Liquid Glass | ◐ | popover 和 mini 窗在所有平台上都是透明的；macOS 上它们和主窗还用了真实的 `NSVisualEffect` 材质（sidebar 与 popover），有意用平台自己的东西而不是复刻 Liquid Glass。其他平台是透明但背后没有模糊，这是平台缺口而非设计 |
| **Workbench** |
| 重置历史对比 | ● | ○ | 原生版的跨配额卡片：按公司 → SubProvider → bucket 分组、两级行标签，Cycles / Time 轴切换存在共享的 `resetHistoryCompareAxis` 里，4 / 8 / 12 / All 选择器的默认值跟随卡片宽度，柱子画的是每次重置时剩下的量 |
| Skills：安装、导入、发现、备份 | ● | ◐ | 从文件夹安装、导入、投影、卸载和备份都在；仓库安装、发现和 harness 激活补丁暂留原生版 |
| 会话交接到终端 | ● | ◐ | Desktop 复制 resume 命令；原生版直接打开 Terminal 执行 |
| **花费与用量** |
| 本地用量扫描 | ● 7 个 harness | ◐ 3 | Codex、Claude Code、Gemini CLI。只计有本地扫描器的 harness：Cursor 的用量来自 dashboard 事件，Grok Bot 根本没有用量来源，所以两端都不算本地扫描 |
| 逐请求账本、多源价格、历史 | ● | ○ | Desktop 在 `client/desktop/` 下保存一份定价后的聚合和一张静态价格表；不写共享账本或历史 |
| **设置** |
| 可写 | ● | ◐ | Desktop 自己的设置页展示的那些键，经跨客户端写入契约——白名单 `shared::settings_writer::WRITABLE_KEYS` 就是边界。provider 凭据和布局编辑器不在其中 |
| provider 凭据面板 | ● 25 | ○ | API-key 适配器读进程环境变量；不持久化任何东西 |
| **平台** |
| 多语言 | ◐ | ○ | 两端将共读一份文案目录 [`AstroQore/vibe-bar-i18n`](https://github.com/AstroQore/vibe-bar-i18n)——原生版走 Swift 包，这里走 npm 包，首个语言是简体中文。目前两端都还没接入 |
| MCP 工具 | ● 12 | ◐ 6 | stdio 上的只读子集；Unix socket 属于原生版 |
| 远程探针同步 | ● | ○ | Machines 页解释了模型；还没有 relay 客户端 |
| App Sandbox | ○ 有意为之 | ○ 暂时 | 两端都不带沙箱发布。原生版**不能**：读浏览器 cookie、探测 AntiGravity 和驱动 Terminal 在沙箱里都被拦。Desktop 目前没有理由，而且会失去同样的 cookie 读取 |
| Windows 和 Linux | —— | ◐ | core crate 在每个 PR 上于三个平台测试，凭据和扫描路径可移植；GUI 只在 macOS 上做过端到端验证。两者都是发布目标——见 [docs/RELEASE.md](docs/RELEASE.md) |

[HANDOVER.md](HANDOVER.md) 是从这里到对齐的地图：原生版有而 Desktop 没有的东西、按什么顺序补齐，
以及这个预览版一路上在自己身上发现的 bug。

## 设计语言

Workbench、popover 和 mini 窗都出自一份规范 [docs/DESIGN.md](docs/DESIGN.md)：
原生 Workbench 的 porcelain——窗口和侧栏的底色、0.5px 的发丝线、强调色、各 provider 的颜色、字号阶梯、
圆角和唯一的 26px 控件高度——从 Swift 源码里搬出来而不是目测，共用的部件（pill、capsule、分段控件、代码块、开关）
在 [`porcelain.css`](apps/desktop/src/workbench/porcelain.css) 里只实现一次。需要新部件的页面先扩展规范。

## 从源码构建

需要 Rust（stable）、Node 22.13+、pnpm，以及你平台上的
[Tauri 前置条件](https://tauri.app/start/prerequisites/)。

```sh
cd apps/desktop && pnpm install
pnpm tauri dev      # 运行
pnpm tauri build    # 打包
```

验证，与 CI 相同的四步：

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cd apps/desktop && pnpm typecheck && pnpm test && pnpm build
```

用 `VIBEBAR_DEMO_HOME=<dir>` 把 app 指向一个合成的数据根——与原生版 demo 模式用的是同一个环境变量。
demo 模式下 Desktop 不发网络请求、不读凭据，并拒绝任何会离开该目录的写入。

不启动 GUI 也能做的两件事：

```sh
cargo run -p vibebar-desktop-core --example inspect -- <data-root>   # Desktop 从一个数据根能看到什么
cd apps/desktop && pnpm screenshots                                  # 重新生成 docs/screenshots/
```

### 更新

发布由 [`release.yml`](.github/workflows/release.yml) 从 tag 构建——Main 通道 `vX.Y.Z`，Dev 通道 `vX.Y.Z-dev.N`——
updater feed 在 `updates` 分支，每个通道一条。app 每天检查一次，从共享的 `updateChannel` 设置读取通道，
在托盘里提供更新。完整流水线见 [docs/RELEASE.md](docs/RELEASE.md)。

## 目录

```
crates/vibebar-desktop-core/   与平台无关的核心：共享数据的读取器和写入者、
                               provider 适配器、刷新调度、用量扫描器、skills
                               service、MCP server。无 Tauri、无 GUI，三个平台
                               都有测试。
apps/desktop/                  React + TypeScript UI：popover、mini 窗、
                               Workbench、设置、首次运行助手
apps/desktop/src-tauri/        Tauri 壳：窗口、托盘、IPC、updater
docs/                          架构、共享存储规则、设计语言、发布流水线，
                               以及跨客户端写入的契约
```

会话的读取和删除来自 [`agent-session-core`](https://github.com/AstroQore/agent-session-kit)，
即 `agent-session-kit` 的 Rust 实现——与原生版 Swift 实现用的是同一个 kit，所以两个客户端按同一套规则处理会话。

## 致谢

Desktop 是一次移植：里面几乎每一条规则——provider 的端点、bucket 的形状、存储布局、
同步引擎的围栏——都是从 [Vibe Bar](https://github.com/AstroQore/vibe-bar) 的 Swift
源码里读出来、在这里重新实现的，为的是让两个客户端表现一致。那份源码致谢谁，这里也致谢谁：

- [CodexBar](https://github.com/steipete/CodexBar) 是原生版菜单栏配额体验的技术参考，
  本客户端移植的若干行为经由原生版追溯到它：Cursor 的端点组合与 legacy request-plan
  回退的触发条件、Grok 计费响应的形状，以及「从本机运行的 language server 发现
  AntiGravity」这个思路。
- [CC Switch](https://github.com/farion1231/cc-switch) 启发了 Skills 管理器所协调的
  统一 skill 工作流，也仍是现有跨 agent skill 布局的互操作参考。
- [ccusage](https://github.com/ccusage/ccusage) 启发了本客户端扫描器所遵循的本地
  会话成本解析与定价语义。
- [LiteLLM](https://github.com/BerriAI/litellm)、
  [models.dev](https://github.com/anomalyco/models.dev) 和
  [Portkey Models](https://github.com/Portkey-AI/models) 维护着公开的模型价格目录，
  本客户端静态价格表里的费率经由原生版合并后的目录与
  [vibebar-model-pricing](https://github.com/AstroQore/vibebar-model-pricing)
  这层小补充追溯到它们。Desktop 目前还不会刷新或合并这些目录。

Desktop 构建在 [Tauri 2](https://github.com/tauri-apps/tauri) 及其 single-instance、
autostart、updater、dialog、opener 插件之上，半透明表面用
[window-vibrancy](https://github.com/tauri-apps/window-vibrancy)，底层是
[rusqlite](https://github.com/rusqlite/rusqlite) 与
[reqwest](https://github.com/seanmonstar/reqwest)，前端是
[React](https://github.com/facebook/react) 加 [Vite](https://github.com/vitejs/vite)。
会话来自 [agent-session-kit](https://github.com/AstroQore/agent-session-kit)，它是我们自己的，
与原生版共用。每个依赖及其版本都记在 `Cargo.lock` 和 `apps/desktop/pnpm-lock.yaml` 里，
各自的许可证在各自的仓库中。

这些项目与 Vibe Bar 相互独立，致谢不代表任何关联或背书。

## 许可证

AGPL-3.0-only，与原生版相同。

## Star 历史

<p align="center">
  <a href="https://star-history.com/#AstroQore/vibe-bar-desktop&Date">
    <img src="https://api.star-history.com/svg?repos=AstroQore/vibe-bar-desktop&type=Date" alt="Star 历史曲线">
  </a>
</p>
