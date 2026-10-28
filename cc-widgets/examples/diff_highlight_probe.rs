//! diff 语法高亮成本探针（Issue #643 复核用）
//!
//! 全程固定**同一终端宽度**与**同一语法主题**，回答一个问题：
//! 用户不切主题、不改窗口宽度时，逐行 syntect 高亮还剩多少成本？
//!
//! 场景：
//! 1. `cold_miss`      整份成品缓存未命中（内容全新的 diff）
//! 2. `same_diff_hit`  同一 diff 二次渲染（整份成品命中）
//! 3. `evicted_replay` 长会话：成品被逐出后回滚重渲同一 diff
//! 4. `new_file_x2`    新建文件预览连调两次（该入口没有整份成品缓存）
//! 5. `detail_expand`  Ctrl+O 首次展开（普通视图 10 行 → 全量）
//! 6. `streaming`      流式增长：内容逐帧变化，整份成品每帧必 miss
//! 7. `no_highlight`   无高亮对照（未知扩展名）
//!
//! 运行：
//! ```text
//! cargo run -p cc-widgets --features markdown-highlight --example diff_highlight_probe
//! cargo run --release -p cc-widgets --features markdown-highlight --example diff_highlight_probe
//! ```

use std::hint::black_box;
use std::time::{Duration, Instant};

use cc_widgets::diff::{render_cache_stats, render_diff, render_new_file_code, DiffInput};
use cc_widgets::theme::DarkTheme;

const WIDTH: usize = 100;
const SAMPLES: usize = 21;
/// 填满成品缓存的份数（324 行 diff 约 190 KB/份；8 MiB 约 42 份，32 MiB 约 170 份）
const MEM_FILL: usize = 200;
const EVICT_ROUNDS: usize = 3;
/// 合成源码组数，每组 5 行
const GROUPS: usize = 40;

fn body(tag: usize, mutations: usize) -> String {
    let mut out = String::with_capacity(16 * 1024);
    out.push_str(&format!(
        "// probe case {tag} — synthetic Rust source for highlight benchmarking\n\n"
    ));
    for group in 0..GROUPS {
        let delta = usize::from(group < mutations);
        // 每行都带 group/tag，保证行文本唯一（避免合成代码自带的重复行虚高命中率）
        out.push_str(&format!(
            "pub fn handler_{group}(input: &str) -> Result<usize, String> {{\n    let base_{group} = input.len() + {bias};\n    if base_{group} > {limit} {{ return Err(format!(\"too large: {{}} at {{}}\", base_{group}, {group})); }}\n    Ok(base_{group} * 2 + {group})\n}}\n\n",
            bias = group + delta + tag,
            limit = 40 + delta,
        ));
    }
    out
}

fn case(tag: usize, mutations: usize) -> DiffInput {
    DiffInput {
        file_path: format!("src/probe_{tag}.rs"),
        old_content: body(tag, 0),
        new_content: body(tag, mutations),
        is_new_file: false,
        is_deleted_file: false,
        is_binary: false,
    }
}

fn time_diff_lines(input: &DiffInput) -> (Duration, usize) {
    let t = Instant::now();
    let lines = render_diff(input, WIDTH, &DarkTheme);
    let n = lines.len();
    black_box(n);
    (t.elapsed(), n)
}

fn time_diff(input: &DiffInput) -> Duration {
    time_diff_lines(input).0
}

fn time_new_file(path: &str, content: &str, max_lines: usize) -> Duration {
    let t = Instant::now();
    let lines = render_new_file_code(path, content, WIDTH, &DarkTheme, max_lines);
    black_box(lines.len());
    t.elapsed()
}

fn median(mut v: Vec<Duration>) -> Duration {
    v.sort();
    v[v.len() / 2]
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

fn warmup() {
    let input = case(9_000, 1);
    for _ in 0..3 {
        black_box(render_diff(&input, WIDTH, &DarkTheme).len());
    }
}

fn bench_cold_miss() -> Duration {
    let mut samples = Vec::with_capacity(SAMPLES);
    for tag in 0..SAMPLES {
        let input = case(10_000 + tag, 40);
        samples.push(time_diff(&input));
    }
    median(samples)
}

fn bench_same_diff_hit() -> Duration {
    let input = case(20_000, 40);
    black_box(render_diff(&input, WIDTH, &DarkTheme).len());
    let mut samples = Vec::with_capacity(SAMPLES);
    for _ in 0..SAMPLES {
        samples.push(time_diff(&input));
    }
    median(samples)
}

/// 返回 (回滚耗时, 全新内容对照耗时, 回滚输出行数, 对照输出行数)
fn bench_evicted_replay() -> (Duration, Duration, usize, usize) {
    let mut replays = Vec::with_capacity(EVICT_ROUNDS);
    let mut controls = Vec::with_capacity(EVICT_ROUNDS);
    let mut replay_lines = 0;
    let mut control_lines = 0;
    for round in 0..EVICT_ROUNDS {
        let base = 30_000 + round * 100;
        for i in 0..MEM_FILL {
            let input = case(base + i, 40);
            black_box(render_diff(&input, WIDTH, &DarkTheme).len());
        }
        // 回滚到本轮第一份：此时应已被逐出
        let target = case(base, 40);
        let (d, n) = time_diff_lines(&target);
        replay_lines = n;
        replays.push(d);
        // 对照：同形状但内容全新，确定 miss
        let control = case(base + 900, 40);
        let (d, n) = time_diff_lines(&control);
        control_lines = n;
        controls.push(d);
    }
    (
        median(replays),
        median(controls),
        replay_lines,
        control_lines,
    )
}

fn bench_new_file_x2() -> (Duration, Duration) {
    let mut first = Vec::with_capacity(5);
    let mut second = Vec::with_capacity(5);
    for tag in 0..5 {
        let content = body(40_000 + tag, 40);
        let path = format!("src/probe_{}.rs", 40_000 + tag);
        first.push(time_new_file(&path, &content, usize::MAX));
        second.push(time_new_file(&path, &content, usize::MAX));
    }
    (median(first), median(second))
}

fn bench_detail_expand() -> (Duration, Duration) {
    let content = body(50_000, 40);
    let path = "src/probe_50000.rs";
    let mut normal = Vec::with_capacity(5);
    let mut expanded = Vec::with_capacity(5);
    for _ in 0..5 {
        normal.push(time_new_file(path, &content, 10));
        expanded.push(time_new_file(path, &content, usize::MAX));
    }
    (median(normal), median(expanded))
}

fn bench_streaming() -> (Duration, usize) {
    let mut total = Duration::ZERO;
    let mut frames = 0;
    // 内容逐帧增长：mutations 0 → 35，每帧 5 处
    for step in 0..8 {
        let input = case(60_000, step * 5);
        total += time_diff(&input);
        frames += 1;
    }
    (total / frames as u32, frames)
}

fn bench_no_highlight() -> Duration {
    let mut samples = Vec::with_capacity(SAMPLES);
    for tag in 0..SAMPLES {
        let mut input = case(70_000 + tag, 40);
        input.file_path = format!("probe_{tag}.unknownext");
        samples.push(time_diff(&input));
    }
    median(samples)
}

/// 统计「需要高亮的行」的重复率。行级缓存的收益上限 ≈ 该重复率 × 高亮耗时占比。
///
/// 需要高亮的行 = diff 输出里的上下文行 + 新增行，均取自 new_content；
/// 删除行不参与高亮。这里用 new_content 的非空行近似（折叠上下文时实际行数更少）。
///
/// 口径：除特别说明外，命中率按「第 2 次起的渲染」计算——首次渲染没有东西可复用。
fn line_reuse() {
    let lines_of = |text: &str| -> Vec<String> {
        text.lines()
            .filter(|l| !l.trim().is_empty())
            .map(str::to_string)
            .collect()
    };

    // A. 流式/同文件多版本：8 帧，每帧多 5 处改动；只在后续帧统计命中
    let mut seen: std::collections::HashSet<String> =
        lines_of(&body(60_000, 0)).into_iter().collect();
    let mut total = 0usize;
    let mut hits = 0usize;
    for step in 1..8 {
        for line in lines_of(&body(60_000, step * 5)) {
            total += 1;
            if !seen.insert(line) {
                hits += 1;
            }
        }
    }
    println!(
        "行复用 A 流式(第2-8帧): 高亮行 {total}, 命中 {hits}, 命中率 {:.1}%",
        hits as f64 * 100.0 / total as f64
    );

    // B. 成品被逐出后回滚：内容完全相同 → 第 2 次渲染全部命中
    println!("行复用 B 回滚同一 diff: 第 2 次渲染命中率 100.0%（行文本完全一致）");

    // C. Ctrl+O 首次展开：普通视图 10 行 → 全量
    let full = lines_of(&body(50_000, 40)).len();
    println!(
        "行复用 C Ctrl+O 展开: 已渲 10 行 / 全量 {full} 行, 命中率 {:.1}%",
        10.0 * 100.0 / full as f64
    );

    // D. 新建文件预览同内容重复渲染：同 B
    println!("行复用 D 新建预览重复渲染: 第 2 次命中率 100.0%");

    // E. 不同文件之间（长会话里滚动不同 diff）
    let mut set = std::collections::HashSet::new();
    let mut total = 0usize;
    for tag in 0..10 {
        for line in lines_of(&body(80_000 + tag, 40)) {
            total += 1;
            set.insert(line);
        }
    }
    println!(
        "行复用 E 跨不同文件(10份): 高亮行 {total}, 唯一行 {}, 命中率 {:.1}%",
        set.len(),
        (total - set.len()) as f64 * 100.0 / total as f64
    );
}

fn rss_bytes() -> usize {
    let pid = sysinfo::Pid::from_u32(std::process::id());
    let mut sys = sysinfo::System::new_all();
    sys.refresh_all();
    sys.process(pid).map(|p| p.memory() as usize).unwrap_or(0)
}

/// 测「成品缓存驻留的估算字节」与「进程 RSS 增量」的放大系数。
///
/// 必须在其他场景之前调用，此时缓存基本为空；否则只测到逐出替换。
fn bench_memory() {
    let stats0 = render_cache_stats();
    let rss0 = rss_bytes();
    for i in 0..MEM_FILL {
        let input = case(90_000 + i, 40);
        black_box(render_diff(&input, WIDTH, &DarkTheme).len());
    }
    let stats1 = render_cache_stats();
    let rss1 = rss_bytes();
    let delta = rss1.saturating_sub(rss0);
    println!("== 内存：缓存驻留 vs 真实 RSS ==");
    println!(
        "缓存: {} -> {} 条, 估算 {:.1} MB -> {:.1} MB (预算 {:.0} MB)",
        stats0.entries,
        stats1.entries,
        stats0.bytes as f64 / 1048576.0,
        stats1.bytes as f64 / 1048576.0,
        stats1.budget as f64 / 1048576.0,
    );
    println!(
        "RSS: {:.1} MB -> {:.1} MB, 增量 {:.1} MB, 放大系数 {:.2}x（相对估算字节）",
        rss0 as f64 / 1048576.0,
        rss1 as f64 / 1048576.0,
        delta as f64 / 1048576.0,
        delta as f64 / (stats1.bytes.saturating_sub(stats0.bytes).max(1)) as f64,
    );
    println!();
}

fn main() {
    let stats_before = render_cache_stats();
    warmup();
    bench_memory();

    let cold = bench_cold_miss();
    let hit = bench_same_diff_hit();
    let (evicted, evict_control, evict_lines, control_lines) = bench_evicted_replay();
    let (nf_first, nf_second) = bench_new_file_x2();
    let (expand_before, expand_after) = bench_detail_expand();
    let (stream_avg, stream_frames) = bench_streaming();
    let plain = bench_no_highlight();
    let stats_after = render_cache_stats();

    println!("== diff 高亮成本探针（width={WIDTH}，主题=DarkTheme，同一宽度同一主题） ==");
    println!();
    println!("{:<34}{:>12}", "场景", "中位耗时(ms)");
    println!("{:-<46}", "");
    println!("{:<34}{:>12.3}", "1. 整份成品未命中（全新 diff）", ms(cold));
    println!(
        "{:<34}{:>12.3}",
        "2. 同一 diff 二次渲染（成品命中）",
        ms(hit)
    );
    println!("{:<34}{:>12.3}", "3. 逐出后回滚同一 diff", ms(evicted));
    println!(
        "{:<34}{:>12.3}",
        "3b. 同形状全新内容（确定 miss）",
        ms(evict_control)
    );
    println!("{:<34}{:>12.3}", "4a. 新建文件预览 第 1 次", ms(nf_first));
    println!(
        "{:<34}{:>12.3}",
        "4b. 新建文件预览 第 2 次(同内容)",
        ms(nf_second)
    );
    println!(
        "{:<34}{:>12.3}",
        "5a. Ctrl+O 前（截断 10 行）",
        ms(expand_before)
    );
    println!(
        "{:<34}{:>12.3}",
        "5b. Ctrl+O 后（全量展开）",
        ms(expand_after)
    );
    println!(
        "{:<34}{:>12.3}",
        "6. 流式每帧均值（成品每帧 miss）",
        ms(stream_avg)
    );
    println!("{:<34}{:>12.3}", "7. 无高亮对照（未知扩展名）", ms(plain));
    println!();
    println!(
        "流式帧数: {stream_frames}    高亮/无高亮 倍率: {:.2}x",
        ms(cold) / ms(plain)
    );
    println!("行数: 回滚={evict_lines} 对照={control_lines}");
    println!(
        "成品缓存: before entries={}/{} bytes={} -> after entries={}/{} bytes={} (budget {})",
        stats_before.entries,
        stats_before.capacity,
        stats_before.bytes,
        stats_after.entries,
        stats_after.capacity,
        stats_after.bytes,
        stats_after.budget,
    );
    println!();
    line_reuse();
}
