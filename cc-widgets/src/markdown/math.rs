//! LaTeX 数学公式到 Unicode / 可读文本的提纯转换器
//!
//! 将常见的 LaTeX 数学表达式转换为终端友好的纯文本，将 LaTeX 标记转化为
//! 直观的 Unicode 符号（如 α、β、≤、≥、×、÷、²、₃ 等），去除 `\text{}`、`\mathrm{}`
//! 等排版包裹，使公式在等宽终端中清晰呈现。

/// 将 LaTeX 表达式转写为终端友好的 Unicode 字符串
pub fn render_math(latex: &str) -> String {
    let mut parser = MathParser::new(latex);
    let mut result = parser.parse();

    // 清理多余连续空格，保留适当单词间隔
    clean_spaces(&mut result);
    result
}

struct MathParser<'a> {
    chars: Vec<char>,
    pos: usize,
    _marker: std::marker::PhantomData<&'a ()>,
}

impl<'a> MathParser<'a> {
    fn new(input: &'a str) -> Self {
        Self {
            chars: input.chars().collect(),
            pos: 0,
            _marker: std::marker::PhantomData,
        }
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn next_char(&mut self) -> Option<char> {
        let c = self.chars.get(self.pos).copied()?;
        self.pos += 1;
        Some(c)
    }

    fn skip_whitespace(&mut self) {
        while let Some(c) = self.peek() {
            if c.is_whitespace() {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    /// 提取花括号 `{...}` 内的内容，支持嵌套
    fn extract_braced(&mut self) -> Option<String> {
        self.skip_whitespace();
        if self.peek() != Some('{') {
            return None;
        }
        self.pos += 1; // 消费 '{'
        let mut depth = 1;
        let mut content = String::new();

        while let Some(c) = self.next_char() {
            if c == '{' {
                depth += 1;
                content.push(c);
            } else if c == '}' {
                depth -= 1;
                if depth == 0 {
                    return Some(content);
                }
                content.push(c);
            } else if c == '\\' {
                content.push(c);
                if let Some(next) = self.next_char() {
                    content.push(next);
                }
            } else {
                content.push(c);
            }
        }
        // 如果未正常闭合，返回已收集的内容
        Some(content)
    }

    /// 提取参数：如果紧跟 `{` 则提取花括号内容，否则取下一个非空白字符
    fn extract_arg(&mut self) -> String {
        self.skip_whitespace();
        if self.peek() == Some('{') {
            self.extract_braced().unwrap_or_default()
        } else if let Some(c) = self.next_char() {
            c.to_string()
        } else {
            String::new()
        }
    }

    fn parse(&mut self) -> String {
        let mut out = String::new();

        while let Some(c) = self.next_char() {
            match c {
                '\\' => {
                    self.parse_command(&mut out);
                }
                '^' => {
                    let arg = self.extract_arg();
                    let rendered_arg = render_math(&arg);
                    if let Some(sup) = to_superscript_string(&rendered_arg) {
                        out.push_str(&sup);
                    } else if rendered_arg.chars().count() == 1 {
                        out.push('^');
                        out.push_str(&rendered_arg);
                    } else {
                        out.push_str(&format!("^({})", rendered_arg));
                    }
                }
                '_' => {
                    let arg = self.extract_arg();
                    let rendered_arg = render_math(&arg);
                    if let Some(sub) = to_subscript_string(&rendered_arg) {
                        out.push_str(&sub);
                    } else if rendered_arg.chars().count() == 1 {
                        out.push('_');
                        out.push_str(&rendered_arg);
                    } else {
                        out.push_str(&format!("_({})", rendered_arg));
                    }
                }
                '{' => {
                    // 孤立的花括号：通常用作分组，内部递归解析
                    self.pos -= 1;
                    if let Some(braced) = self.extract_braced() {
                        out.push_str(&render_math(&braced));
                    }
                }
                '}' => {
                    // 多余的闭括号忽略
                }
                _ => {
                    out.push(c);
                }
            }
        }

        out
    }

    fn parse_command(&mut self, out: &mut String) {
        let Some(first) = self.peek() else {
            return;
        };

        // 处理非字母命令，例如 `\{`, `\}`, `\_`, `\,`, `\\` 等
        if !first.is_alphabetic() {
            self.pos += 1;
            match first {
                '{' | '}' | '_' | '%' | '$' | '&' | '#' => {
                    out.push(first);
                }
                ',' | ';' | ':' => {
                    out.push(' ');
                }
                '!' => {
                    // 负空格，忽略
                }
                '\\' => {
                    // 换行
                    out.push('\n');
                }
                _ => {
                    out.push(first);
                }
            }
            return;
        }

        // 读取字母命令名
        let mut cmd = String::new();
        while let Some(c) = self.peek() {
            if c.is_alphabetic() {
                cmd.push(c);
                self.pos += 1;
            } else {
                break;
            }
        }

        match cmd.as_str() {
            // 文本和格式包装：直接提取内部原样文本
            "text" | "mathrm" | "mathbf" | "mathit" | "mathsf" | "mathtt" | "operatorname" => {
                let inner = self.extract_arg();
                out.push_str(&inner);
            }
            // 分数
            "frac" | "dfrac" | "tfrac" => {
                let num = self.extract_arg();
                let den = self.extract_arg();
                let num_rendered = render_math(&num).trim().to_string();
                let den_rendered = render_math(&den).trim().to_string();

                let num_simple = is_simple_expr(&num_rendered);
                let den_simple = is_simple_expr(&den_rendered);

                let formatted_num = if num_simple {
                    num_rendered
                } else {
                    format!("({})", num_rendered)
                };
                let formatted_den = if den_simple {
                    den_rendered
                } else {
                    format!("({})", den_rendered)
                };

                out.push_str(&format!("{} / {}", formatted_num, formatted_den));
            }
            // 根号
            "sqrt" => {
                self.skip_whitespace();
                let root = if self.peek() == Some('[') {
                    // \sqrt[n]{x}
                    self.pos += 1;
                    let mut r = String::new();
                    while let Some(c) = self.next_char() {
                        if c == ']' {
                            break;
                        }
                        r.push(c);
                    }
                    Some(render_math(&r))
                } else {
                    None
                };

                let body = self.extract_arg();
                let body_rendered = render_math(&body).trim().to_string();

                if let Some(r) = root {
                    let sup = to_superscript_string(&r).unwrap_or_else(|| format!("({})", r));
                    out.push_str(&format!("{}√({})", sup, body_rendered));
                } else if is_simple_expr(&body_rendered) {
                    out.push_str(&format!("√{}", body_rendered));
                } else {
                    out.push_str(&format!("√({})", body_rendered));
                }
            }
            // 括号修饰符 \left, \right
            "left" | "right" => {
                self.skip_whitespace();
                if let Some(c) = self.next_char() {
                    match c {
                        '.' => {} // \left. 或 \right. 表示隐藏边界
                        '\\' => {
                            if let Some(next) = self.next_char() {
                                if next == '{' || next == '}' || next == '|' {
                                    out.push(next);
                                }
                            }
                        }
                        _ => out.push(c),
                    }
                }
            }
            // 空格
            "quad" => out.push_str("  "),
            "qquad" => out.push_str("    "),
            // 常用符号转 Unicode
            _ => {
                if let Some(sym) = lookup_symbol(&cmd) {
                    // 如果符号前后需要与普通字母保持间隙（如 \le \ge + *）
                    let need_space_before = out.ends_with(|c: char| c.is_alphanumeric());
                    if need_space_before && is_binary_operator(sym) {
                        out.push(' ');
                    }
                    out.push_str(sym);
                    if is_binary_operator(sym) {
                        out.push(' ');
                    }
                } else {
                    // 未识别的命令：如果有参数，提取参数内容；否则输出命令本身
                    self.skip_whitespace();
                    if self.peek() == Some('{') {
                        let inner = self.extract_arg();
                        out.push_str(&render_math(&inner));
                    } else {
                        // 保留命令名
                        out.push_str(&cmd);
                    }
                }
            }
        }
    }
}

/// 判断是否为二元运算符/关系符，适合左右加空格
fn is_binary_operator(sym: &str) -> bool {
    matches!(
        sym,
        "×" | "÷"
            | "±"
            | "∓"
            | "·"
            | "≤"
            | "≥"
            | "≠"
            | "≈"
            | "≡"
            | "∼"
            | "∈"
            | "∉"
            | "⊂"
            | "⊆"
            | "⊃"
            | "⊇"
            | "∪"
            | "∩"
            | "←"
            | "→"
            | "↔"
            | "⇐"
            | "⇒"
            | "⇔"
    )
}

/// 判断表达式是否为无需外层括号包裹的简单原子（单变量、单个数字）
fn is_simple_expr(s: &str) -> bool {
    let s = s.trim();
    if s.is_empty() {
        return true;
    }
    // 包含二元操作符或多段空格时需要加括号
    if s.contains(' ')
        || s.contains('+')
        || s.contains('-')
        || s.contains('*')
        || s.contains('/')
        || s.contains('=')
    {
        return false;
    }
    true
}

/// 将字符串尝试转换为全部上标 Unicode，若有无法转换的字符则返回 None
fn to_superscript_string(s: &str) -> Option<String> {
    if s.is_empty() {
        return None;
    }
    let mut out = String::new();
    for c in s.chars() {
        let sup = to_superscript_char(c)?;
        out.push(sup);
    }
    Some(out)
}

/// 将字符串尝试转换为全部下标 Unicode，若有无法转换的字符则返回 None
fn to_subscript_string(s: &str) -> Option<String> {
    if s.is_empty() {
        return None;
    }
    let mut out = String::new();
    for c in s.chars() {
        let sub = to_subscript_char(c)?;
        out.push(sub);
    }
    Some(out)
}

fn to_superscript_char(c: char) -> Option<char> {
    match c {
        '0' => Some('⁰'),
        '1' => Some('¹'),
        '2' => Some('²'),
        '3' => Some('³'),
        '4' => Some('⁴'),
        '5' => Some('⁵'),
        '6' => Some('⁶'),
        '7' => Some('⁷'),
        '8' => Some('⁸'),
        '9' => Some('⁹'),
        '+' => Some('⁺'),
        '-' => Some('⁻'),
        '=' => Some('⁼'),
        '(' => Some('⁽'),
        ')' => Some('⁾'),
        'n' => Some('ⁿ'),
        'i' => Some('ⁱ'),
        'a' => Some('ᵃ'),
        'b' => Some('ᵇ'),
        'c' => Some('ᶜ'),
        'd' => Some('ᵈ'),
        'e' => Some('ᵉ'),
        'f' => Some('ᶠ'),
        'g' => Some('ᵍ'),
        'h' => Some('ʰ'),
        'j' => Some('ʲ'),
        'k' => Some('ᵏ'),
        'l' => Some('ˡ'),
        'm' => Some('ᵐ'),
        'o' => Some('ᵒ'),
        'p' => Some('ᵖ'),
        'r' => Some('ʳ'),
        's' => Some('ˢ'),
        't' => Some('ᵗ'),
        'u' => Some('ᵘ'),
        'v' => Some('ᵛ'),
        'w' => Some('ʷ'),
        'x' => Some('ˣ'),
        'y' => Some('ʸ'),
        'z' => Some('ᶻ'),
        _ => None,
    }
}

fn to_subscript_char(c: char) -> Option<char> {
    match c {
        '0' => Some('₀'),
        '1' => Some('₁'),
        '2' => Some('₂'),
        '3' => Some('₃'),
        '4' => Some('₄'),
        '5' => Some('₅'),
        '6' => Some('₆'),
        '7' => Some('₇'),
        '8' => Some('₈'),
        '9' => Some('₉'),
        '+' => Some('₊'),
        '-' => Some('₋'),
        '=' => Some('₌'),
        '(' => Some('₍'),
        ')' => Some('₎'),
        'a' => Some('ₐ'),
        'e' => Some('ₑ'),
        'h' => Some('ₕ'),
        'i' => Some('ᵢ'),
        'j' => Some('ⱼ'),
        'k' => Some('ₖ'),
        'l' => Some('ₗ'),
        'm' => Some('ₘ'),
        'n' => Some('ₙ'),
        'o' => Some('ₒ'),
        'p' => Some('ₚ'),
        'r' => Some('ᵣ'),
        's' => Some('ₛ'),
        't' => Some('ₜ'),
        'u' => Some('ᵤ'),
        'v' => Some('ᵥ'),
        'x' => Some('ₓ'),
        _ => None,
    }
}

fn lookup_symbol(cmd: &str) -> Option<&'static str> {
    match cmd {
        // 希腊字母（小写）
        "alpha" => Some("α"),
        "beta" => Some("β"),
        "gamma" => Some("γ"),
        "delta" => Some("δ"),
        "epsilon" | "varepsilon" => Some("ε"),
        "zeta" => Some("ζ"),
        "eta" => Some("η"),
        "theta" | "vartheta" => Some("θ"),
        "iota" => Some("ι"),
        "kappa" => Some("κ"),
        "lambda" => Some("λ"),
        "mu" => Some("μ"),
        "nu" => Some("ν"),
        "xi" => Some("ξ"),
        "pi" | "varpi" => Some("π"),
        "rho" | "varrho" => Some("ρ"),
        "sigma" | "varsigma" => Some("σ"),
        "tau" => Some("τ"),
        "upsilon" => Some("υ"),
        "phi" | "varphi" => Some("φ"),
        "chi" => Some("χ"),
        "psi" => Some("ψ"),
        "omega" => Some("ω"),

        // 希腊字母（大写）
        "Gamma" => Some("Γ"),
        "Delta" => Some("Δ"),
        "Theta" => Some("Θ"),
        "Lambda" => Some("Λ"),
        "Xi" => Some("Ξ"),
        "Pi" => Some("Π"),
        "Sigma" => Some("Σ"),
        "Upsilon" => Some("Υ"),
        "Phi" => Some("Φ"),
        "Psi" => Some("Ψ"),
        "Omega" => Some("Ω"),

        // 运算符与关系符
        "times" => Some("×"),
        "div" => Some("÷"),
        "pm" => Some("±"),
        "mp" => Some("∓"),
        "cdot" => Some("·"),
        "cdots" | "dots" | "ldots" => Some("…"),
        "le" | "leq" => Some("≤"),
        "ge" | "geq" => Some("≥"),
        "ne" | "neq" => Some("≠"),
        "approx" => Some("≈"),
        "equiv" => Some("≡"),
        "sim" => Some("∼"),
        "simeq" => Some("≃"),
        "propto" => Some("∝"),
        "in" => Some("∈"),
        "notin" => Some("∉"),
        "ni" | "owns" => Some("∋"),
        "subset" => Some("⊂"),
        "subseteq" => Some("⊆"),
        "supset" => Some("⊃"),
        "supseteq" => Some("⊇"),
        "cup" => Some("∪"),
        "cap" => Some("∩"),
        "forall" => Some("∀"),
        "exists" => Some("∃"),
        "nexists" => Some("∄"),
        "infty" => Some("∞"),
        "partial" => Some("∂"),
        "nabla" => Some("∇"),
        "sum" => Some("∑"),
        "prod" => Some("∏"),
        "int" => Some("∫"),
        "iint" => Some("∬"),
        "iiint" => Some("∭"),
        "oint" => Some("∮"),
        "leftarrow" | "gets" => Some("←"),
        "rightarrow" | "to" => Some("→"),
        "leftrightarrow" => Some("↔"),
        "Leftarrow" => Some("⇐"),
        "Rightarrow" => Some("⇒"),
        "Leftrightarrow" | "iff" => Some("⇔"),
        "degree" => Some("°"),
        "circ" => Some("∘"),
        "prime" => Some("′"),
        "angle" => Some("∠"),
        "perp" => Some("⊥"),
        "vert" => Some("|"),
        "Vert" => Some("‖"),
        _ => None,
    }
}

/// 清理文本中的连续空格，去除括号内侧无意义空格，保留适当的单空格
fn clean_spaces(s: &mut String) {
    let mut cleaned = String::with_capacity(s.len());
    let mut last_was_space = false;
    for c in s.chars() {
        if c == ' ' {
            if !last_was_space {
                cleaned.push(' ');
                last_was_space = true;
            }
        } else {
            cleaned.push(c);
            last_was_space = false;
        }
    }
    let mut res = cleaned.trim().to_string();
    while res.contains("( ") {
        res = res.replace("( ", "(");
    }
    while res.contains(" )") {
        res = res.replace(" )", ")");
    }
    while res.contains("[ ") {
        res = res.replace("[ ", "[");
    }
    while res.contains(" ]") {
        res = res.replace(" ]", "]");
    }
    *s = res;
}
