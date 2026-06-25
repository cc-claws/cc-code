# [BUG] Hook 命令 stderr 中文乱码：GBK 编码未正确处理

**状态**: Open
**优先级**: P2
**模块**: middlewares/hooks
**创建时间**: 2026-06-25
**发现方式**: 用户报告

## 现象

`peri.log` 中 hook 命令失败时的 stderr 输出显示为乱码：

```
WARN Command hook exited with code 1: 'bash' ���ڲ����ⲿ���...
WARN Command hook exited with code 1: ��ʱ��Ӧ�� &��
```

日志文件本身是 UTF-8 编码（BOM `EF BB BF` 确认）。

## 根因

`peri-middlewares/src/hooks/executor.rs:98` 使用 `String::from_utf8_lossy` 解码子进程 stderr：

```rust
let stderr = String::from_utf8_lossy(&output.stderr);
```

Windows 上 `cmd.exe` 的 stderr 输出使用系统 code page（中文 Windows 默认 cp936/GBK）。`from_utf8_lossy` 将无效 UTF-8 字节替换为 `�`（U+FFFD），导致中文内容不可读。

### 字节级证据

日志文件第 13 行乱码区域 hex dump：

```
27 62 61 73 68 27 20 EF BF BD EF BF BD ... DA B2 ...
```

- `EF BF BD` = U+FFFD（`from_utf8_lossy` 替换结果）
- `DA B2` = GBK 中 `内` 字的原始字节（恰好也是合法 UTF-8 序列，未被替换）

原始输出应为：
- `'bash' 不是内部或外部命令，也不是可运行的程序或批处理文件。`
- `此时不应有 &。`

## 影响

- 所有 command hook（exit code 1/2/unexpected）的 stderr 日志不可读
- Windows 用户无法从日志排查 hook 执行失败原因
- 日志中保留大量无意义的乱码行

## 修复方向

在 `executor.rs` 中对 Windows 平台使用 GBK 解码：

```rust
// 方案 1：cfg 条件编译
#[cfg(target_os = "windows")]
let stderr = {
    use encoding_rs::GBK;
    let (decoded, _, _) = GBK.decode(&output.stderr);
    decoded.into_owned()
};

#[cfg(not(target_os = "windows"))]
let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
```

需要在 `peri-middlewares/Cargo.toml` 添加 `encoding_rs` 依赖。

注意：需确认用户是否通过 `chcp 65001` 切换了 code page（此时 stderr 可能是 UTF-8）。可考虑优先尝试 UTF-8，失败后回退 GBK。

## 相关文件

- `peri-middlewares/src/hooks/executor.rs:97-98` — `from_utf8_lossy` 调用点
- `peri-middlewares/Cargo.toml` — 需添加 `encoding_rs` 依赖

## 关联 Issue

- 无

## 验证标准

Windows 中文环境下，hook 命令 stderr 包含中文时，`peri.log` 中显示正确的中文字符而非乱码。
