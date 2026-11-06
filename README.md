# RitKraken

Rust 桌面 Git 客户端。界面以 GitKraken Desktop 9.11.0 的三栏布局为参照，使用 GPUI Kit 0.9.0 渲染，通过本机 Git 读取和更新仓库。

## 使用

打开构建后的 RitKraken，在 **Repository Management** 选择 **Open a Repository**，也可以在底部输入仓库路径。顶部标签页可在多个仓库间切换。

1. 点提交图里的 `// WIP` 查看工作区，或点历史提交查看说明和文件。
2. 点右栏的文件打开文件页。分支栏会收成图标栏，关闭文件时恢复。在 **File View** 查看对应版本，在 **Diff View** 切换 **Hunk**、**Inline** 或 **Split**，用箭头跳到上一个或下一个改动。
3. 用 **Stage** 暂存整文件，或用 **Stage hunk** 暂存一个区块。已暂存区块可在 **Staged Files** 中打开并取消暂存。
4. 填写 **Summary** 并提交。选择 **View changes** 回到剩余工作区改动。

单击分支可定位提交，双击本地分支可切换分支。`F5` 刷新，`Ctrl+O` 打开仓库管理。

**Fetch** 更新所有远端的跟踪引用，右侧箭头可选单个远端。**Push** 推送当前分支，首次推送可从菜单选远端并建立跟踪。**Branch** 创建并切换分支。**Stash** 收起暂存、未暂存和未跟踪的改动，**Pop** 恢复最近一次 stash 和它的暂存状态。

右上角齿轮或 `Ctrl+,` 打开偏好设置，点 **Exit Preferences** 保存。默认先加载 2,000 条提交，到达提交图底部时继续加载，并在下次启动恢复打开的仓库标签。

## 构建

在项目根目录运行对应平台的脚本。两个脚本都按当前 Rust 工具链的架构做 release 构建，并使用 `Cargo.lock`。

| 平台 | 命令 | 产物 |
| --- | --- | --- |
| Windows | `.\build-windows.cmd` | `target/windows/<Rust 目标>/release/ritkraken.exe` |
| macOS | `bash ./build-macos.sh` | `target/macos/<Rust 目标>/release/RitKraken.app` |

使用 Rust 1.92 或更新版本，并安装本机 Git。[GPUI Kit 平台要求](https://gpui-kit.com/docs/installation/)

- Windows 需要 MSVC Rust 工具链、Visual Studio 的 C++ Build Tools 和 Windows SDK。脚本会复用项目内已有的 `.build/rustup` 和 `.build/cargo`，也可通过 `RUSTUP_HOME` 和 `CARGO_HOME` 指定。
- macOS 需要 macOS 15 或更新版本及 Xcode Command Line Tools，可用 `xcode-select --install` 安装。生成的 `.app` 使用 ad hoc 签名，可在构建机上打开。

文件行数检查需要 Python 3.11 或更新版本。

```powershell
cargo test --workspace --release --locked
python tools/check_project.py
```

也可用 `cargo run --release -- <仓库路径>` 直接打开仓库。文件和 diff 预览上限为 8 MiB。区块暂存适用于普通文本修改，新增、删除、重命名和二进制文件可整文件暂存。

## 代码组织

- `crates/git` 提供仓库、历史、工作区、diff 和远端操作。这个包独立于 GPUI，集成测试使用临时仓库和本地 bare 远端。
- `crates/desktop` 负责窗口与交互。仓库管理、引用、提交图、暂存与提交详情、文件审阅、工具栏和设置各自维护功能代码。`shell.rs` 组合应用外壳，`session.rs` 管理标签，`operations.rs` 调度后台 Git 操作。

项目文件的硬上限是 1000 行。`tools/check_project.py` 检查已跟踪和未被 Git 忽略的文件。依赖变更后，可运行 `python tools/check_project.py --compact-lock <Cargo生成的锁文件>` 保存紧凑的锁文件，然后用 `--locked` 验证构建。
