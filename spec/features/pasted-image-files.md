# 本机图片文件粘贴为附件

终端接管 Ctrl+V 后，复制的图片文件可能以路径文本经 `Event::Paste` 到达 TUI。聊天输入现在会将单个可读取的图片路径转换为 `[Image #N]`，并持有图片数据，发送或执行中入队时使用现有附件链路。

- 支持 PNG、JPEG/JPG、WebP、GIF、BMP；统一转成 PNG，GIF 只取首帧。
- 支持中文、空格、引号、绝对路径、项目目录相对路径及 `file://` URL。带括号的绝对文件路径按字面量读取，不执行 shell。
- Alt+V 的剪贴板文件列表复用相同解码器，不再仅支持 RGB/RGBA PNG。
- 普通文本、非图片路径、缺失或损坏图片、超限图片继续保留为文本。多行内容保持已有粘贴占位符行为，不自动拆分为多个附件。
- 本机 `!` 命令草稿和运行中 shell stdin 不进行图片路径转换；设置面板和交互弹窗仍优先处理粘贴。
- 输入文件和输出 PNG 均限制为 20 MiB，RGBA 像素数据限制为 64 MiB，并设置解码器内存限制。

## 本地验证

使用临时目录生成真实编码的图片，不读取或修改系统剪贴板来构造测试。

- `clipboard::image_file`：五种格式、灰度 PNG、按内容识别格式、坏文件及文件大小限制。
- `app::paste_ops::image_paste_test`：终端粘贴事件、中文空格和引号、file URL、括号文件名、相对路径、执行中 Enter 入队、普通文本回退、shell 草稿和设置面板保护。
- 同时回归现有 `paste_ops`、`queued` 和键盘处理测试。

上述为本地/headless 回归；不等同于 Windows Terminal 实机操作和真实模型接收验收。运行中的已安装 cc-code 不会自动应用源码修改，需要构建或发布新版本。

2026-09-21 本地测试结果：`image_paste`、`paste_ops`、`queued`、`event::keyboard`、`clipboard::` 合并筛选，共 95 项通过。单任务编译使用 `RUST_MIN_STACK=16777216`，TUI 测试 profile 覆盖 `debug=0`、`codegen-units=8`，未修改系统或项目构建配置。

`cargo check -p peri-tui --bins -j 1`、本次 Rust 文件格式检查及 `git diff --check` 均通过。

## 本机试用构建

2026-09-21 已额外生成独立开发版 `target/debug/cc-code-image-paste.exe`，`--version` 和 `--help` 均返回成功。原有 `target/debug/cc-code.exe` 及 npm 安装版本未覆盖，正在运行的旧会话未终止。开发版显示 workspace 版本 `0.2.0`，不是 npm 发版号。

在新的 Windows Terminal 标签页、目标项目目录下执行：

```powershell
& 'D:\code\peri\target\debug\cc-code-image-paste.exe'
```

在资源管理器复制单张本机图片，然后回到新开的聊天输入框按 Ctrl+V，预期出现 `[Image #N]`；按 Enter 后才提交给 Agent。上述二进制启动检查不替代实际粘贴或真实模型验收。
