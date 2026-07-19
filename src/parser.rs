//! CSSスタイルシートのパーサー。`/* コメント */`除去→
//! `セレクタ { 宣言リスト }`ブロックへの分割→宣言(`property: value;`)
//! のパース、という素直な逐次スキャンで実装する(第一段)。
//!
//! 対応: カンマ区切りの複数セレクタ(`h1, h2 { ... }`)、
//! セミコロン区切りの複数宣言、コメント除去、宣言末尾の`!important`
//! (2026-07-19対応、`value!important`/`value !important`の両表記——
//! `! important`のように`!`と`important`の間に空白がある表記は非対応
//! ・素通りしてただの値の一部として扱われる、正直なスコープの限界)、
//! `@media`ブロック(2026-07-19対応、対応する構文サブセットは`media`
//! モジュール冒頭コメント参照)。
//! 未対応(次段階): CSS変数(`--foo`/`var()`)、文字列内のリテラル
//! `{`/`}`/`;`(例: `content: "a;b"`)のエスケープ処理、
//! カスケードレイヤー(`@layer`)。

use crate::media::{parse_media_query, MediaQuery};
use crate::selector::{parse_selector, Selector};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Declaration {
    pub property: String,
    pub value: String,
    /// 宣言末尾に`!important`が付いていたか(スコープ: モジュール
    /// 冒頭コメント参照)。カスケード側(`cascade::compute_style`)で
    /// 「specificityに関わらず常に非`!important`宣言に勝つ」という
    /// CSS本来のセマンティクスに使われる。
    pub important: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule {
    /// カンマ区切りの複数セレクタは同じ宣言を共有する複数の`Rule`では
    /// なく、1つの`Rule`が複数の`selectors`を持つ形で表現する
    /// (CSSの実際のセマンティクスに合わせる: `h1, h2 { color: red }`は
    /// 1つの宣言ブロックが2つのセレクタにマッチする)。
    pub selectors: Vec<Selector>,
    pub declarations: Vec<Declaration>,
    /// このルールを包む`@media`ブロックの条件。トップレベル(`@media`
    /// の外)のルールは`None`(常にマッチする)。
    pub media: Option<MediaQuery>,
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

/// 値の末尾に`!important`(大小文字問わず、値との間の空白は任意)が
/// あれば取り除いて`true`を返す。`! important`(`!`と`important`の
/// 間に空白がある表記)は非対応——素通りしてただの値の一部として扱う
/// (モジュール冒頭コメント参照)。
fn strip_important_suffix(value: &str) -> (String, bool) {
    let lower = value.to_ascii_lowercase();
    if let Some(stripped_len) = lower.strip_suffix("!important").map(str::len) {
        let value = value[..stripped_len].trim_end().to_string();
        (value, true)
    } else {
        (value.to_string(), false)
    }
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
            let (value, important) = strip_important_suffix(value.trim());
            if property.is_empty() || value.is_empty() {
                return None;
            }
            Some(Declaration { property, value, important })
        })
        .collect()
}

/// `s`の先頭に`{`が1つ消費された残り(depth=1からスタート)から、
/// 対応する閉じ`}`の`s`内でのインデックスを探す(ネストした`{`/`}`
/// を数え上げる、`@media`ブロック内のルールブロックに対応するため)。
fn find_matching_close_brace(s: &str) -> Option<usize> {
    let mut depth = 1i32;
    for (i, c) in s.char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

/// `prefix`(トリム済み)が`@media`で始まっていれば、その後ろの
/// メディアクエリ文字列(トリム済み)を返す。
fn strip_at_media_prefix(prefix: &str) -> Option<&str> {
    prefix.strip_prefix("@media").map(str::trim)
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
        let prefix = rest[..open_brace].trim();
        let after_open = &rest[open_brace + 1..];

        if let Some(media_text) = strip_at_media_prefix(prefix) {
            let Some(close_rel) = find_matching_close_brace(after_open) else {
                break; // 閉じ括弧の無い壊れた入力は、そこで打ち切る。
            };
            let inner = &after_open[..close_rel];
            let media_query = parse_media_query(media_text);
            // ネストした`@media`(仕様上は許されるがこの実装のスコープ外)
            // の場合、既に内側で`media`が設定されていればそちらを優先し、
            // 外側の条件では上書きしない(安全側: 内側の条件を尊重)。
            for mut rule in parse_stylesheet(inner) {
                if rule.media.is_none() {
                    rule.media = Some(media_query.clone());
                }
                rules.push(rule);
            }
            rest = &after_open[close_rel + 1..];
            continue;
        }

        let Some(close_brace) = after_open.find('}') else {
            break; // 閉じ括弧の無い壊れた入力は、そこで打ち切る。
        };
        let declarations_text = &after_open[..close_brace];
        let selectors = parse_selector_list(prefix);
        if !selectors.is_empty() {
            rules.push(Rule { selectors, declarations: parse_declarations(declarations_text), media: None });
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

    fn decl(property: &str, value: &str) -> Declaration {
        Declaration { property: property.to_string(), value: value.to_string(), important: false }
    }

    #[test]
    fn parses_a_single_rule_with_multiple_declarations() {
        let rules = parse_stylesheet("p { color: red; font-size: 12px; }");
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].selectors, vec![vec![descendant(vec![SimplePart::Tag("p".to_string())])]]);
        assert_eq!(rules[0].declarations, vec![decl("color", "red"), decl("font-size", "12px")]);
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
        assert_eq!(rules[0].declarations, vec![decl("color", "red")]);
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
