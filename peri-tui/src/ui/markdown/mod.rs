mod table_holdback;

use ratatui::text::Text;

use peri_widgets::markdown::{LinkHit, MarkdownDoc};
use peri_widgets::DefaultMarkdownTheme;

use super::message_view::ContentBlockView;
pub use table_holdback::{HoldbackDecision, TableHoldbackScanner};

static THEME: DefaultMarkdownTheme = DefaultMarkdownTheme;

/// 初始 ViewModel 构建时使用的默认 markdown 渲染宽度。
/// 此时还不知道真实终端宽度，渲染线程首次渲染时会通过
/// `rendered_width` 检测宽度不一致并全量重解析。
pub const DEFAULT_MARKDOWN_WIDTH: usize = 80;

/// 解析 markdown 文本为 ratatui Text
pub fn parse_markdown(input: &str, max_width: usize) -> Text<'static> {
    peri_widgets::markdown::parse_markdown(input, &THEME, max_width)
}

/// 解析 markdown 文本（含超链接命中区）
pub fn parse_markdown_rich(input: &str, max_width: usize) -> MarkdownDoc {
    peri_widgets::markdown::parse_markdown_with_links(input, &THEME, max_width)
}

/// 解析 markdown 文本为 ratatui Text（使用默认宽度 80）
pub fn parse_markdown_default(input: &str) -> Text<'static> {
    parse_markdown(input, DEFAULT_MARKDOWN_WIDTH)
}

/// 解析 markdown 文本（默认宽度 80，含超链接命中区）
pub fn parse_markdown_default_rich(input: &str) -> MarkdownDoc {
    parse_markdown_rich(input, DEFAULT_MARKDOWN_WIDTH)
}

/// 从 `text` 的 `[0..prefix_len]` 范围内找到最后一个块级边界。
///
/// 块级边界定义为不在代码围栏内的 `\n\n`（双换行/空行）。
/// 使用前向扫描追踪代码围栏状态，正确处理未闭合围栏。
///
/// 返回值：边界后的字节位置（即新内容起始处）。
/// 如果找不到边界，返回 0（需要全量重解析）。
pub fn find_last_block_boundary(text: &str, prefix_len: usize) -> usize {
    if prefix_len == 0 {
        return 0;
    }
    let scan_end = prefix_len.min(text.len());
    let bytes = text.as_bytes();

    // 前向扫描：追踪围栏状态，记录最后一个不在围栏内的 \n\n 位置
    let mut in_code_fence = false;
    let mut last_boundary = 0;
    let mut pos = 0;

    while pos < scan_end {
        // 检测代码围栏（3+ 个连续反引号）
        if bytes[pos] == b'`'
            && pos + 2 < scan_end
            && bytes[pos + 1] == b'`'
            && bytes[pos + 2] == b'`'
        {
            in_code_fence = !in_code_fence;
            // 跳过所有连续反引号
            while pos < scan_end && bytes[pos] == b'`' {
                pos += 1;
            }
            continue;
        }

        // 检测空行（\n\n），且不在代码围栏内
        if !in_code_fence && bytes[pos] == b'\n' && pos + 1 < scan_end && bytes[pos + 1] == b'\n' {
            last_boundary = pos + 2;
        }

        pos += 1;
    }

    last_boundary
}

/// 增量版本的 ensure_rendered：只解析新增内容，复用已缓存的渲染前缀。
///
/// 支持表格 holdback：流式过程中，不完整的表格行会被暂缓渲染，
/// 直到行完整或流结束后再提交。非流式模式（历史恢复）始终全部渲染。
///
/// 三条路径：
/// 1. 前文稳定（boundary == rendered_prefix_len）→ 只解析新增部分，追加到 rendered
/// 2. 有不稳定块（0 < boundary < rendered_prefix_len）→ 保留稳定前缀，重解析 boundary 之后
/// 3. 无边界（boundary == 0）→ 全量重解析兜底
pub fn ensure_rendered_incremental(block: &mut ContentBlockView, max_width: usize) {
    if let ContentBlockView::Text {
        raw,
        rendered,
        rendered_links,
        dirty,
        rendered_prefix_len,
        rendered_prefix_lines,
        rendered_width,
        holdback_scanner,
    } = block
    {
        // 宽度变化（初始 80 宽 → 实际终端宽，或 resize）→ 强制全量重解析。
        // 必须同时清空 rendered.lines：否则 rendered_prefix_len=0 使
        // find_last_block_boundary 返回 0，与 effective_prefix_len(0) 相等
        // 走 Path 1 增量追加，新内容叠加在旧内容上导致整段重复渲染。
        if *rendered_width != max_width {
            *dirty = true;
            *rendered_prefix_len = 0;
            *rendered_prefix_lines = 0;
            rendered.lines.clear();
            rendered_links.clear();
        }
        if !*dirty || raw.len() == *rendered_prefix_len {
            return;
        }

        // 表格 holdback 检查
        let decision = holdback_scanner.scan(raw);

        // 确定实际可渲染的文本范围
        let effective_end = match &decision {
            HoldbackDecision::Hold { holdback_offset } => {
                // 只渲染到 holdback 位置
                let offset = (*holdback_offset).min(raw.len());
                // 不要回退到已渲染的前面
                offset.max(*rendered_prefix_len)
            }
            HoldbackDecision::Commit | HoldbackDecision::FlushAll => raw.len(),
        };

        if effective_end <= *rendered_prefix_len {
            // 没有新内容可渲染（全部被 holdback）
            *dirty = false;
            return;
        }

        // 根据实际渲染范围决定渲染策略
        let text_to_render = &raw[..effective_end];
        let effective_prefix_len = *rendered_prefix_len;

        let last_stable_boundary =
            find_last_block_boundary(text_to_render, effective_prefix_len).min(effective_end);

        if last_stable_boundary == effective_prefix_len {
            // 路径 1：前文稳定，只解析新增部分
            let new_text = &text_to_render[effective_prefix_len..];
            if !new_text.is_empty() {
                let new_doc = parse_markdown_rich(new_text, max_width);
                // 追加新行到已有渲染结果，链接行号平移到追加起点
                let base_lines = rendered.lines.len();
                for line in new_doc.text.lines {
                    rendered.lines.push(line);
                }
                for hit in new_doc.links {
                    rendered_links.push(LinkHit {
                        line: hit.line + base_lines,
                        ..hit
                    });
                }
            }
        } else if last_stable_boundary > 0 {
            // 路径 2：有不稳定块，保留前缀，重解析 boundary 之后
            let keep_count = *rendered_prefix_lines;
            let reparse_text = &text_to_render[last_stable_boundary..];
            let new_doc = parse_markdown_rich(reparse_text, max_width);
            rendered.lines.truncate(keep_count);
            rendered_links.retain(|hit| hit.line < keep_count);
            if keep_count > 0 && last_stable_boundary < effective_prefix_len {
                // 需要重新计算：从 boundary 开始全量重解析
                let full_doc =
                    parse_markdown_rich(&text_to_render[last_stable_boundary..], max_width);
                rendered.lines.clear();
                rendered_links.clear();
                for line in full_doc.text.lines {
                    rendered.lines.push(line);
                }
                rendered_links.extend(full_doc.links);
            } else {
                for line in new_doc.text.lines {
                    rendered.lines.push(line);
                }
                for hit in new_doc.links {
                    rendered_links.push(LinkHit {
                        line: hit.line + keep_count,
                        ..hit
                    });
                }
            }
        } else {
            // 路径 3：全量重解析
            let doc = parse_markdown_rich(text_to_render, max_width);
            *rendered = doc.text;
            *rendered_links = doc.links;
        }

        *rendered_prefix_len = effective_end;
        *rendered_prefix_lines = rendered.lines.len();
        *rendered_width = max_width;

        // FlushAll 时重置 scanner（流结束）
        if matches!(decision, HoldbackDecision::FlushAll) {
            holdback_scanner.reset();
        }

        *dirty = false;
    }
}

/// 强制提交所有 holdback 内容（流结束时调用）
///
/// 将 scanner 设为非流式模式，然后执行一次完整渲染。
pub fn ensure_rendered_flush(block: &mut ContentBlockView, max_width: usize) {
    if let ContentBlockView::Text {
        holdback_scanner,
        dirty,
        raw,
        ..
    } = block
    {
        // 如果 raw 为空，无需处理
        if raw.is_empty() {
            return;
        }
        // 切换到非流式模式（触发 FlushAll）
        holdback_scanner.set_streaming(false);
        // 标记 dirty 以确保渲染发生
        *dirty = true;
        ensure_rendered_incremental(block, max_width);
    }
}

#[cfg(test)]
#[path = "mod_test.rs"]
mod markdown_tests;
