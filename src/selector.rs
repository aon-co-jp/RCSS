//! セレクタとその優先度(specificity)計算。第一段では**単純セレクタの
//! 組み合わせのみ**を対象とする(`div.foo#bar`のようなタグ名/クラス/ID
//! の組み合わせ)。子孫結合子(スペース)・子結合子(`>`)・隣接兄弟結合子
//! (`+`)等のコンビネータは次段階の課題として明記(`cssparser`が
//! `Parser<'i,'t>`でライフタイム分離するのと同様、複雑な結合子は
//! 一からスコープを広げていく設計)。

/// コンパウンドセレクタを構成する単純セレクタの一部品。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SimplePart {
    Tag(String),
    Class(String),
    Id(String),
    /// `*`(universal selector)。何にでもマッチするがspecificityには
    /// 寄与しない。
    Universal,
}

/// `div.foo#bar`のような、結合子を含まない1つのコンパウンドセレクタ。
pub type CompoundSelector = Vec<SimplePart>;

/// 要素側が満たすべき最小限のインターフェース。特定のDOM実装
/// (`rhtml5::Element`等)に直接依存しないよう、トレイトとして
/// 切り出す(cssparserがDOM非依存であるのと同じ設計判断)。
pub trait ElementLike {
    fn tag_name(&self) -> &str;
    fn classes(&self) -> Vec<&str>;
    fn id(&self) -> Option<&str>;
}

pub fn matches<E: ElementLike + ?Sized>(selector: &CompoundSelector, el: &E) -> bool {
    selector.iter().all(|part| match part {
        SimplePart::Tag(name) => el.tag_name() == name,
        SimplePart::Class(name) => el.classes().contains(&name.as_str()),
        SimplePart::Id(name) => el.id() == Some(name.as_str()),
        SimplePart::Universal => true,
    })
}

/// CSS specificityの標準的な(id数, class数, tag数)モデル。
/// タプルの辞書式順序比較がそのまま優先順位比較になる
/// (id > class > tag)。universal selectorはどれにも寄与しない。
pub fn specificity(selector: &CompoundSelector) -> (u32, u32, u32) {
    let mut ids = 0;
    let mut classes = 0;
    let mut tags = 0;
    for part in selector {
        match part {
            SimplePart::Id(_) => ids += 1,
            SimplePart::Class(_) => classes += 1,
            SimplePart::Tag(_) => tags += 1,
            SimplePart::Universal => {}
        }
    }
    (ids, classes, tags)
}

/// `div.foo#bar` / `*` / `.foo` / `#bar` のような単一コンパウンド
/// セレクタ文字列をパースする。空白を含む複合セレクタ(結合子)は
/// この関数のスコープ外(次段階の課題)。
pub fn parse_compound_selector(input: &str) -> Option<CompoundSelector> {
    let input = input.trim();
    if input.is_empty() {
        return None;
    }
    if input == "*" {
        return Some(vec![SimplePart::Universal]);
    }

    let mut parts = Vec::new();
    let mut chars = input.chars().peekable();
    let mut current_kind: Option<char> = None; // '.', '#', またはNone(タグ名の先頭)
    let mut buf = String::new();

    macro_rules! flush {
        () => {
            if !buf.is_empty() {
                let part = match current_kind {
                    Some('.') => SimplePart::Class(std::mem::take(&mut buf)),
                    Some('#') => SimplePart::Id(std::mem::take(&mut buf)),
                    _ => SimplePart::Tag(std::mem::take(&mut buf)),
                };
                parts.push(part);
            }
        };
    }

    while let Some(&c) = chars.peek() {
        if c == '.' || c == '#' {
            flush!();
            current_kind = Some(c);
            chars.next();
        } else if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
            buf.push(c);
            chars.next();
        } else {
            // 未対応の文字(結合子等)に達したら、そこで打ち切る
            // (第一段のスコープ外の入力を静かに無視する簡略化)。
            break;
        }
    }
    flush!();

    if parts.is_empty() {
        None
    } else {
        Some(parts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FakeElement {
        tag: &'static str,
        classes: Vec<&'static str>,
        id: Option<&'static str>,
    }

    impl ElementLike for FakeElement {
        fn tag_name(&self) -> &str {
            self.tag
        }
        fn classes(&self) -> Vec<&str> {
            self.classes.clone()
        }
        fn id(&self) -> Option<&str> {
            self.id
        }
    }

    #[test]
    fn parses_tag_class_and_id_selectors() {
        assert_eq!(parse_compound_selector("div"), Some(vec![SimplePart::Tag("div".to_string())]));
        assert_eq!(parse_compound_selector(".foo"), Some(vec![SimplePart::Class("foo".to_string())]));
        assert_eq!(parse_compound_selector("#bar"), Some(vec![SimplePart::Id("bar".to_string())]));
        assert_eq!(parse_compound_selector("*"), Some(vec![SimplePart::Universal]));
    }

    #[test]
    fn parses_compound_tag_class_id_selector() {
        assert_eq!(
            parse_compound_selector("div.foo#bar"),
            Some(vec![
                SimplePart::Tag("div".to_string()),
                SimplePart::Class("foo".to_string()),
                SimplePart::Id("bar".to_string()),
            ])
        );
    }

    #[test]
    fn matching_requires_every_part_to_match() {
        let el = FakeElement { tag: "div", classes: vec!["foo", "bar"], id: Some("x") };
        assert!(matches(&parse_compound_selector("div.foo#x").unwrap(), &el));
        assert!(!matches(&parse_compound_selector("span.foo#x").unwrap(), &el));
        assert!(!matches(&parse_compound_selector("div.missing").unwrap(), &el));
        assert!(matches(&parse_compound_selector("*").unwrap(), &el));
    }

    #[test]
    fn specificity_orders_id_over_class_over_tag() {
        let id_sel = parse_compound_selector("#x").unwrap();
        let class_sel = parse_compound_selector(".x").unwrap();
        let tag_sel = parse_compound_selector("div").unwrap();
        assert!(specificity(&id_sel) > specificity(&class_sel));
        assert!(specificity(&class_sel) > specificity(&tag_sel));
    }
}
