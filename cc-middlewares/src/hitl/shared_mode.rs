use std::sync::{
    atomic::{AtomicU8, Ordering},
    Arc,
};

/// 权限模式：**只剩两档**。
///
/// 原来的 `Default` / `DontAsk` / `AcceptEdit` 已移除——实测用户不愿意"天天确认"，
/// 这几种模式要么每次弹窗、要么半自动，体验都差且不智能。
///
/// - `AutoMode`（**默认**）：由 Jev 语义门按调用逐个判定；**只有判不准时才弹一次窗**
/// - `Bypass`：全部放行（危险，仅在用户明确要求时使用）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum PermissionMode {
    /// 大模型/语义门自动判断允不允许（默认）
    #[default]
    AutoMode = 0,
    /// 所有都允许
    Bypass = 1,
}

impl PermissionMode {
    /// 循环切换：`AutoMode ↔ Bypass`。
    pub fn next(self) -> Self {
        match self {
            Self::AutoMode => Self::Bypass,
            Self::Bypass => Self::AutoMode,
        }
    }

    /// 状态栏显示文本
    pub fn display_name(self) -> &'static str {
        match self {
            Self::AutoMode => "Auto",
            Self::Bypass => "Bypass",
        }
    }
}

/// `u8` → 模式。**未知值一律回退到 `AutoMode`**（默认档）：
/// 宁可走判定，也不要因为一个陈旧/异常的值意外滑进 `Bypass`。
///
/// 注：权限模式不落盘（只存在于内存 `AtomicU8` 与 ACP 字符串映射），
/// 因此不存在历史值兼容问题。
impl From<u8> for PermissionMode {
    fn from(value: u8) -> Self {
        match value {
            1 => Self::Bypass,
            _ => Self::AutoMode,
        }
    }
}

/// 跨线程共享的权限模式状态（Arc<AtomicU8> 封装）
pub struct SharedPermissionMode {
    inner: AtomicU8,
}

impl SharedPermissionMode {
    /// 创建新的共享权限模式实例，返回 Arc<Self>
    pub fn new(mode: PermissionMode) -> Arc<Self> {
        Arc::new(Self {
            inner: AtomicU8::new(mode as u8),
        })
    }

    /// 读取当前权限模式
    pub fn load(&self) -> PermissionMode {
        let v = self.inner.load(Ordering::Relaxed);
        PermissionMode::from(v)
    }

    /// 设置权限模式
    pub fn store(&self, mode: PermissionMode) {
        self.inner.store(mode as u8, Ordering::Relaxed);
    }

    /// CAS 循环切换到下一个模式，返回切换后的模式
    pub fn cycle(&self) -> PermissionMode {
        loop {
            let current = self.inner.load(Ordering::Relaxed);
            let current_mode = PermissionMode::from(current);
            let next_mode = current_mode.next();
            let next = next_mode as u8;
            match self
                .inner
                .compare_exchange(current, next, Ordering::Relaxed, Ordering::Relaxed)
            {
                Ok(_) => return next_mode,
                Err(_) => continue,
            }
        }
    }
}

#[cfg(test)]
#[path = "shared_mode_test.rs"]
mod tests;
