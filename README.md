# Osu! Beatmap Seekman

Osu! Beatmap Seekman 是一个基于 Tauri 2、React 和 Rust 的 osu! beatmap 下载工具。它可以按星数，日期等按条件搜索 ranked / loved 谱面，构建下载队列，并通过多个镜像源批量下载到 osu! 的 `Songs` 文件夹。

支持Windows,Linux甚至Android平台，一键下载一千张rank图，多个镜像站多管齐下多重并发，自动检测卡顿换源重试，让你安心入睡

【面向有批量下载特定时间段全 rank、loved 需求的人群，可以像官网那样筛选各种条件，pp 吃专用，再也不怕没图刷啦】

（android版本待优化）


各种下图器都太老了用不惯，hiosu 的全 rank 没了，一怒之下怒了一下，拿买 GPT 送的 Codex 弄一个下图器，不用白不用。

当前版本：`1.0.3`

## 主要功能

- 选择 osu! 的 `Songs` 文件夹作为 `.osz` 下载目标。
- 扫描本地曲库，识别已有文件夹和 `Songs` 中现有的 `.osz` 文件，避免重复加入队列。
- 支持扫描本地已有谱面，并自动在搜索时去重避免重复下载。
- 支持 Ranked / Loved、日期段、星数 SR、OD、HP、CS、AR、BPM、长度、模式、mania 4K/7K、关键词筛选。
- 日期、CS、AR 等条件会尽量使用 osu! 官方搜索语法传入，以减少本地过滤压力。
- 支持按时间、长度、BPM 正序或倒序排序，默认按时间从新到旧。
- `.osz` 下载支持带视频、不带视频。
- 支持仅 `.osu` 文件模式，该模式从 osu! 官方地址 `https://osu.ppy.sh/osu/BEATMAP_ID` 下载。
- `.osu` 文件不会进入 `Songs`，会保存到软件目录同级的 `.osu` 文件夹。
- `.osz` 下载支持 Hinamizawa、Catboy、Nerinyan、Sayobot 多镜像源。
- 支持手动调整镜像源优先级。
- 支持混杂模式：启用后会轮流使用多个镜像源并发下载，失败或卡住时切换到下一个源。
- 下载队列最多 1,000,000 个任务。
- 加入队列后不会自动下载，需要手动开始。
- 下载时实时显示当前镜像源和进度。
- 下载缓存会先写入软件根目录的 `download-cache`，完成后再移动到目标目录，避免在 `Songs` 里留下碎片文件。
- 支持暂停、继续、重试、清空队列。
- 一键重试会丢弃旧缓存，重新读取当前镜像优先级，并从最高优先级镜像重新下载；旧的卡住尝试不会再更新任务进度。
- 下载过程中如果 30 秒没有收到新数据，会自动判定为卡住并切换到下一个镜像源。
- 已完成任务会自动从下载队列移除。
- 任务和设置保存到 Tauri 应用数据目录的 `state.json`。

## osu! API 填什么

进入 osu! 网页端账号设置，创建一个 OAuth Application：

1. 打开 osu! 账号设置里的 OAuth 应用页面。
2. 新建应用，应用名可以填 `Osu Beatmap Seekman`。
3. `Application Callback URL` 可以填 `http://localhost`。
4. 创建后复制 `Client ID` 和 `Client Secret` 到软件里。

软件搜索 beatmapset 只需要：

- `Client ID`：osu! 给你的应用数字 ID。
- `Client Secret`：osu! 给你的应用密钥。
- `Bearer Token`：一般留空。

程序会使用 `client_credentials` 自动获取 public token。

## 开发运行

先安装依赖：

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

```powershell
npm run tauri:build
```

正式版构建产物通常在：

```text
src-tauri/target/release/osu_beatmap_seekman.exe
src-tauri/target/release/bundle/nsis/Osu! Beatmap Seekman_1.0.3_x64-setup.exe
```

## GitHub 多平台发布

仓库包含 GitHub Actions 工作流：

- `.github/workflows/release-desktop.yml`：构建 Windows NSIS 安装包、Linux deb / AppImage、macOS DMG。
- `.github/workflows/release-android.yml`：构建 Android APK。

推送版本标签即可触发云端构建并上传到 GitHub Release 草稿：

```powershell
git tag v1.0.3
git push origin v1.0.3
```

如果要重新发布同一个版本，需要先删除本地和远端旧标签，再重新推送。

### Android APK 签名

Android release APK 必须签名才能安装。先在本地生成 keystore：

```powershell
keytool -genkeypair -v -keystore seekman-release.jks -alias seekman -keyalg RSA -keysize 2048 -validity 10000
```

再把 keystore 转成 Base64：

```powershell
[Convert]::ToBase64String([IO.File]::ReadAllBytes("seekman-release.jks")) | Set-Content -NoNewline seekman-release.jks.base64.txt
```

到 GitHub 仓库的 `Settings -> Secrets and variables -> Actions -> New repository secret` 添加：

```text
ANDROID_SIGNING_KEY        seekman-release.jks.base64.txt 里的完整内容
ANDROID_KEY_ALIAS          seekman
ANDROID_KEYSTORE_PASSWORD  生成 keystore 时输入的 store password
ANDROID_KEY_PASSWORD       生成 keystore 时输入的 key password
```

不要把 `.jks` 或密码提交到仓库。

## 发布文件

本项目的发布附件建议放在 `release-artifacts` 文件夹中：

```text
Osu! Beatmap Seekman_1.0.3_x64-setup.exe
Osu-Beatmap-Seekman-1.0.3-portable.exe
Osu-Beatmap-Seekman-1.0.3-source.zip
SHA256SUMS.txt
```

推荐用户下载安装版：

```text
Osu! Beatmap Seekman_1.0.3_x64-setup.exe
```

