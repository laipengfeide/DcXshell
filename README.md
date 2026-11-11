# Dc Xshell

一款基于 **Tauri 2 + Rust + Vue 3** 的桌面 SSH 客户端，面向运维、测试与研发人员。支持**修改密码、操作日志审计**，以及**局域网内登录用户之间的流式文件互传**，无需 USB Key 即可使用。

## 功能特性

### 会话与连接管理
- 支持本地账号登录或以访客身份进入；**访客最多 3 个并行会话，登录后不限数量**
- 连接信息保存在本地 SQLite 数据库，支持分组、树形展示，按登录用户隔离
- 新建连接前提供「测试连接」，**测试成功后才能保存**
- 右侧以服务器卡片 + 终端形式展示会话

### 终端
- 基于 xterm.js 的终端，支持窗口自适应
- 持续输出型命令（如 `top`）可不停刷新显示
- 支持 `Ctrl+C` / `Ctrl+V` 复制粘贴，也支持鼠标右键复制、粘贴
- 仅在用户按**回车提交整行命令**后记录到操作日志（见下方），避免逐字符噪音

### 稳定性
- 5 秒心跳检测连接状态
- 掉线后自动重连，最多尝试 3 次；仍失败则卡片变红提示离线

### 账号与安全
- 左下角头像 + 用户名菜单，登录用户含「修改密码」「操作日志」；访客无「修改密码」入口
- 修改密码需要原密码校验，新密码需非空
- 登录、退出、访客进入/离开时自动记录本机局域网 IPv4（取真实网卡出口地址，不走 127.0.0.1），写入用户记录与会话记录，便于后续局域网互传定位对方

### 操作日志审计
- 所有登录用户和访客的操作都写入本地 SQLite 审计表，**仅本人可见**（访客按设备标识隔离）
- 记录的动作：
  - 登录、退出（含访客进出）
  - 连接到哪台服务器（连接名 + 主机地址）
  - 终端里**回车提交的整行命令**
- **密码脱敏**：命令行含明显密码形态（`-p***`、`--password`、`passwd` 后跟内容）时自动打码；`sudo`、`ssh`、`su`、`mysql -p`、`passwd` 等交互密码提示会识别并对后续输入打码，绝不明文存储
- 日志页面按时间倒序显示，支持按动作类型筛选

### 局域网文件互传（阶段二）
- 同网段双方都**已登录**后自动互相发现，无需手动输入 IP
- 协议：**UDP 45454 广播公告（3 秒/次、10 秒超时）+ TCP 45455 直连流式传输**
- 单文件上限 **4GB**，全程 64KB 分块流式收发，**不整文件读入内存**
- 传输完成后 SHA-256 校验，不一致会删除残缺文件并标记失败
- 接收方收到请求后弹窗确认，可更改保存目录；传输记录里完成的条目提供「打开目录」按钮，点击后系统文件管理器会自动定位并选中该文件
- 首次监听会触发 Windows 防火墙授权（需选**专用网络**），按提示放行即可
- 工具条在「新建连接」旁边新增「互传文件」按钮；**访客不参与**局域网互传

### 文件传输（SFTP）
- 连接成功后点击会话卡片上的文件夹图标打开文件传输弹窗
- 上传：可选择上传目录，右侧实时列出服务器目录，内置常用目录（默认 `/home`）
- 下载：选择远端文件保存到本地
- 传输过程弹窗显示文件名、大小与进度；小文件快速完成时直接提示成功
- 服务器目录支持右键操作：
  - 复制 / 粘贴（切换目录后仍可一直粘贴同一文件）
  - 同名文件冲突时弹窗选择「覆盖」或「重命名新文件」（自动加 `(2)`）
  - 重命名、删除（文件与目录）
- 压缩包右键解压：
  - 解压到当前文件夹
  - 解压到同名文件夹（如 `bundle.tar.gz` → `bundle/`）
  - 支持 `.tar` / `.tar.gz` / `.tgz` / `.tar.bz2` / `.tbz2` / `.tar.xz` / `.txz` / `.zip` / `.gz` / `.bz2` / `.xz`
  - 解压通过服务器本地 `tar` / `unzip`（zip 无 unzip 时回退 `python3 -m zipfile`）执行

### 界面
- 暗色主题，Element Plus 组件库
- 屏蔽浏览器默认右键菜单，右键交互统一使用应用内菜单
- 主工具栏两行布局，为后续功能（如互传文件）预留位置，避免越挤越乱

### 分发与运行时
- Rust 开启 **MSVC crt-static**：单 exe 不再依赖目标机器的 `VCRUNTIME140.dll`，拷贝到任何 Win10/11 机器双击即可运行
- 测试程序 `sshtest` 通过 Cargo feature gate (`test-server`) 排除出 release 打包，安装包内不包含该二进制

## 技术栈

| 层 | 技术 |
| --- | --- |
| 桌面框架 | Tauri 2.11 |
| 后端 | Rust：`russh` 0.65（SSH）、`russh-sftp` 3（SFTP）、`rusqlite`（SQLite）、`tokio`（TCP）、标准库 `UdpSocket`（广播发现）、`sha2`（校验） |
| 前端 | Vue 3 + TypeScript + Vite 4 |
| UI / 终端 | Element Plus 2.7、xterm.js 5.5 |
| 插件 | tauri-plugin-dialog、tauri-plugin-clipboard-manager |

## 环境要求

- Windows 10/11 x64（WebView2 Runtime，Win11 通常自带）
- Node.js ≥ 18 与 npm
- Rust ≥ 1.89（通过 [rustup](https://rustup.rs/) 安装）
- Visual Studio Build Tools（C++ 生成工具 / MSVC）

## 本地运行

```bash
git clone https://github.com/laipengfeide/DcXshell.git
cd DcXshell

# 安装前端依赖
npm install

# 以开发模式启动桌面客户端（前端热更新 + Rust 后端）
npm run tauri dev
```

## 构建安装包

```bash
npm run tauri build
```

构建产物：

- 可执行文件：`src-tauri/target/release/dc-xshell.exe`
- Windows 安装包：`src-tauri/target/release/bundle/nsis/Dc Xshell_1.0.0_x64-setup.exe`

> 仅需调试前端界面时，可执行 `npm run dev` 在浏览器中预览，但 SSH / SFTP / 局域网互传等功能依赖 Tauri 后端，需用 `npm run tauri dev`。

## 使用流程

1. 启动客户端，登录账号或以访客身份进入
2. 新建连接，填写主机、端口、用户名与密码/密钥等信息
3. 点击「测试连接」，成功后保存到左侧连接树
4. 双击连接打开会话，终端即可交互；所有回车提交的命令会被**脱敏写入操作日志**
5. 点击会话卡片上的文件夹图标，打开文件传输：上传、下载、右键复制/粘贴/解压/重命名/删除
6. 点击左下角头像菜单或顶部「操作日志」按钮查看本人操作记录
7. 双方都登录后，打开「互传文件」面板即可看到同网段在线用户，选择对方并选文件发送；接收方弹窗确认后开始传输，完成后可点「打开目录」定位文件

## 局域网互传 — 首次提示

- **防火墙授权**：首次监听 UDP/TCP 端口时 Windows 会弹「Windows Defender 防火墙已阻止此应用的一些功能」，请勾选**专用网络**后点「允许访问」。公用网络下不会被其他设备发现。
- **双方都需要登录**：访客不参与互传，只有已登录账号才会广播自己。

## 目录结构

```
DcXshell/
├─ src/
│  ├─ components/
│  │  ├─ Sidebar.vue             连接树 + 头像菜单 + 工具栏
│  │  ├─ SessionView.vue         终端会话卡片
│  │  ├─ AuditLogView.vue        操作日志页面
│  │  ├─ ChangePasswordDialog.vue 修改密码弹窗
│  │  └─ TransferPanel.vue       局域网互传面板
│  ├─ views/WorkspaceView.vue     主工作区，整合所有子组件
│  ├─ api.ts                     Tauri 命令调用封装
│  └─ types.ts                   数据类型定义
├─ src-tauri/
│  ├─ src/
│  │  ├─ main.rs                 入口与命令注册
│  │  ├─ commands.rs             连接/会话/密码修改/打开路径
│  │  ├─ auth.rs                 登录注册与会话恢复
│  │  ├─ audit.rs                操作日志读写与密码脱敏
│  │  ├─ net.rs                  本机局域网 IPv4 探测
│  │  ├─ lan.rs                  UDP 广播发现 + TCP 流式传输
│  │  ├─ ssh_client.rs           SSH 连接、心跳与重连
│  │  ├─ sftp.rs                 SFTP 文件传输、复制、解压
│  │  ├─ models.rs               数据库表结构
│  │  └─ db.rs                   SQLite 初始化与设备标识
│  ├─ .cargo/config.toml         MSVC crt-static 消除 VCRUNTIME140 依赖
│  ├─ capabilities/              Tauri 权限声明
│  └─ tauri.conf.json           应用配置与版本资源
├─ package.json
└─ README.md
```

## 说明

- 依赖与构建产物（`node_modules/`、`dist/`、`src-tauri/target/`、安装包等）不纳入版本管理。
- 若服务器缺少 `tar` / `unzip`，解压会提示先安装对应工具；zip 无 `unzip` 时需有 `python3`。
- 首次运行会在 `%APPDATA%\com.dcxshell.app\` 下生成 `dcxshell.db`（SQLite，包含连接、审计日志、设备标识）与 `secret.key`（用于本地连接密码加密）。卸载不会删除这些数据。
