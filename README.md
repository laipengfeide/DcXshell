# Dc Xshell

一款基于 **Tauri 2 + Rust + Vue 3** 的桌面 SSH 客户端，面向运维、测试与研发人员，**无需 USB Key** 即可使用。

## 功能特性

### 会话与连接管理
- 支持登录（本地账号）或以访客身份进入；**访客最多 3 个并行会话，登录后不限数量**
- 连接信息保存在本地 SQLite 数据库，支持分组、树形展示
- 新建连接前提供「测试连接」，**测试成功后才能保存**
- 右侧以服务器卡片 + 终端形式展示会话

### 终端
- 基于 xterm.js 的终端，支持窗口自适应
- 持续输出型命令（如 `top`）可不停刷新显示
- 支持 `Ctrl+C` / `Ctrl+V` 复制粘贴，也支持鼠标右键复制、粘贴

### 稳定性
- 5 秒心跳检测连接状态
- 掉线后自动重连，最多尝试 3 次；仍失败则卡片变红提示离线

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

## 技术栈

| 层 | 技术 |
| --- | --- |
| 桌面框架 | Tauri 2.11 |
| 后端 | Rust：`russh` 0.65（SSH）、`russh-sftp` 3（SFTP）、`rusqlite`（SQLite）、`tokio` |
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

> 仅需调试前端界面时，可执行 `npm run dev` 在浏览器中预览，但 SSH / SFTP 等功能依赖 Tauri 后端，需用 `npm run tauri dev`。

## 使用流程

1. 启动客户端，登录账号或以访客身份进入
2. 新建连接，填写主机、端口、用户名与密码/密钥等信息
3. 点击「测试连接」，成功后保存到左侧连接树
4. 双击连接打开会话，终端即可交互
5. 点击会话卡片上的文件夹图标，打开文件传输：上传、下载、右键复制/粘贴/解压/重命名/删除
6. 连接异常时客户端自动重连 3 次，仍失败则卡片变红提示已掉线

## 目录结构

```
DcXshell/
├─ src/                     前端源码（Vue 组件、API、样式）
│  ├─ components/           连接弹窗、终端会话、文件传输弹窗等
│  ├─ api.ts                Tauri 命令调用封装
│  └─ types.ts              数据类型定义
├─ src-tauri/
│  ├─ src/
│  │  ├─ main.rs            入口与命令注册
│  │  ├─ commands.rs        连接/会话管理与状态
│  │  ├─ ssh_client.rs      SSH 连接、心跳与重连
│  │  ├─ sftp.rs            SFTP 文件传输、复制、解压
│  │  └─ db.rs              SQLite 存储
│  ├─ capabilities/         Tauri 权限声明
│  └─ tauri.conf.json       应用配置
├─ package.json
└─ README.md
```

## 说明

- 依赖与构建产物（`node_modules/`、`dist/`、`src-tauri/target/`、安装包等）不纳入版本管理。
- 若服务器缺少 `tar` / `unzip`，解压会提示先安装对应工具；zip 无 `unzip` 时需有 `python3`。
