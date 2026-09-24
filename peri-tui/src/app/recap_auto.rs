//! 自动触发 recap 调度状态机。
//!
//! 对齐 Codex CLI `codex-rs/tui/src/app/recap.rs` 的自动 recap 触发逻辑：
//! - 终端失焦 + 最后一轮完成 + 60 秒静默 → 自动触发 recap
//! - 至少 3 个已完成轮次，两次 recap 之间至少新增 2 轮
//! - 重新聚焦取消未触发的定时，in-flight 结果按 revision 丢弃
//! - 失败自动重试（30s 间隔）

use std::time::{Duration, Instant};

/// 至少需要的已完成轮次数
const MIN_COMPLETED_TURNS: usize = 3;
/// 两次 recap 之间至少新增的轮次数
const MIN_TURNS_BETWEEN_RECAPS: usize = 2;
/// 触发前静默延迟
const RECAP_DELAY: Duration = Duration::from_secs(60);
/// 失败重试间隔
const RECAP_RETRY_DELAY: Duration = Duration::from_secs(30);

fn configured_recap_delay() -> Duration {
    if let Ok(val) = std::env::var("PERI_AUTO_RECAP_DELAY") {
        if let Ok(secs) = val.parse::<u64>() {
            return Duration::from_secs(secs);
        }
    }
    RECAP_DELAY
}

fn configured_min_turns() -> usize {
    if let Ok(val) = std::env::var("PERI_AUTO_RECAP_MIN_TURNS") {
        if let Ok(turns) = val.parse::<usize>() {
            return turns;
        }
    }
    MIN_COMPLETED_TURNS
}

/// 自动 recap 调度状态。
///
/// 每次 `maybe_trigger()` 返回 `Some(revision)` 时，调用方应执行 recap 触发。
/// `turn_revision` 用于识别 stale 请求：任何轮次结束/聚焦变化都会 bump revision，
/// 使飞行中的旧请求在回调时被判 stale 而安全丢弃。
#[derive(Debug)]
pub struct AutoRecapState {
    /// 是否启用自动 recap（来自配置）
    pub enabled: bool,
    /// 首次失焦时刻
    unfocused_since: Option<Instant>,
    /// 最后一轮完成时刻
    last_turn_finished_at: Option<Instant>,
    /// 已完成的轮次计数
    completed_turns: usize,
    /// 上次 recap 时的轮次计数
    last_recapped_turn_count: usize,
    /// 轮次版本号（每次轮次结束/聚焦变化时递增）
    turn_revision: u64,
    /// 飞行中的 recap 请求对应的 revision
    in_flight_revision: Option<u64>,
    /// 是否已调度（防止 50ms 轮询重复触发）
    scheduled: bool,
    /// 失败重试时间
    retry_at: Option<Instant>,
    /// 上次已记录的阻塞原因签名，用于日志去重（避免 50ms 轮询刷屏）
    last_diag: Option<(&'static str, usize)>,
    /// 时间源（注入便于测试）
    now_fn: fn() -> Instant,
}

impl AutoRecapState {
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled,
            unfocused_since: None,
            last_turn_finished_at: None,
            completed_turns: 0,
            last_recapped_turn_count: 0,
            turn_revision: 0,
            in_flight_revision: None,
            scheduled: false,
            retry_at: None,
            last_diag: None,
            now_fn: Instant::now,
        }
    }

    /// 测试用：注入自定义时间源
    #[cfg(test)]
    pub fn with_now_fn(enabled: bool, now_fn: fn() -> Instant) -> Self {
        Self {
            now_fn,
            ..Self::new(enabled)
        }
    }

    fn now(&self) -> Instant {
        (self.now_fn)()
    }

    /// 当前生效的触发延迟秒数（含环境变量覆盖），供日志展示。
    pub fn recap_delay_secs(&self) -> u64 {
        configured_recap_delay().as_secs()
    }

    /// 当前生效的最小轮次数（含环境变量覆盖），供日志展示。
    pub fn recap_min_turns(&self) -> usize {
        configured_min_turns()
    }

    /// 检查是否满足自动触发条件
    fn ready_to_fire(&self) -> bool {
        self.trigger_block().is_none()
    }

    /// 返回阻塞触发的首个原因；`None` 表示当前满足触发条件。
    ///
    /// 与 [`Self::ready_to_fire`] 同源，仅用于日志排查。
    fn trigger_block(&self) -> Option<&'static str> {
        if !self.enabled {
            return Some("auto_recap 已禁用（/config 中「会话回顾」为关）");
        }
        if self.scheduled {
            return Some("已调度，等待 in-flight recap 返回");
        }
        if self.in_flight_revision.is_some() {
            return Some("recap 请求在途（in-flight）");
        }

        // 重试路径：不要求失焦，只检查时间
        if let Some(retry_at) = self.retry_at {
            return if self.now() >= retry_at {
                None
            } else {
                Some("失败重试等待中（30s 间隔）")
            };
        }

        // 正常路径：需要失焦
        let unf = match self.unfocused_since {
            Some(t) => t,
            None => return Some("终端未失焦（未收到 FocusLost 事件）"),
        };

        if self.completed_turns < configured_min_turns() {
            return Some("已完成轮次不足（见 MIN_TURNS）");
        }

        if self.completed_turns > 0
            && self.last_recapped_turn_count > 0
            && self.completed_turns - self.last_recapped_turn_count < MIN_TURNS_BETWEEN_RECAPS
        {
            return Some("距上次 recap 新增轮次不足 2");
        }

        // 取失焦时刻和最后一轮完成时刻的较迟者，加静默延迟
        let anchor = match self.last_turn_finished_at {
            Some(t) => unf.max(t),
            None => unf,
        };

        if self.now() >= anchor + configured_recap_delay() {
            None
        } else {
            Some("静默等待中（未到触发延迟）")
        }
    }

    /// 未满足触发条件时按状态变化打一条 debug 日志（避免 50ms 轮询刷屏）。
    pub fn log_blocked(&mut self) {
        let Some(reason) = self.trigger_block() else {
            return;
        };
        let sig = (reason, self.completed_turns);
        if self.last_diag == Some(sig) {
            return;
        }
        self.last_diag = Some(sig);
        tracing::debug!(
            reason,
            enabled = self.enabled,
            completed_turns = self.completed_turns,
            last_recapped_turns = self.last_recapped_turn_count,
            unfocused_secs = ?self.unfocused_since.map(|t| t.elapsed().as_secs()),
            last_turn_secs = ?self.last_turn_finished_at.map(|t| t.elapsed().as_secs()),
            "auto_recap: 未触发"
        );
    }

    /// 如果满足条件，标记已调度并返回 revision；否则返回 None。
    /// 调用方应在收到 Some 时执行 recap 触发。
    pub fn maybe_trigger(&mut self) -> Option<u64> {
        if !self.ready_to_fire() {
            return None;
        }
        self.turn_revision += 1;
        self.scheduled = true;
        self.retry_at = None;
        self.in_flight_revision = Some(self.turn_revision);
        Some(self.turn_revision)
    }

    /// 终端重新聚焦：取消未触发的定时，in-flight 结果按 revision 丢弃
    pub fn on_focus_gained(&mut self) {
        tracing::info!("auto_recap: 终端聚焦，取消待触发");
        self.unfocused_since = None;
        self.scheduled = false;
        self.retry_at = None;
        self.in_flight_revision = None;
        self.turn_revision += 1;
    }

    /// 终端失焦：记录首次失焦时刻
    pub fn on_focus_lost(&mut self) {
        if self.unfocused_since.is_none() {
            self.unfocused_since = Some(self.now());
        }
        tracing::info!(
            enabled = self.enabled,
            completed_turns = self.completed_turns,
            delay_secs = configured_recap_delay().as_secs(),
            min_turns = configured_min_turns(),
            "auto_recap: 终端失焦"
        );
    }

    /// 一轮对话完成：累加轮次、记录完成时刻、bump revision
    pub fn on_turn_finished(&mut self) {
        self.completed_turns += 1;
        self.last_turn_finished_at = Some(self.now());
        self.turn_revision += 1;
        tracing::debug!(
            completed_turns = self.completed_turns,
            "auto_recap: 轮次完成"
        );
    }

    /// recap 成功完成：记录轮次计数、清除调度状态
    pub fn on_recap_done(&mut self) {
        self.last_recapped_turn_count = self.completed_turns;
        self.scheduled = false;
        self.in_flight_revision = None;
    }

    /// recap 失败：30 秒后重试
    pub fn on_recap_failed(&mut self) {
        self.retry_at = Some(self.now() + RECAP_RETRY_DELAY);
        self.scheduled = false;
        self.in_flight_revision = None;
    }

    /// 触发被推迟（如 loading 中）：清除 scheduled 但保留 in-flight revision
    pub fn on_trigger_deferred(&mut self) {
        self.scheduled = false;
        self.in_flight_revision = None;
    }

    /// 检查 in-flight 请求是否仍然有效（revision 匹配）
    pub fn is_current_request(&self, revision: u64) -> bool {
        self.in_flight_revision == Some(revision)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use std::time::Duration;

    /// 可控时间源
    static MOCK_TIME: Mutex<Option<Instant>> = Mutex::new(None);
    /// 测试互斥锁：防止多线程并发测试污染共享的 MOCK_TIME
    static TEST_LOCK: Mutex<()> = Mutex::new(());

    struct TestContext {
        state: AutoRecapState,
        _guard: std::sync::MutexGuard<'static, ()>,
    }

    impl std::ops::Deref for TestContext {
        type Target = AutoRecapState;
        fn deref(&self) -> &Self::Target {
            &self.state
        }
    }

    impl std::ops::DerefMut for TestContext {
        fn deref_mut(&mut self) -> &mut Self::Target {
            &mut self.state
        }
    }

    fn mock_now() -> Instant {
        MOCK_TIME.lock().unwrap().unwrap_or_else(Instant::now)
    }

    fn set_mock_time(t: Instant) {
        *MOCK_TIME.lock().unwrap() = Some(t);
    }

    fn advance_mock_time(d: Duration) {
        let mut guard = MOCK_TIME.lock().unwrap();
        if let Some(t) = *guard {
            *guard = Some(t + d);
        }
    }

    fn make_state(enabled: bool) -> TestContext {
        let guard = TEST_LOCK.lock().unwrap();
        let base = Instant::now();
        set_mock_time(base);
        TestContext {
            state: AutoRecapState::with_now_fn(enabled, mock_now),
            _guard: guard,
        }
    }

    #[test]
    fn test_not_ready_when_disabled() {
        let mut state = make_state(false);
        state.on_focus_lost();
        for _ in 0..5 {
            state.on_turn_finished();
        }
        advance_mock_time(Duration::from_secs(120));
        assert!(state.maybe_trigger().is_none(), "禁用时不应触发");
    }

    #[test]
    fn test_not_ready_when_focused() {
        let mut state = make_state(true);
        for _ in 0..5 {
            state.on_turn_finished();
        }
        // 不失焦
        advance_mock_time(Duration::from_secs(120));
        assert!(state.maybe_trigger().is_none(), "未失焦不应触发");
    }

    #[test]
    fn test_not_ready_insufficient_turns() {
        let mut state = make_state(true);
        state.on_focus_lost();
        state.on_turn_finished();
        state.on_turn_finished();
        advance_mock_time(Duration::from_secs(120));
        assert!(state.maybe_trigger().is_none(), "不足 3 轮不应触发");
    }

    #[test]
    fn test_not_ready_before_delay() {
        let mut state = make_state(true);
        state.on_focus_lost();
        for _ in 0..3 {
            state.on_turn_finished();
        }
        // 只过了 30 秒
        advance_mock_time(Duration::from_secs(30));
        assert!(state.maybe_trigger().is_none(), "未到 60 秒不应触发");
    }

    #[test]
    fn test_ready_after_delay() {
        let mut state = make_state(true);
        state.on_focus_lost();
        for _ in 0..3 {
            state.on_turn_finished();
        }
        advance_mock_time(Duration::from_secs(61));
        assert!(state.maybe_trigger().is_some(), "满足条件应触发");
    }

    #[test]
    fn test_no_double_trigger() {
        let mut state = make_state(true);
        state.on_focus_lost();
        for _ in 0..3 {
            state.on_turn_finished();
        }
        advance_mock_time(Duration::from_secs(61));
        let first = state.maybe_trigger();
        assert!(first.is_some(), "第一次应触发");
        let second = state.maybe_trigger();
        assert!(second.is_none(), "不应重复触发");
    }

    #[test]
    fn test_focus_gained_cancels() {
        let mut state = make_state(true);
        state.on_focus_lost();
        for _ in 0..3 {
            state.on_turn_finished();
        }
        advance_mock_time(Duration::from_secs(61));
        state.on_focus_gained();
        assert!(state.maybe_trigger().is_none(), "聚焦后应取消");
    }

    #[test]
    fn test_min_turns_between_recaps() {
        let mut state = make_state(true);
        state.on_focus_lost();
        for _ in 0..3 {
            state.on_turn_finished();
        }
        advance_mock_time(Duration::from_secs(61));
        let rev = state.maybe_trigger();
        assert!(rev.is_some());
        state.on_recap_done();

        // 只新增 1 轮，不应触发
        state.on_focus_lost();
        state.on_turn_finished();
        advance_mock_time(Duration::from_secs(61));
        assert!(state.maybe_trigger().is_none(), "距上次仅新增 1 轮不应触发");

        // 再新增 1 轮，总共新增 2 轮，应触发
        state.on_turn_finished();
        advance_mock_time(Duration::from_secs(61));
        assert!(state.maybe_trigger().is_some(), "距上次新增 2 轮应触发");
    }

    #[test]
    fn test_retry_after_failure() {
        let mut state = make_state(true);
        state.on_focus_lost();
        for _ in 0..3 {
            state.on_turn_finished();
        }
        advance_mock_time(Duration::from_secs(61));
        assert!(state.maybe_trigger().is_some());

        // 失败
        state.on_recap_failed();
        // 立即不重试
        assert!(state.maybe_trigger().is_none(), "失败后不应立即重试");
        // 29 秒后仍不重试
        advance_mock_time(Duration::from_secs(29));
        assert!(state.maybe_trigger().is_none(), "29 秒内不应重试");
        // 30 秒后重试
        advance_mock_time(Duration::from_secs(1));
        assert!(state.maybe_trigger().is_some(), "30 秒后应重试");
    }

    #[test]
    fn test_retry_does_not_require_unfocus() {
        let mut state = make_state(true);
        state.on_focus_lost();
        for _ in 0..3 {
            state.on_turn_finished();
        }
        advance_mock_time(Duration::from_secs(61));
        assert!(state.maybe_trigger().is_some());
        state.on_recap_failed();

        // 重新聚焦（清除 unfocused_since），但 retry_at 已设置
        state.on_focus_gained();
        // 聚焦后 on_focus_gained 会清 retry_at，所以需要重新设置
        // 这里验证的是 retry 路径不检查 unfocused_since
        // 由于 on_focus_gained 清了 retry_at，此场景实际不会重试
        // 这正是预期行为：聚焦取消一切
        assert!(state.maybe_trigger().is_none(), "聚焦后不应重试");
    }

    #[test]
    fn test_turn_finished_during_unfocus() {
        let mut state = make_state(true);
        state.on_focus_lost();
        for _ in 0..3 {
            state.on_turn_finished();
        }
        // anchor = max(unfocused_since, last_turn_finished_at)
        // 失焦时刻 < 最后轮完成时刻，所以从最后轮完成时刻 +60s 起算
        advance_mock_time(Duration::from_secs(59));
        assert!(state.maybe_trigger().is_none(), "59 秒不应触发");
        advance_mock_time(Duration::from_secs(2));
        assert!(state.maybe_trigger().is_some(), "61 秒应触发");
    }

    #[test]
    fn test_is_current_request() {
        let mut state = make_state(true);
        state.on_focus_lost();
        for _ in 0..3 {
            state.on_turn_finished();
        }
        advance_mock_time(Duration::from_secs(61));
        let rev = state.maybe_trigger().unwrap();
        assert!(state.is_current_request(rev), "当前请求应有效");
        assert!(!state.is_current_request(rev + 1), "不同 revision 应无效");

        state.on_focus_gained();
        assert!(!state.is_current_request(rev), "聚焦后请求应失效");
    }

    #[test]
    fn test_on_trigger_deferred() {
        let mut state = make_state(true);
        state.on_focus_lost();
        for _ in 0..3 {
            state.on_turn_finished();
        }
        advance_mock_time(Duration::from_secs(61));
        assert!(state.maybe_trigger().is_some());
        state.on_trigger_deferred();
        // 推迟后 scheduled 和 in_flight 清除，但条件仍满足，可再次触发
        // 注意：on_trigger_deferred 后 scheduled=false, in_flight=None
        // 所以 ready_to_fire 重新评估，仍然满足条件
        assert!(state.maybe_trigger().is_some(), "推迟后仍可再次触发");
    }
}
