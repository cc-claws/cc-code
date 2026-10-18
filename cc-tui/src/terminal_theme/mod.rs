//! 终端主题自动探测：`theme = "auto"` 时由本模块决定实际调色板。
//!
//! 两级判定：
//! 1. **颜色能力**：`TERM_PROGRAM=Apple_Terminal` 不支持 24-bit 真彩色，选 `*-ansi` 调色板
//! 2. **背景明暗**：先发 OSC 11 查询问终端自己的背景色（回 `rgb:RRRR/GGGG/BBBB` 后按
//!    亮度判明暗）；终端不应答时，macOS 退回系统外观（`AppleInterfaceStyle`），其余按深色
//!
//! 注意：**不要**从 Terminal.app 的 `Default Window Settings` profile 推断背景色。那只代表
//! 下一个新窗口用哪个 profile，窗口可以在 `Shell > Show Inspector` 里单独指定（本机上
//! 默认 profile 是黑底的 Pro，而实际窗口却是白底的 Basic，据此判定必错）。
//!
//! 探测结果缓存到 `OnceLock`：`theme::cur()` 在渲染热路径上，子进程探测只能发生一次。

use std::sync::OnceLock;

use crate::ui::theme::ThemeName;

/// 探测结果缓存。
static RESOLVED: OnceLock<ThemeName> = OnceLock::new();
/// OSC 11 查询结果缓存（`None` = 终端未应答或本次不查询）。
static OSC_SCHEME: OnceLock<Option<Scheme>> = OnceLock::new();

/// 终端颜色能力。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorSupport {
    /// 24-bit 真彩色，可直接使用 `Color::Rgb` 调色板。
    TrueColor,
    /// 仅 16/256 色，使用 `*-ansi` 调色板。
    Ansi,
}

/// 终端背景明暗。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scheme {
    Light,
    Dark,
}

/// 解析 `theme = "auto"` 对应的具体主题（进程内只探测一次）。
pub fn resolve() -> ThemeName {
    *RESOLVED.get_or_init(|| theme_for(capability(), scheme()))
}

/// 启动期预探测背景明暗。
///
/// 必须在 `enable_raw_mode()` 之后、`EventReader::start()` 之前调用：查询回复从 stdin
/// 原始字节读入，事件读取器一旦启动就会抢先消费。
///
/// 只有持久化主题为 `auto` 时才探测，显式主题的用户不必付这笔启动开销
/// （探测未命中需等满超时）。运行期再切到 `auto` 时来不及做终端往返了，
/// `scheme()` 只能走平台回退（macOS 读系统外观）。
pub fn preload() {
    let wants_auto = crate::config::load()
        .ok()
        .and_then(|c| c.config.theme)
        .and_then(|t| ThemeName::from_setting(&t))
        == Some(ThemeName::Auto);
    if !wants_auto {
        return;
    }
    let _ = OSC_SCHEME.get_or_init(query_background_scheme);
}

/// 当前终端的颜色能力。
pub fn capability() -> ColorSupport {
    capability_from_env(env_var)
}

fn env_var(name: &str) -> Option<String> {
    std::env::var(name).ok()
}

/// 颜色能力判定（按 `TERM_PROGRAM` 注入 env 读取器，便于测试）。
pub fn capability_from_env(get: impl Fn(&str) -> Option<String>) -> ColorSupport {
    if is_apple_terminal(&get) {
        ColorSupport::Ansi
    } else {
        ColorSupport::TrueColor
    }
}

fn is_apple_terminal(get: &impl Fn(&str) -> Option<String>) -> bool {
    get("TERM_PROGRAM").is_some_and(|v| v.eq_ignore_ascii_case("Apple_Terminal"))
}

/// 能力 + 明暗 → 具体主题。`default` 与外向偏好（daltonized）不参与自动选择。
pub fn theme_for(capability: ColorSupport, scheme: Scheme) -> ThemeName {
    match (capability, scheme) {
        (ColorSupport::Ansi, Scheme::Light) => ThemeName::LightAnsi,
        (ColorSupport::Ansi, Scheme::Dark) => ThemeName::DarkAnsi,
        (ColorSupport::TrueColor, Scheme::Light) => ThemeName::Light,
        (ColorSupport::TrueColor, Scheme::Dark) => ThemeName::Dark,
    }
}

/// 背景明暗：OSC 11 缓存优先，其次平台回退，最后按深色处理。
pub fn scheme() -> Scheme {
    if let Some(cached) = OSC_SCHEME.get().copied().flatten() {
        return cached;
    }
    // macOS 读系统外观判定；非 macOS 没有系统外观可读，一律按深色兜底。
    #[cfg(target_os = "macos")]
    {
        macos_appearance_scheme().unwrap_or(Scheme::Dark)
    }
    #[cfg(not(target_os = "macos"))]
    {
        Scheme::Dark
    }
}

/// 按相对亮度判明暗（sRGB 亮度权重，阈值 0.5）。
///
/// 1/2 个分量按灰度处理（NSColor 的 `NSWhite` 是「白度 + alpha」），3/4 个按 RGB(A)。
pub fn scheme_from_components(components: &[f32]) -> Option<Scheme> {
    let (r, g, b) = match components {
        [w] | [w, _] => (*w, *w, *w),
        [r, g, b] => (*r, *g, *b),
        [r, g, b, _a] => (*r, *g, *b),
        _ => return None,
    };
    if [r, g, b].iter().any(|v| !v.is_finite() || *v < 0.0) {
        return None;
    }
    let luminance = 0.2126 * r + 0.7152 * g + 0.0722 * b;
    Some(if luminance < 0.5 {
        Scheme::Dark
    } else {
        Scheme::Light
    })
}

/// 解析 OSC 11 回复（`\x1b]11;rgb:RRRR/GGGG/BBBB\x07`，接受 8/16 bit 分量）。
pub fn parse_osc11(reply: &str) -> Option<Scheme> {
    let start = reply.find("11;")? + 3;
    let body = &reply[start..];
    // 终止符有两种：BEL（`\x07`）或 ST（`\x1b\\`），ST 需同时剥掉 ESC 与反斜杠。
    let body = body.trim_end_matches(['\u{7}', '\u{1b}', '\\']);
    let rgb = body.strip_prefix("rgb:")?;
    let mut components = Vec::new();
    for raw in rgb.split('/') {
        let raw = raw.trim();
        if raw.is_empty() || raw.len() > 4 || !raw.chars().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }
        let value = u32::from_str_radix(raw, 16).ok()? as f32;
        let max = (16u32.pow(raw.len() as u32) - 1) as f32;
        components.push(value / max);
    }
    scheme_from_components(&components)
}

// ── 平台探测 ─────────────────────────────────────────────────────────────────

/// macOS 系统外观（浅色 / 深色）。
#[cfg(target_os = "macos")]
fn macos_appearance_scheme() -> Option<Scheme> {
    let output = std::process::Command::new("defaults")
        .args(["read", "-g", "AppleInterfaceStyle"])
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    Some(scheme_from_appearance(
        &String::from_utf8_lossy(&output.stdout),
        output.status.success(),
    ))
}

/// 解析 `defaults read -g AppleInterfaceStyle` 的结果。
///
/// macOS 只在深色模式写这个键（值为 `Dark`）；浅色模式下键不存在，命令以非零码退出、
/// 不产出任何输出。因此「命令跑起来了但没拿到 Dark」就按浅色处理。
pub fn scheme_from_appearance(stdout: &str, succeeded: bool) -> Scheme {
    if succeeded && stdout.trim().eq_ignore_ascii_case("dark") {
        Scheme::Dark
    } else {
        Scheme::Light
    }
}

/// OSC 11 查询：写入查询序列后在 stdin 上等 120ms 收回复。
///
/// 只在 `preload()` 里调用一次，且必须早于事件读取器启动，否则回复会被事件读取器消费。
#[cfg(unix)]
fn query_background_scheme() -> Option<Scheme> {
    use std::{io::Write, time::Duration, time::Instant};

    // stdin/stdout 不是终端（管道、重定向）时不查询：既问不到回复，读 stdin 还会
    // 吃掉用户通过管道喂进来的输入。
    // SAFETY: isatty 只查询 fd 状态，对任意 fd 都安全。
    let tty =
        unsafe { libc::isatty(libc::STDIN_FILENO) == 1 && libc::isatty(libc::STDOUT_FILENO) == 1 };
    if !tty {
        return None;
    }

    let mut stdout = std::io::stdout();
    stdout.write_all(b"\x1b]11;?\x07").ok()?;
    stdout.flush().ok()?;

    let mut reply = Vec::new();
    let deadline = Instant::now() + Duration::from_millis(120);
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }
        let mut poll_fd = libc::pollfd {
            fd: libc::STDIN_FILENO,
            events: libc::POLLIN,
            revents: 0,
        };
        // SAFETY: single valid pollfd, count 1.
        let ready = unsafe { libc::poll(&mut poll_fd, 1, remaining.as_millis() as i32) };
        if ready <= 0 {
            break;
        }
        let mut buf = [0u8; 64];
        // SAFETY: valid buffer pointer and length for a single read from stdin.
        let read = unsafe { libc::read(libc::STDIN_FILENO, buf.as_mut_ptr().cast(), buf.len()) };
        if read <= 0 {
            break;
        }
        reply.extend_from_slice(&buf[..read as usize]);
        if reply.ends_with(b"\x07") || reply.ends_with(b"\x1b\\") {
            break;
        }
    }
    parse_osc11(&String::from_utf8_lossy(&reply))
}

#[cfg(not(unix))]
fn query_background_scheme() -> Option<Scheme> {
    None
}

#[cfg(test)]
#[path = "mod_test.rs"]
mod tests;
