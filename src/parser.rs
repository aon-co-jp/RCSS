//! CSSスタイルシートのパーサー。`/* コメント */`除去→
//! `セレクタ { 宣言リスト }`ブロックへの分割→宣言(`property: value;`)
//! のパース、という素直な逐次スキャンで実装する(第一段)。
//!
//! 対応: カンマ区切りの複数セレクタ(`h1, h2 { ... }`)、
//! セミコロン区切りの複数宣言、コメント除去。
//! 未対応(次段階): `@media`等のat-rule、`!important`、CSS変数
//! (`--foo`/`var()`)、文字列内のリテラル`{`/`}`/`;`(例:
//! `content: "a;b"`)のエスケープ処理。

use crate::selector::{parse_selector, Selector};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Declaration {
    pub property: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule {
    /// カンマ区切りの複数セレクタは同じ宣言を共有する複数の`Rule`では
    /// なく、1つの`Rule`が複数の`selectors`を持つ形で表現する
    /// (CSSの実際のセマンティクスに合わせる: `h1, h2 { color: red }`は
    /// 1つの宣言ブロックが2つのセレクタにマッチする)。
    pub selectors: Vec<Selector>,
    pub declarations: Vec<Declaration>,
}

fn strip_comments(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '/' && chars.peek() == Some(&'*') {
            chars.next(); // consume '*'
            while let Some(c) = chars.next() {
                if c == '*' && chars.peek() == Some(&'/') {
                    chars.next();
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn parse_declarations(block: &str) -> Vec<Declaration> {
    block
        .split(';')
        .filter_map(|decl| {
            let decl = decl.trim();
            if decl.is_empty() {
                return None;
            }
            let (property, value) = decl.split_once(':')?;
            let property = property.trim().to_ascii_lowercase();
            let value = value.trim().to_string();
            if property.is_empty() || value.is_empty() {
                return None;
            }
            Some(Declaration { property, value })
        })
        .collect()
}

fn parse_selector_list(selector_text: &str) -> Vec<Selector> {
    selector_text.split(',').filter_map(|s| parse_selector(s.trim())).collect()
}

/// スタイルシート文字列全体をパースし、出現順を保った`Rule`列を返す
/// (カスケードでの「後勝ち」判定に出現順が必要なため、順序は
/// 呼び出し側にとって意味を持つ)。
pub fn parse_stylesheet(css: &str) -> Vec<Rule> {
    let css = strip_comments(css);
    let mut rules = Vec::new();
    let mut rest = css.as_str();

    while let Some(open_brace) = rest.find('{') {
        let selector_text = &rest[..open_brace];
        let after_open = &rest[open_brace + 1..];
        let Some(close_brace) = after_open.find('}') else {
            break; // 閉じ括弧の無い壊れた入力は、そこで打ち切る。
        };
        let declarations_text = &after_open[..close_brace];
        let selectors = parse_selector_list(selector_text);
        if !selectors.is_empty() {
            rules.push(Rule { selectors, declarations: parse_declarations(declarations_text) });
        }
        rest = &after_open[close_brace + 1..];
    }

    rules
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::selector::{Combinator, SelectorSegment, SimplePart};

    fn descendant(parts: Vec<SimplePart>) -> SelectorSegment {
        SelectorSegment { combinator: Combinator::Descendant, compound: parts }
    }

    #[test]
    fn parses_a_single_rule_with_multiple_declarations() {
        let rules = parse_stylesheet("p { color: red; font-size: 12px; }");
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].selectors, vec![vec![descendant(vec![SimplePart::Tag("p".to_string())])]]);
        assert_eq!(
            rules[0].declarations,
            vec![
                Declaration { property: "color".to_string(), value: "red".to_string() },
                Declaration { property: "font-size".to_string(), value: "12px".to_string() },
            ]
        );
    }

    #[test]
    fn comma_separated_selectors_share_one_rule() {
        let rules = parse_stylesheet("h1, h2 { margin: 0; }");
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].selectors.len(), 2);
    }

    #[test]
    fn comments_are_stripped_before_parsing() {
        let rules = parse_stylesheet("/* comment */ p { /* inline */ color: red; }");
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].declarations, vec![Declaration { property: "color".to_string(), value: "red".to_string() }]);
    }

    #[test]
    fn multiple_rules_are_parsed_in_source_order() {
        let rules = parse_stylesheet("p { color: red; } .foo { color: blue; }");
        assert_eq!(rules.len(), 2);
        assert_eq!(rules[0].selectors[0], vec![descendant(vec![SimplePart::Tag("p".to_string())])]);
        assert_eq!(rules[1].selectors[0], vec![descendant(vec![SimplePart::Class("foo".to_string())])]);
    }

    #[test]
    fn parses_descendant_combinator_in_stylesheet() {
        let rules = parse_stylesheet("div p { color: red; }");
        assert_eq!(rules.len(), 1);
        assert_eq!(
            rules[0].selectors[0],
            vec![
                descendant(vec![SimplePart::Tag("div".to_string())]),
                descendant(vec![SimplePart::Tag("p".to_string())]),
            ]
        );
    }

    #[test]
    fn parses_child_and_adjacent_sibling_combinators_in_stylesheet() {
        let rules = parse_stylesheet("div > p { color: red; } li + li { color: blue; }");
        assert_eq!(rules.len(), 2);
        assert_eq!(
            rules[0].selectors[0],
            vec![
                descendant(vec![SimplePart::Tag("div".to_string())]),
                SelectorSegment { combinator: Combinator::Child, compound: vec![SimplePart::Tag("p".to_string())] },
            ]
        );
        assert_eq!(
            rules[1].selectors[0],
            vec![
                descendant(vec![SimplePart::Tag("li".to_string())]),
                SelectorSegment {
                    combinator: Combinator::AdjacentSibling,
                    compound: vec![SimplePart::Tag("li".to_string())],
                },
            ]
        );
    }

    #[test]
    fn parses_general_sibling_combinator_in_stylesheet() {
        let rules = parse_stylesheet(".a ~ .b { color: green; }");
        assert_eq!(rules.len(), 1);
        assert_eq!(
            rules[0].selectors[0],
            vec![
                descendant(vec![SimplePart::Class("a".to_string())]),
                SelectorSegment {
                    combinator: Combinator::GeneralSibling,
                    compound: vec![SimplePart::Class("b".to_string())],
                },
            ]
        );
    }
}
