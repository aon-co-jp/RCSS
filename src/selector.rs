//! セレクタとその優先度(specificity)計算。単純セレクタの組み合わせ
//! (`div.foo#bar`のようなタグ名/クラス/IDの組み合わせ)に加え、
//! **子孫結合子(スペース区切り、例: `div p`)**を対応する(2026-07-18
//! 追加)。子結合子(`>`)・隣接兄弟結合子(`+`)・一般兄弟結合子(`~`)は
//! 次段階の課題として明記(`cssparser`が`Parser<'i,'t>`でライフタイム
//! 分離するのと同様、複雑な結合子は一からスコープを広げていく設計)。

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

/// スペース区切りのコンパウンドセレクタ列(子孫結合子)。
/// 例: `div p.foo` は`[Tag(div), [Tag(p), Class(foo)]]`のように、
/// 左から右へ祖先→子孫の順で並ぶ。最後の要素がマッチ対象の要素
/// そのものに対する条件、それより前の要素は祖先のどこかに存在すべき
/// 条件を表す(`matches_selector`参照)。
pub type Selector = Vec<CompoundSelector>;

/// カンマを含まない1つのセレクタ文字列(空白区切りの子孫結合子を含む
/// 場合がある)をパースする。
pub fn parse_selector(input: &str) -> Option<Selector> {
    let compounds: Vec<CompoundSelector> =
        input.split_whitespace().filter_map(parse_compound_selector).collect();
    if compounds.is_empty() {
        None
    } else {
        Some(compounds)
    }
}

/// `Selector`(子孫結合子を含みうる)のspecificityは、構成する各
/// コンパウンドセレクタのspecificityの合計(CSS仕様通り)。
pub fn selector_specificity(selector: &Selector) -> (u32, u32, u32) {
    selector.iter().fold((0, 0, 0), |(ai, ac, at), compound| {
        let (id, class, tag) = specificity(compound);
        (ai + id, ac + class, at + tag)
    })
}

/// `Selector`が要素`el`にマッチするかを判定する。`ancestors`は
/// 直近の親から順にルートに向かう祖先の並び(`ancestors[0]`が親)。
/// 子孫結合子は「直接の親である必要はなく、祖先のどこかに一致する
/// 要素が(セレクタの並び順を保って)存在すればよい」というCSSの
/// セマンティクスを、右から左へ祖先チェーンを消費する形で実装する。
pub fn matches_selector<E: ElementLike + ?Sized>(selector: &Selector, el: &E, ancestors: &[&E]) -> bool {
    let Some((target, ancestor_selectors)) = selector.split_last() else {
        return false;
    };
    if !matches(target, el) {
        return false;
    }

    let mut remaining = ancestor_selectors;
    let mut chain_start = 0usize;
    while let Some((needle, rest)) = remaining.split_last() {
        let found = ancestors[chain_start.min(ancestors.len())..].iter().position(|a| matches(needle, *a));
        match found {
            Some(offset) => {
                chain_start += offset + 1;
                remaining = rest;
            }
            None => return false,
        }
    }
    true
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

    #[test]
    fn parses_descendant_combinator_selector() {
        assert_eq!(
            parse_selector("div p.foo"),
            Some(vec![
                vec![SimplePart::Tag("div".to_string())],
                vec![SimplePart::Tag("p".to_string()), SimplePart::Class("foo".to_string())],
            ])
        );
    }

    #[test]
    fn descendant_combinator_matches_indirect_ancestor() {
        let root = FakeElement { tag: "div", classes: vec![], id: None };
        let middle = FakeElement { tag: "section", classes: vec![], id: None };
        let target = FakeElement { tag: "p", classes: vec![], id: None };
        let selector = parse_selector("div p").unwrap();
        // ancestors[0] = immediate parent, ancestors[1] = grandparent, etc.
        assert!(matches_selector(&selector, &target, &[&middle, &root]));
    }

    #[test]
    fn descendant_combinator_rejects_when_ancestor_missing() {
        let middle = FakeElement { tag: "section", classes: vec![], id: None };
        let target = FakeElement { tag: "p", classes: vec![], id: None };
        let selector = parse_selector("div p").unwrap();
        assert!(!matches_selector(&selector, &target, &[&middle]));
    }

    #[test]
    fn descendant_combinator_requires_selector_order_to_be_preserved() {
        // "div p" が祖先チェーンに`div`と`p`両方あっても、順序が
        // 逆(先祖側にpがあり、divがそれより内側)なら一致しない。
        let outer_p = FakeElement { tag: "p", classes: vec![], id: None };
        let inner_div = FakeElement { tag: "div", classes: vec![], id: None };
        let target = FakeElement { tag: "span", classes: vec![], id: None };
        let selector = parse_selector("div p span").unwrap();
        // ancestors[0] = immediate parent(inner_div), ancestors[1] = outer_p
        // "p" (2番目のセレクタ)を探すが、inner_divの外側(outer_p)にしか
        // 存在せず、その後ろに"div"を探しても見つからないので不一致。
        assert!(!matches_selector(&selector, &target, &[&inner_div, &outer_p]));
    }
}
