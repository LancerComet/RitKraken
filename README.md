# RitKraken

Rust 桌面 Git 客户端原型。界面以 GitKraken Desktop 9.11.0 的三栏布局为参照，使用 GPUI Kit 0.9.0 渲染，通过本机 Git 读取和更新仓库。

## 在 Windows 试用

双击构建后的 `target/release/ritkraken.exe`，在 **Repository Management** 选择 **Open a Repository**，也可以在底部输入仓库路径。顶部标签页可在多个仓库间切换。

1. 点提交图里的 `// WIP` 查看工作区，或点历史提交查看说明和文件。
2. 点右栏的文件查看 diff，用 **Inline** 和 **Split** 切换显示方式。
3. 用 **Stage** 暂存整文件，或用 **Stage hunk** 暂存一个区块。已暂存区块可在 **Staged Files** 中打开并取消暂存。
4. 填写 **Summary** 并提交。选择 **View changes** 回到剩余工作区改动。

单击分支可定位提交，双击本地分支可切换分支。`F5` 刷新，`Ctrl+O` 打开仓库管理。

## 构建

本机验证使用 Rust 1.99.1、MSVC Build Tools 和 Git for Windows。

```powershell
cargo build --release --locked
cargo test --release --locked
```

也可用 `cargo run --release -- <仓库路径>` 直接打开仓库。原型加载最近可达的最多 100,000 条提交，diff 预览上限为 8 MiB。区块暂存适用于普通文本修改，新增、删除、重命名和二进制文件可整文件暂存。
