pub mod animation;
pub mod verb;

use std::time::Instant;

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Paragraph, Widget, WidgetRef},
};

use crate::theme::Theme;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpinnerMode {
    Thinking,
    ToolUse,
    Responding,
    Idle,
}

pub struct SpinnerState {
    mode: SpinnerMode,
    verb: String,
    start_time: Instant,
    token_count: usize,
    displayed_tokens: usize,
    tick: u64,
    raw_tick: u64,
    /// 最后一次从非 Idle 切换到 Idle 时捕获的耗时（ms），0 表示无记录
    last_summary_elapsed_ms: u64,
    /// 完成态总结行动词（随机过去式英文，如 "Cogitated"）
    last_summary_verb: String,
    /// 完成时刻（wall-clock），None 表示无记录
    last_summary_done_at: Option<std::time::SystemTime>,
    /// 随机动词列表（按语言选择）
    verb_list: &'static [&'static str],
    /// 当前思考段起点。None = 不在思考段中
    thinking_started_at: Option<Instant>,
    /// 上一段思考耗时（ms）。0 = 本回合尚无已结束的思考段
    last_thought_ms: u64,
    /// 本回合第几段思考（从 1 开始计）。用于 `thinking more` 判定
    thinking_round: u32,
    /// 最后一次 token 增长（或思考活动）的时刻。停滞转红的计时基准（对齐 CC 的 lastTokenTime）
    last_token_at: Instant,
    /// 停滞转红强度的平滑值（0..=1，每 50ms 向目标靠拢 10%，对齐 CC 的平滑规则）
    stall_intensity: f32,
    /// 上次推进停滞平滑的时刻
    stall_last_smooth_at: Instant,
}

impl SpinnerState {
    pub fn new(mode: SpinnerMode) -> Self {
        Self::with_lang(mode, "zh")
    }

    /// 按语言代码初始化 SpinnerState（自动匹配对应语言的动词列表）
    pub fn with_lang(mode: SpinnerMode, lang: &str) -> Self {
        let verb_list = verb::verbs_for_lang(lang);
        Self {
            mode,
            verb: verb::pick_verb_from(None, verb_list),
            start_time: Instant::now(),
            token_count: 0,
            displayed_tokens: 0,
            tick: 0,
            raw_tick: 0,
            last_summary_elapsed_ms: 0,
            last_summary_verb: String::new(),
            last_summary_done_at: None,
            verb_list,
            thinking_started_at: None,
            last_thought_ms: 0,
            thinking_round: 0,
            last_token_at: Instant::now(),
            stall_intensity: 0.0,
            stall_last_smooth_at: Instant::now(),
        }
    }

    /// 设置语言并同步更新动词列表
    pub fn set_language(&mut self, lang: &str) {
        self.verb_list = verb::verbs_for_lang(lang);
    }

    pub fn set_mode(&mut self, mode: SpinnerMode) {
        self.set_mode_with_label(mode, None);
    }

    /// 切换模式，可选传入翻译后的 label 覆盖默认中文。
    pub fn set_mode_with_label(&mut self, mode: SpinnerMode, label: Option<String>) {
        let was_active = self.mode != SpinnerMode::Idle;
        self.mode = mode;
        self.verb = match (&self.mode, label) {
            (SpinnerMode::Thinking, Some(l)) => l,
            (SpinnerMode::Thinking, None) => "思考中…".to_string(),
            (SpinnerMode::ToolUse, Some(l)) => l,
            (SpinnerMode::ToolUse, None) => "执行工具…".to_string(),
            (SpinnerMode::Responding, Some(l)) => l,
            (SpinnerMode::Responding, None) => "正在生成回复…".to_string(),
            (SpinnerMode::Idle, _) => String::new(),
        };
        // 从活跃状态切换到 Idle 时，记录耗时、随机完成动词和完成时刻
        if was_active && self.mode == SpinnerMode::Idle {
            self.last_summary_elapsed_ms = self.elapsed_ms();
            self.last_summary_verb = verb::pick_summary_verb();
            self.last_summary_done_at = Some(std::time::SystemTime::now());
        }
        // 从 Idle 切换到活跃状态时，重置计时器和总结记录
        if !was_active && self.mode != SpinnerMode::Idle {
            self.start_time = Instant::now();
            self.last_summary_elapsed_ms = 0;
            self.last_summary_verb = String::new();
            self.last_summary_done_at = None;
            self.last_token_at = Instant::now();
            self.stall_intensity = 0.0;
            self.stall_last_smooth_at = Instant::now();
        }
    }

    pub fn set_verb(&mut self, active_form: Option<&str>) {
        self.verb = verb::pick_verb_from(active_form, self.verb_list);
    }

    /// 方案 A：回合开始时随机选定一个动词，整轮固定。
    pub fn pick_round_verb(&mut self) {
        self.verb = verb::pick_round_verb(self.verb_list);
    }

    /// 切换模式但**保留当前 verb**（方案 A 用：工具/回复/思考切换时不换词）。
    /// 进入 `Idle` 时不保留（Idle 无 verb）；其余模式恢复选中词。
    pub fn set_mode_keep_verb(&mut self, mode: SpinnerMode) {
        if mode == SpinnerMode::Idle {
            self.set_mode_with_label(SpinnerMode::Idle, None);
            return;
        }
        let saved_verb = self.verb.clone();
        self.set_mode_with_label(mode, None);
        self.verb = saved_verb;
    }

    /// 设置随机动词列表（按语言切换时调用）
    pub fn set_verb_list(&mut self, verb_list: &'static [&'static str]) {
        self.verb_list = verb_list;
    }

    pub fn set_token_count(&mut self, count: usize) {
        // token 增长即「有新输出」：刷新停滞计时并清零转红强度（对齐 CC 的新 token 重置）
        if count > self.token_count {
            self.last_token_at = Instant::now();
            self.stall_intensity = 0.0;
            self.stall_last_smooth_at = Instant::now();
        }
        self.token_count = count;
    }

    pub fn advance_tick(&mut self) {
        self.raw_tick = self.raw_tick.wrapping_add(1);
        self.displayed_tokens =
            animation::smooth_increment(self.displayed_tokens, self.token_count);
        // 每 2 个 raw tick 才推进一帧（星号旋转更快）
        if self.raw_tick.is_multiple_of(2) {
            self.tick += 1;
        }
        self.advance_stall();
    }

    /// 推进停滞转红的平滑强度（对齐 CC 2.1.300：每 50ms 一步、向目标靠拢 10%）。
    /// 工具活跃执行期间持续刷新计时基准，永不进入停滞。
    fn advance_stall(&mut self) {
        let has_active_tools = self.mode == SpinnerMode::ToolUse;
        if has_active_tools {
            self.last_token_at = Instant::now();
        }
        let target = animation::stall_intensity_target(self.time_since_last_token_ms(), has_active_tools);
        let now = Instant::now();
        if target <= 0.0 && self.stall_intensity <= 0.0 {
            self.stall_last_smooth_at = now;
            return;
        }
        let dt_ms = now.duration_since(self.stall_last_smooth_at).as_millis() as u64;
        if dt_ms >= 50 {
            let steps = dt_ms / 50;
            let mut cur = self.stall_intensity;
            for _ in 0..steps {
                let diff = target - cur;
                if diff.abs() < 0.01 {
                    cur = target;
                    break;
                }
                cur += diff * 0.1;
            }
            self.stall_intensity = cur;
            self.stall_last_smooth_at = now;
        }
    }

    /// 距最后一次 token 活动已过去的毫秒数
    pub fn time_since_last_token_ms(&self) -> u64 {
        self.last_token_at.elapsed().as_millis() as u64
    }

    /// 当前停滞转红强度（0..=1，平滑后）
    pub fn stalled_intensity(&self) -> f32 {
        self.stall_intensity
    }

    pub fn elapsed_ms(&self) -> u64 {
        self.start_time.elapsed().as_millis() as u64
    }

    // ── 思考段追踪（Thinking 状态行用）──────────────────────────────

    /// 进入一段新的思考。重复调用（已在思考中）**不重置起点**，
    /// 避免流式 reasoning chunk 反复到达时把计时清零。
    /// 仅当从「非思考」切换到「思考」时，递增 `thinking_round`。
    pub fn begin_thinking(&mut self) {
        // 思考事件到达本身即模型活动信号（reasoning 流不一定计入 token_count），刷新停滞计时
        self.last_token_at = Instant::now();
        if self.thinking_started_at.is_none() {
            self.thinking_started_at = Some(Instant::now());
            self.thinking_round += 1;
        }
    }

    /// 结束当前思考段，把耗时记入 `last_thought_ms`。
    /// 若不在思考中则无操作（幂等）。
    pub fn end_thinking(&mut self) {
        if let Some(start) = self.thinking_started_at.take() {
            self.last_thought_ms = start.elapsed().as_millis() as u64;
        }
    }

    /// 是否正处于思考段中
    pub fn is_thinking(&self) -> bool {
        self.thinking_started_at.is_some()
    }

    /// 当前思考段已持续毫秒数。不在思考中返回 0。
    pub fn thinking_elapsed_ms(&self) -> u64 {
        self.thinking_started_at
            .map(|t| t.elapsed().as_millis() as u64)
            .unwrap_or(0)
    }

    /// 上一段已结束的思考耗时（ms）。0 = 本回合尚无已结束的思考段。
    pub fn last_thought_ms(&self) -> u64 {
        self.last_thought_ms
    }

    /// 本回合第几段思考（从 1 开始）。0 = 尚未开始任何思考段。
    /// `>= 2` 表示「本轮已产出过、再次思考」。
    pub fn thinking_round(&self) -> u32 {
        self.thinking_round
    }

    /// 重置思考追踪（回合开始时调用）：清空轮次与上一段耗时。
    /// 注意：`reset()` 会做同样的事，但它同时清空 verb/mode；
    /// 本方法只清思考追踪，不影响 verb（方案 A 的回合词）。
    pub fn reset_thinking_tracking(&mut self) {
        self.thinking_started_at = None;
        self.last_thought_ms = 0;
        self.thinking_round = 0;
    }

    pub fn tick(&self) -> u64 {
        self.tick
    }

    pub fn raw_tick(&self) -> u64 {
        self.raw_tick
    }

    /// 返回当前 spinner frame 字符，用于终端标题动画
    pub fn title_frame(&self) -> char {
        animation::tick_to_frame(self.tick)
    }

    pub fn verb(&self) -> &str {
        &self.verb
    }

    pub fn verb_list(&self) -> &'static [&'static str] {
        self.verb_list
    }

    pub fn mode(&self) -> &SpinnerMode {
        &self.mode
    }

    pub fn last_summary_elapsed_ms(&self) -> u64 {
        self.last_summary_elapsed_ms
    }

    pub fn last_summary_verb(&self) -> &str {
        &self.last_summary_verb
    }

    pub fn last_summary_done_at(&self) -> Option<std::time::SystemTime> {
        self.last_summary_done_at
    }

    /// 清空完成态总结行（用于 error/interrupt 路径，避免失败任务显示成功文案）
    pub fn clear_summary(&mut self) {
        self.last_summary_elapsed_ms = 0;
        self.last_summary_verb = String::new();
        self.last_summary_done_at = None;
    }

    /// 从持久化数据恢复完成态总结行（`-c`/`-r` 恢复会话时使用）。
    ///
    /// 与 [`Self::set_mode`] 的 Idle 切换逻辑产出的状态一致：verb + 耗时 + 完成时刻。
    pub fn restore_summary(
        &mut self,
        verb: String,
        elapsed_ms: u64,
        done_at: std::time::SystemTime,
    ) {
        self.last_summary_elapsed_ms = elapsed_ms;
        self.last_summary_verb = verb;
        self.last_summary_done_at = Some(done_at);
    }

    pub fn displayed_tokens(&self) -> usize {
        self.displayed_tokens
    }

    /// 重置所有字段到初始状态（保留 verb_list）
    pub fn reset(&mut self) {
        self.mode = SpinnerMode::Idle;
        self.verb = String::new();
        self.start_time = Instant::now();
        self.token_count = 0;
        self.displayed_tokens = 0;
        self.tick = 0;
        self.raw_tick = 0;
        self.last_summary_elapsed_ms = 0;
        self.last_summary_verb = String::new();
        self.last_summary_done_at = None;
        self.thinking_started_at = None;
        self.last_thought_ms = 0;
        self.thinking_round = 0;
        self.last_token_at = Instant::now();
        self.stall_intensity = 0.0;
        self.stall_last_smooth_at = Instant::now();
    }
}

pub struct SpinnerWidget<'a> {
    state: &'a SpinnerState,
    show_elapsed: bool,
    show_tokens: bool,
    primary_color: Color,
    secondary_color: Color,
    shimmer_color: Color,
    error_color: Color,
}

impl<'a> SpinnerWidget<'a> {
    pub fn new(state: &'a SpinnerState) -> Self {
        Self {
            state,
            show_elapsed: true,
            show_tokens: true,
            primary_color: crate::theme::DarkTheme.accent(),
            secondary_color: crate::theme::DarkTheme.muted(),
            shimmer_color: crate::theme::DarkTheme.loading(),
            error_color: crate::theme::DarkTheme.error(),
        }
    }

    pub fn show_elapsed(mut self, show: bool) -> Self {
        self.show_elapsed = show;
        self
    }

    pub fn show_tokens(mut self, show: bool) -> Self {
        self.show_tokens = show;
        self
    }

    pub fn theme_colors(mut self, primary: Color, secondary: Color) -> Self {
        self.primary_color = primary;
        self.secondary_color = secondary;
        self
    }

    /// 设置流光色（与主色成对，对齐 CC 的 base/shimmer 色对）。
    pub fn shimmer_color(mut self, color: Color) -> Self {
        self.shimmer_color = color;
        self
    }

    /// 从 `Theme` trait 派生 spinner 颜色，替代硬编码默认值。
    pub fn with_theme(mut self, theme: &dyn Theme) -> Self {
        self.primary_color = theme.accent();
        self.secondary_color = theme.muted();
        self.shimmer_color = theme.loading();
        self.error_color = theme.error();
        self
    }
}

impl WidgetRef for SpinnerWidget<'_> {
    fn render_ref(&self, area: Rect, buf: &mut Buffer) {
        let mut spans: Vec<Span<'_>> = vec![];

        let frame = animation::tick_to_frame(self.state.tick());
        let orange = Style::default().fg(self.primary_color);
        let gray = Style::default().fg(self.secondary_color);

        spans.push(Span::styled(format!("{} ", frame), orange));

        // 停滞时流光熄火，整词向 error 色插值（对齐 CC 的 stalled 渲染分支）
        let stall = self.state.stalled_intensity();
        if stall > 0.0 {
            let color = animation::blend_color(self.primary_color, self.error_color, stall);
            spans.push(Span::styled(
                self.state.verb().to_string(),
                Style::default().fg(color),
            ));
        } else {
            let verb_spans = animation::shimmer_verb_spans(
                self.state.verb(),
                self.state.elapsed_ms(),
                self.primary_color,
                self.shimmer_color,
            );
            spans.extend(verb_spans);
        }

        let elapsed = self.state.elapsed_ms();
        let displayed_tokens = self.state.displayed_tokens();

        let mut suffix_parts = Vec::new();

        if self.show_elapsed {
            suffix_parts.push(animation::format_elapsed(elapsed));
        }

        if self.show_tokens && displayed_tokens > 0 {
            suffix_parts.push(format!(
                "↓ {} tokens",
                animation::format_tokens(displayed_tokens)
            ));
        }

        if !suffix_parts.is_empty() {
            spans.push(Span::styled(
                format!(" ({}", suffix_parts.join(" · ")),
                gray,
            ));
            spans.push(Span::styled(")", gray));
        }

        Paragraph::new(Line::from(spans)).render(area, buf);
    }
}

impl Widget for SpinnerWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        self.render_ref(area, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spinner_summary_lifecycle_and_clear() {
        let mut state = SpinnerState::new(SpinnerMode::Idle);
        assert_eq!(state.last_summary_elapsed_ms(), 0);
        assert!(state.last_summary_verb().is_empty());
        assert!(state.last_summary_done_at().is_none());

        state.set_mode(SpinnerMode::Responding);
        assert_eq!(state.last_summary_elapsed_ms(), 0);

        state.set_mode(SpinnerMode::Idle);
        assert!(!state.last_summary_verb().is_empty());
        assert!(state.last_summary_done_at().is_some());

        state.clear_summary();
        assert_eq!(state.last_summary_elapsed_ms(), 0);
        assert!(state.last_summary_verb().is_empty());
        assert!(state.last_summary_done_at().is_none());
    }

    #[test]
    fn test_restore_summary_reproduces_completed_line() {
        // Arrange：重置后（模拟 open_thread 的 reset_agent_session）应无总结行
        let mut state = SpinnerState::new(SpinnerMode::Idle);
        state.reset();
        assert_eq!(state.last_summary_elapsed_ms(), 0);

        // Act：从持久化数据恢复
        let done_at = std::time::SystemTime::now();
        state.restore_summary("Cooked".to_string(), 25_000, done_at);

        // Assert：三个字段均恢复
        assert_eq!(state.last_summary_elapsed_ms(), 25_000);
        assert_eq!(state.last_summary_verb(), "Cooked");
        assert_eq!(state.last_summary_done_at(), Some(done_at));
    }

    #[test]
    fn test_begin_thinking_is_idempotent() {
        // Arrange
        let mut state = SpinnerState::new(SpinnerMode::Idle);
        assert!(!state.is_thinking(), "初始不应在思考中");
        assert_eq!(state.thinking_round(), 0);

        // Act：连续三次 begin（模拟流式 reasoning chunk 反复到达）
        state.begin_thinking();
        let first_start = state.thinking_started_at;
        state.begin_thinking();
        state.begin_thinking();

        // Assert：起点不变、轮次只加一次
        assert!(state.is_thinking());
        assert_eq!(state.thinking_round(), 1, "重复 begin 不应重复计数");
        assert_eq!(
            state.thinking_started_at, first_start,
            "重复 begin 不应重置计时起点"
        );
    }

    #[test]
    fn test_end_thinking_records_duration_and_multiple_rounds() {
        // Arrange
        let mut state = SpinnerState::new(SpinnerMode::Responding);
        assert_eq!(state.last_thought_ms(), 0);

        // Act：第一段思考
        state.begin_thinking();
        std::thread::sleep(std::time::Duration::from_millis(12));
        state.end_thinking();

        // Assert：耗时被记录，轮次为 1，退出思考态
        assert!(!state.is_thinking(), "end 后应退出思考态");
        assert!(state.last_thought_ms() >= 10, "应记录第一段耗时");
        assert_eq!(state.thinking_round(), 1);

        // Act：第二段思考（模拟被打断后再次思考）
        state.begin_thinking();

        // Assert：轮次递增到 2（供 `thinking more` 判定）
        assert_eq!(state.thinking_round(), 2, "第二段思考轮次应为 2");
    }

    #[test]
    fn test_end_thinking_without_begin_is_noop() {
        // Arrange：不在思考中
        let mut state = SpinnerState::new(SpinnerMode::Idle);

        // Act：直接 end（未 begin）
        state.end_thinking();

        // Assert：无副作用
        assert_eq!(state.last_thought_ms(), 0);
        assert_eq!(state.thinking_round(), 0);
        assert!(!state.is_thinking());
    }

    #[test]
    fn test_reset_clears_thinking_tracking() {
        // Arrange：进入思考两轮
        let mut state = SpinnerState::new(SpinnerMode::Responding);
        state.begin_thinking();
        state.end_thinking();
        state.begin_thinking();
        assert_eq!(state.thinking_round(), 2);

        // Act
        state.reset();

        // Assert
        assert_eq!(state.thinking_round(), 0);
        assert_eq!(state.last_thought_ms(), 0);
        assert!(!state.is_thinking());
    }

    #[test]
    fn test_thinking_elapsed_ms_returns_zero_when_not_thinking() {
        // Arrange
        let mut state = SpinnerState::new(SpinnerMode::Idle);
        assert_eq!(state.thinking_elapsed_ms(), 0);

        // Act
        state.begin_thinking();
        std::thread::sleep(std::time::Duration::from_millis(12));

        // Assert：思考中返回实际耗时
        assert!(state.thinking_elapsed_ms() >= 10);

        // Act：结束思考后
        state.end_thinking();

        // Assert：回到 0
        assert_eq!(state.thinking_elapsed_ms(), 0);
    }

    #[test]
    fn test_with_lang_and_set_language() {
        let en_state = SpinnerState::with_lang(SpinnerMode::Idle, "en");
        let zh_state = SpinnerState::with_lang(SpinnerMode::Idle, "zh-CN");
        assert_eq!(en_state.verb_list(), verb::EN_VERBS);
        assert_eq!(zh_state.verb_list(), verb::ZH_VERBS);
        let mut switch_state = en_state;
        switch_state.set_language("zh");
        assert_eq!(switch_state.verb_list(), verb::ZH_VERBS);
    }
}
