# Osu! Beatmap Seekman

Osu! Beatmap Seekman 是一个基于 Tauri 2 + Rust 的 osu! beatmap 批量搜索与下载工具。它支持按星数、日期、长度、模式、OD/HP/CS/AR/BPM、mania 键数等条件构建候选列表，并通过多个镜像源批量下载到 osu!stable 的 `Songs` 目录，也支持 `.osu` 单文件模式、osu!lazer 曲库去重、AlphaOsu PP 推荐、歌单导入导出和收藏夹写入。

当前版本：`2.1.0`

## 主要功能

1.歌曲搜索下载。你可以以超级快的速度批量下载茫茫多的图！Seekman支持Loved，Ranked，Graveyard的自   由下载，强大的搜索过滤功能，让你在四个镜像站和调度算法加持下，下图超快！
  64最大并发，一百万队列，睡梦中安然下图！

2.接入AlphaOSU！通过先进的AI算法，输入ID就能推荐一大批适合你刷的PP图并一键下载它们！
//Alphaosu项目地址 https://github.com/AlphaOSU/AlphaOSU/

3.强大的收藏夹分享功能！游戏内的收藏夹随意导出并下到朋友的电脑里，并自动为他创建新的收藏夹！
  给别人推荐歌单再也不用苦苦打表也不需要一个个下载了，seekman全部做到了自动化！

4.全平台支持，Lazer部分适配，Linux和安卓均可畅想高速下图服务和Seekman的好用功能！

5.现代化的UI，流畅高效的性能,还有更多好用的功能，快下载试试吧！

## 2.1.0 更新

- 修复 Tauri 发行版中“查看发布页”、作者 osu! 主页和 B 站赛事录像链接无法打开系统浏览器的问题。
- 外部链接统一通过后端 opener 打开，避免 `window.open` 在桌面端失效。

## 2.0.2 更新

- 窄屏和平板布局下，搜索候选列表和歌单候选列表限制为约十行高度，列表内部滚动，避免页面被超长候选列表撑得需要滑动半天。
- 下载任务新增“强制结束”：当下载到收藏夹的歌单任务只剩少数歌曲失败或卡住时，可以把已经下载并缓存完成的歌曲直接转移到 Songs 并写入目标收藏夹，同时移除未完成项目。

- 全新分页式界面：`设置`、`搜图`、`下载`、`歌单` 分页拆开，页面更清爽。
- 下载逻辑改为“任务”模型：一次添加会形成一个下载任务，任务从前往后依次处理，点开任务可查看具体下载项目。
- 每个下载任务支持单独删除，并带二次确认弹窗。
- 总进度条改为按歌曲完成数量计算，例如 `20/40` 就显示 50%，避免下载大小变化导致进度条回退。
- 歌单模式增强：支持扫描 osu!stable 收藏夹、导出收藏夹歌单、导入 Seekman 歌单并一键添加为任务。
- 歌单导出会保存更多谱面信息，包含 beatmapset id、beatmap id、标题、作者、难度名、模式、MD5、OD/HP/CS/AR/BPM、时长、来源、tags 等字段。
- 歌单导入后会保留源收藏夹中的具体子难度信息，下载完成写入收藏夹时只写入这些特定难度。
- 歌单任务会先全部缓存到本地，等任务内歌曲全部下载完成后再统一转移到 `Songs` 并修改收藏夹，减少中途失败造成的半成品状态。
- 支持下载到新建收藏夹，也支持选择已有收藏夹作为目标。
- 添加 osu!lazer 曲库扫描，用 lazer 的本地结构进行去重提示；lazer 谱面仍需要用户手动导入到 lazer。
- 接入 AlphaOsu 推荐，可输入用户 ID/用户名获取 PP 推荐图并加入候选列表。
- 搜索关键词帮助弹窗：说明 `artist`、`creator`、`title`、`source`、`tag` 等官方搜索字段。
- 设置页新增本地识别刷新按钮，可按当前选择的 Stable/lazer 来源重新扫描计数。
- 新增多主题配色，并记住用户设置。
- 顶部分页按钮与下载页信息栏经过视觉强化，界面层次更清楚。
- Android 端保留存储权限申请和外部目录下载逻辑，支持通过 GitHub Actions 构建签名 APK。


## 歌单与收藏夹Stable全面适配，Lazer可用

歌单功能面向 osu!stable：

- 扫描 `collection.db`，列出已有收藏夹。
- 导出某个收藏夹为 Seekman CSV 歌单。
- 导入别人分享的 Seekman 歌单后，可直接添加为下载任务。
- 支持将下载结果写入新收藏夹或已有收藏夹。
- 写入收藏夹前会自动备份 `collection.db`。
- *对于Lazer用户，可以选择把歌单下载在本地之后手动导入，实现导入歌单的效果
注意：收藏夹写入是实验性功能。使用前建议关闭 osu!stable，并确认选择的是包含 `Songs` 和 `collection.db` 的 osu!stable 根目录。

## osu! API 填什么

进入 osu! 账号设置页面，创建一个 OAuth Application：

1. 打开 [osu! OAuth 应用页面](https://osu.ppy.sh/home/account/edit#authenticator-app)。
2. 新建应用，应用名可以填写 `Osu Beatmap Seekman`。
3. `Application Callback URL` 可以填写 `http://localhost`。
4. 创建后复制 `Client ID` 和 `Client Secret` 到软件里。

软件搜索 beatmapset 通常只需要：

- `Client ID`：osu! 给你的应用数字 ID。
- `Client Secret`：osu! 给你的应用密钥。
- `Bearer Token`：一般留空。

程序会使用 `client_credentials` 自动获取 public token。

## 开发运行

安装依赖：

```powershell
npm install
```

启动开发模式：

```powershell
npm run dev
```

如果已经构建过 debug 版本，也可以运行：

```powershell
.\run.ps1
```

## 构建正式版

Windows 本地构建：

```powershell
npm run tauri:build
```

正式版构建产物通常在：

```text
src-tauri/target/release/osu_beatmap_seekman.exe
src-tauri/target/release/bundle/nsis/Osu! Beatmap Seekman_2.1.0_x64-setup.exe
```

Android 本地构建：

```powershell
npm run tauri:android:build
```

## GitHub 多平台发布

仓库包含 GitHub Actions 工作流：

- `.github/workflows/release-desktop.yml`：构建 Windows NSIS 安装包、Linux deb / AppImage、macOS DMG。
- `.github/workflows/release-android.yml`：构建 Android APK，并使用仓库 Secrets 签名。

推送版本标签即可触发云端构建并上传到 GitHub Release 草稿：

```powershell
git tag v2.1.0
git push origin v2.1.0
```

如果要重新发布同一个版本，需要先删除本地和远端旧标签，再重新推送。

## 发布文件

发布附件建议包含：

```text
Osu! Beatmap Seekman_2.1.0_x64-setup.exe
Osu! Beatmap Seekman_2.1.0_amd64.AppImage
Osu! Beatmap Seekman_2.1.0_amd64.deb
Osu! Beatmap Seekman_2.1.0_aarch64.dmg
app-universal-release-signed.apk
Osu-Beatmap-Seekman-2.1.0-source.zip
SHA256SUMS.txt
```

普通 Windows 用户推荐下载：

```text
Osu! Beatmap Seekman_2.1.0_x64-setup.exe
```

## 镜像源说明

当前 `.osz` 下载会按用户设置优先使用以下镜像：

- Hinamizawa
- Catboy
- Nerinyan
- Sayobot

为了避免被镜像站限制，程序会为请求带上 User-Agent，并为 Sayobot 请求带上项目发布地址作为 Referer：

```text
https://github.com/linnzero00/Osu-Beatmap-Seekman
```

