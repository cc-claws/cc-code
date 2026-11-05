use super::math::render_math;

#[test]
fn test_render_math_text_wrapper() {
    let input = r#"\text{Total Amount} = (\text{Base Price} + \text{Extra Fee}).\text{round}(2)"#;
    let result = render_math(input);
    assert_eq!(
        result, "Total Amount = (Base Price + Extra Fee).round(2)",
        "应去除 text 包装并保留原样文本"
    );
}

#[test]
fn test_render_math_superscript_and_subscript() {
    let input = r#"E = mc^2 + a_1 + x_{12} + y^{10}"#;
    let result = render_math(input);
    assert_eq!(
        result, "E = mc² + a₁ + x₁₂ + y¹⁰",
        "应将数字上下标转换为 Unicode 上下标"
    );
}

#[test]
fn test_render_math_complex_subscript_fallback() {
    let input = r#"x_{max} + y_{custom_val}"#;
    let result = render_math(input);
    assert!(
        result.contains("x_(max)") || result.contains("x_max") || result.contains("xₘₐₓ"),
        "无法完全转为 Unicode 下标的复杂标识符应优雅保留"
    );
}

#[test]
fn test_render_math_greek_symbols() {
    let input = r#"\alpha + \beta = \gamma \times \Delta"#;
    let result = render_math(input);
    assert_eq!(result, "α + β = γ × Δ", "应将常见希腊字母转换为 Unicode");
}

#[test]
fn test_render_math_relations_and_operators() {
    let input = r#"a \le b \ge c \ne d \approx e \pm f"#;
    let result = render_math(input);
    assert_eq!(
        result, "a ≤ b ≥ c ≠ d ≈ e ± f",
        "应将常见关系符与二元运算符转换为 Unicode"
    );
}

#[test]
fn test_render_math_fractions() {
    let input = r#"\frac{a + b}{c + d} + \frac{1}{2}"#;
    let result = render_math(input);
    assert_eq!(
        result, "(a + b) / (c + d) + 1 / 2",
        "分式应转换为可读的除法形式"
    );
}

#[test]
fn test_render_math_square_root() {
    let input = r#"\sqrt{x^2 + y^2} + \sqrt{2}"#;
    let result = render_math(input);
    assert_eq!(result, "√(x² + y²) + √2", "根号应转为 Unicode 根号形式");
}

#[test]
fn test_render_math_brackets_and_escapes() {
    let input = r#"\left( \frac{x}{y} \right) + \{ a, b \} + x\_1"#;
    let result = render_math(input);
    assert_eq!(
        result, "(x / y) + { a, b } + x_1",
        "括号修饰符与转义字符应正确还原"
    );
}
