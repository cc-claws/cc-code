use crate::{app::App, command::Command};

pub struct GcCommand;

impl Command for GcCommand {
    fn name(&self) -> &str {
        "gc"
    }

    fn description(&self, _lc: &crate::i18n::LcRegistry) -> String {
        "手动触发内存回收并显示 RSS 变化".to_string()
    }

    fn aliases(&self) -> Vec<&str> {
        vec!["memory"]
    }

    fn execute(&self, app: &mut App, _args: &str) {
        let rss_before = crate::mimalloc_config::query_rss();

        crate::mimalloc_config::alloc_collect();

        let rss_after = crate::mimalloc_config::query_rss();

        let msg = match (rss_before, rss_after) {
            (Some((before, _)), Some((after, peak))) => {
                let delta = before as isize - after as isize;
                let sign = if delta >= 0 { "+" } else { "" };
                format!(
                    "内存回收完成: {} → {} ({sign}{}) / 峰值 {}",
                    fmt_bytes(before),
                    fmt_bytes(after),
                    fmt_bytes(delta.unsigned_abs()),
                    fmt_bytes(peak),
                )
            }
            _ => "内存回收完成（RSS 统计不可用）".to_string(),
        };

        app.active_mut().messages.pending_messages.push(msg);
    }
}

fn fmt_bytes(bytes: usize) -> String {
    const KB: usize = 1024;
    const MB: usize = 1024 * KB;
    if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.0} KB", bytes as f64 / KB as f64)
    } else {
        format!("{bytes} B")
    }
}
