//! セレクタとその優先度(specificity)計算。単純セレクタの組み合わせ
//! (`div.foo#bar`のようなタグ名/クラス/IDの組み合わせ)に加え、
//! **子孫結合子(スペース区切り、例: `div p`)・子結合子(`>`)・
//! 隣接兄弟結合子(`+`)・一般兄弟結合子(`~`)**を対応する
//! (`~`は2026-07-19追加)。
//!
//! **隣接兄弟結合子(`+`)・一般兄弟結合子(`~`)共通のスコープの限界
//! (正直な開示)**: `matches_selector`は呼び出し側から`el`自身の直前の
//! 兄弟列(`preceding_siblings`)しか受け取らない設計のため、`+`/`~`が
//! セレクタの**最も右側の結合**(例: `li + li`・`li ~ li`)である場合のみ
//! 正しく判定できる。`div + p span`・`div ~ p span`のように`+`/`~`が
//! それより左側(祖先チェーン側)に現れる場合、祖先の兄弟情報がそもそも
//! 呼び出し側から渡されないため判定不能——安全側に倒して常に不一致
//! (`false`)を返す(黙って誤判定するより、明示的に「未対応」を選ぶ)。
//! 深い位置での兄弟結合子対応が必要になった場合は、`ancestors`を
//! フラットな配列ではなくDOM木参照に置き換える設計変更が必要。
//!
//! 一般兄弟結合子(`~`)自体の判定ロジックは、隣接兄弟結合子(`+`)より
//! むしろ単純: `+`は「直前の兄弟(1つだけ)」を見るのに対し、`~`は
//! `preceding_siblings`(target要素の直前の兄弟から順に並んだ配列)を
//! 全件スキャンし、いずれか1つでも一致すれば真——直前かどうかの位置は
//! 問わない。ただし上記の通り、この判定に使える`preceding_siblings`は
//! target要素自身のものしか渡されないため、チェーンの深い位置での`~`は
//! `+`と同様にスコープ外のまま。

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

/// コンパウンドセレクタ同士をつなぐ結合子。`SelectorSegment::combinator`
/// は「この segment が、1つ左隣の segment とどう関係するか」を表す
/// (先頭segmentの`combinator`は参照されない——左隣が存在しないため)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Combinator {
    /// スペース区切り(例: `div p`)。祖先のどこかに一致すればよい。
    Descendant,
    /// `>`(例: `div > p`)。直接の親でなければならない。
    Child,
    /// `+`(例: `li + li`)。直前の兄弟でなければならない。
    AdjacentSibling,
    /// `~`(例: `li ~ li`)。それより前の兄弟のいずれかでよい
    /// (直前である必要はない)。
    GeneralSibling,
}

/// 結合子付きの1コンパウンドセレクタ。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectorSegment {
    pub combinator: Combinator,
    pub compound: CompoundSelector,
}

/// 結合子で連結されたコンパウンドセレクタ列。
/// 例: `div > p.foo` は
/// `[{Descendant, [Tag(div)]}, {Child, [Tag(p), Class(foo)]}]`のように、
/// 左から右へ祖先→子孫の順で並ぶ。最後の要素がマッチ対象の要素
/// そのものに対する条件(`matches_selector`参照)。
pub type Selector = Vec<SelectorSegment>;

/// カンマを含まない1つのセレクタ文字列(空白区切りの子孫結合子・
/// `>`/`+`/`~`結合子を含む場合がある。スペース有無どちらの表記
/// (`div > p`/`div>p`)にも対応するため、`>`/`+`/`~`の前後に強制的に
/// 空白を挿入してからトークナイズする)。
pub fn parse_selector(input: &str) -> Option<Selector> {
    let spaced: String = input
        .chars()
        .flat_map(|c| if c == '>' || c == '+' || c == '~' { vec![' ', c, ' '] } else { vec![c] })
        .collect();

    let mut segments = Vec::new();
    let mut pending = Combinator::Descendant;
    for token in spaced.split_whitespace() {
        match token {
            ">" => pending = Combinator::Child,
            "+" => pending = Combinator::AdjacentSibling,
            "~" => pending = Combinator::GeneralSibling,
            _ => {
                let compound = parse_compound_selector(token)?;
                segments.push(SelectorSegment { combinator: pending, compound });
                pending = Combinator::Descendant;
            }
        }
    }

    if segments.is_empty() {
        None
    } else {
        Some(segments)
    }
}

/// `Selector`(結合子を含みうる)のspecificityは、構成する各
/// コンパウンドセレクタのspecificityの合計(CSS仕様通り。結合子自体は
/// specificityに寄与しない)。
pub fn selector_specificity(selector: &Selector) -> (u32, u32, u32) {
    selector.iter().fold((0, 0, 0), |(ai, ac, at), seg| {
        let (id, class, tag) = specificity(&seg.compound);
        (ai + id, ac + class, at + tag)
    })
}

/// `Selector`が要素`el`にマッチするかを判定する。
///
/// - `ancestors`: 直近の親から順にルートに向かう祖先の並び
///   (`ancestors[0]`が親)。子孫結合子・子結合子の判定に使う。
/// - `preceding_siblings`: `el`の直前の兄弟から順に並べたもの
///   (`preceding_siblings[0]`が直前の兄弟)。隣接兄弟結合子・一般兄弟
///   結合子の判定に使う。祖先チェーンの深い位置での`+`/`~`はスコープ外
///   (モジュール冒頭コメント参照)——安全側に倒して不一致を返す。
pub fn matches_selector<E: ElementLike + ?Sized>(
    selector: &Selector,
    el: &E,
    ancestors: &[&E],
    preceding_siblings: &[&E],
) -> bool {
    let Some((target, rest)) = selector.split_last() else {
        return false;
    };
    if !matches(&target.compound, el) {
        return false;
    }

    let mut remaining = rest;
    let mut ancestor_cursor = 0usize;
    // `next_relation`は、直前に処理した segment(最初は`target`)が
    // 「1つ左隣の segment」とどう関係するかを表す
    // (= その左隣 segment 自身が持つ`combinator`フィールドの値)。
    let mut next_relation = target.combinator;
    // `+`/`~`はtarget自身から見た兄弟列(`preceding_siblings`)でしか
    // 判定できない(スコープ限界、モジュール冒頭コメント参照)——
    // ループ1周目だけ`true`。
    let mut at_target_position = true;

    while let Some((needle, rest2)) = remaining.split_last() {
        match next_relation {
            Combinator::Descendant => {
                let found =
                    ancestors[ancestor_cursor.min(ancestors.len())..].iter().position(|a| matches(&needle.compound, *a));
                match found {
                    Some(offset) => ancestor_cursor += offset + 1,
                    None => return false,
                }
            }
            Combinator::Child => {
                match ancestors.get(ancestor_cursor) {
                    Some(a) if matches(&needle.compound, *a) => ancestor_cursor += 1,
                    _ => return false,
                }
            }
            Combinator::AdjacentSibling => {
                if !at_target_position {
                    return false;
                }
                match preceding_siblings.first() {
                    Some(sib) if matches(&needle.compound, *sib) => {}
                    _ => return false,
                }
            }
            Combinator::GeneralSibling => {
                if !at_target_position {
                    return false;
                }
                if !preceding_siblings.iter().any(|sib| matches(&needle.compound, *sib)) {
                    return false;
                }
            }
        }
        next_relation = needle.combinator;
        at_target_position = false;
        remaining = rest2;
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
                SelectorSegment { combinator: Combinator::Descendant, compound: vec![SimplePart::Tag("div".to_string())] },
                SelectorSegment {
                    combinator: Combinator::Descendant,
                    compound: vec![SimplePart::Tag("p".to_string()), SimplePart::Class("foo".to_string())],
                },
            ])
        );
    }

    #[test]
    fn parses_child_and_adjacent_sibling_combinators_with_or_without_spaces() {
        let expected = vec![
            SelectorSegment { combinator: Combinator::Descendant, compound: vec![SimplePart::Tag("div".to_string())] },
            SelectorSegment { combinator: Combinator::Child, compound: vec![SimplePart::Tag("p".to_string())] },
        ];
        assert_eq!(parse_selector("div > p"), Some(expected.clone()));
        assert_eq!(parse_selector("div>p"), Some(expected));

        let expected_sibling = vec![
            SelectorSegment { combinator: Combinator::Descendant, compound: vec![SimplePart::Tag("li".to_string())] },
            SelectorSegment { combinator: Combinator::AdjacentSibling, compound: vec![SimplePart::Tag("li".to_string())] },
        ];
        assert_eq!(parse_selector("li + li"), Some(expected_sibling.clone()));
        assert_eq!(parse_selector("li+li"), Some(expected_sibling));
    }

    #[test]
    fn descendant_combinator_matches_indirect_ancestor() {
        let root = FakeElement { tag: "div", classes: vec![], id: None };
        let middle = FakeElement { tag: "section", classes: vec![], id: None };
        let target = FakeElement { tag: "p", classes: vec![], id: None };
        let selector = parse_selector("div p").unwrap();
        // ancestors[0] = immediate parent, ancestors[1] = grandparent, etc.
        assert!(matches_selector(&selector, &target, &[&middle, &root], &[]));
    }

    #[test]
    fn descendant_combinator_rejects_when_ancestor_missing() {
        let middle = FakeElement { tag: "section", classes: vec![], id: None };
        let target = FakeElement { tag: "p", classes: vec![], id: None };
        let selector = parse_selector("div p").unwrap();
        assert!(!matches_selector(&selector, &target, &[&middle], &[]));
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
        assert!(!matches_selector(&selector, &target, &[&inner_div, &outer_p], &[]));
    }

    #[test]
    fn child_combinator_matches_only_the_immediate_parent() {
        let grandparent = FakeElement { tag: "div", classes: vec![], id: None };
        let parent = FakeElement { tag: "section", classes: vec![], id: None };
        let target = FakeElement { tag: "p", classes: vec![], id: None };
        let selector = parse_selector("section > p").unwrap();
        assert!(matches_selector(&selector, &target, &[&parent, &grandparent], &[]));

        // "div > p" が要求する親は grandparent (div) であって、parent
        // (section) は間に挟まっているので子結合子には一致しない
        // (子孫結合子であれば一致するはずだが、`>`はそれを許さない)。
        let selector2 = parse_selector("div > p").unwrap();
        assert!(!matches_selector(&selector2, &target, &[&parent, &grandparent], &[]));
    }

    #[test]
    fn adjacent_sibling_combinator_matches_only_the_immediately_preceding_sibling() {
        let first_li = FakeElement { tag: "li", classes: vec![], id: None };
        let second_li = FakeElement { tag: "li", classes: vec![], id: None };
        let selector = parse_selector("li + li").unwrap();

        // second_li の直前の兄弟が first_li なので一致する。
        assert!(matches_selector(&selector, &second_li, &[], &[&first_li]));

        // 直前の兄弟が無ければ不一致。
        assert!(!matches_selector(&selector, &second_li, &[], &[]));

        // 直前の兄弟のタグが違えば不一致。
        let span_sibling = FakeElement { tag: "span", classes: vec![], id: None };
        assert!(!matches_selector(&selector, &second_li, &[], &[&span_sibling]));
    }

    #[test]
    fn adjacent_sibling_combinator_deeper_in_chain_is_out_of_scope_and_fails_safe() {
        // "div + p span" のように `+` が最も右側の結合ではない場合は、
        // 祖先の兄弟情報が呼び出し側から渡されないため判定不能——
        // 安全側に倒して常に不一致を返す(モジュール冒頭コメント参照)。
        let ancestor = FakeElement { tag: "p", classes: vec![], id: None };
        let target = FakeElement { tag: "span", classes: vec![], id: None };
        let selector = parse_selector("div + p span").unwrap();
        assert!(!matches_selector(&selector, &target, &[&ancestor], &[]));
    }

    #[test]
    fn parses_general_sibling_combinator_with_or_without_spaces() {
        let expected = vec![
            SelectorSegment { combinator: Combinator::Descendant, compound: vec![SimplePart::Class("a".to_string())] },
            SelectorSegment { combinator: Combinator::GeneralSibling, compound: vec![SimplePart::Class("b".to_string())] },
        ];
        assert_eq!(parse_selector(".a ~ .b"), Some(expected.clone()));
        assert_eq!(parse_selector(".a~.b"), Some(expected));
    }

    #[test]
    fn general_sibling_combinator_matches_any_earlier_sibling_not_just_adjacent() {
        // "li ~ li" は、直前の兄弟でなくても、それより前に一致する兄弟が
        // あれば真(`+`との違い)。preceding_siblings[0]が直前の兄弟。
        let first_li = FakeElement { tag: "li", classes: vec![], id: None };
        let span_between = FakeElement { tag: "span", classes: vec![], id: None };
        let target = FakeElement { tag: "li", classes: vec![], id: None };
        let selector = parse_selector("li ~ li").unwrap();

        // 直前の兄弟(span_between)は一致しないが、その前(first_li)が
        // 一致するので全体としては真。
        assert!(matches_selector(&selector, &target, &[], &[&span_between, &first_li]));
    }

    #[test]
    fn general_sibling_combinator_rejects_when_no_preceding_sibling_matches() {
        let span_sibling = FakeElement { tag: "span", classes: vec![], id: None };
        let target = FakeElement { tag: "li", classes: vec![], id: None };
        let selector = parse_selector("li ~ li").unwrap();

        // 兄弟が無ければ不一致。
        assert!(!matches_selector(&selector, &target, &[], &[]));
        // 前の兄弟がいずれもタグ違いなら不一致。
        assert!(!matches_selector(&selector, &target, &[], &[&span_sibling]));
    }

    #[test]
    fn general_sibling_combinator_deeper_in_chain_is_out_of_scope_and_fails_safe() {
        // "div ~ p span" のように `~` が最も右側の結合ではない場合は、
        // `+`と同じ理由(祖先の兄弟情報が渡されない)で判定不能——
        // 安全側に倒して常に不一致を返す(モジュール冒頭コメント参照)。
        let ancestor = FakeElement { tag: "p", classes: vec![], id: None };
        let target = FakeElement { tag: "span", classes: vec![], id: None };
        let selector = parse_selector("div ~ p span").unwrap();
        assert!(!matches_selector(&selector, &target, &[&ancestor], &[]));
    }
}
