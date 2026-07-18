//! カスケード計算。ある要素にマッチする全ルールを集め、
//! (specificity, ソース順)で昇順ソートしてから宣言を順に適用する
//! ことで、「同点なら後勝ち・specificityが高い方が勝つ」という
//! CSSの基本原則を再現する(`!important`・詳細度の入れ子計算・
//! カスケードレイヤー(`@layer`)等は次段階の課題として明記)。

use std::collections::BTreeMap;

use crate::parser::Rule;
use crate::selector::{matches, specificity, ElementLike};

/// 計算済みスタイル(プロパティ名→値)。`BTreeMap`を使うことで
/// `style_to_string`の出力順が決定的になる(テスト・SSR出力の
/// 再現性のため)。
pub type ComputedStyle = BTreeMap<String, String>;

pub fn compute_style<E: ElementLike + ?Sized>(stylesheet: &[Rule], el: &E) -> ComputedStyle {
    let mut matched: Vec<(u32, u32, u32, usize, &Vec<crate::parser::Declaration>)> = Vec::new();

    for (index, rule) in stylesheet.iter().enumerate() {
        let best = rule.selectors.iter().filter(|sel| matches(sel, el)).map(specificity).max();
        if let Some((ids, classes, tags)) = best {
            matched.push((ids, classes, tags, index, &rule.declarations));
        }
    }

    // (specificity, ソース順)昇順にソート → 後で適用したものが勝つ、
    // という単純な「上書き」ループでカスケードを実現する。
    matched.sort_by_key(|(ids, classes, tags, index, _)| (*ids, *classes, *tags, *index));

    let mut style = ComputedStyle::new();
    for (_, _, _, _, declarations) in matched {
        for decl in declarations {
            style.insert(decl.property.clone(), decl.value.clone());
        }
    }
    style
}

/// 計算済みスタイルを、HTML`style`属性にそのまま入れられる
/// `"prop: value; prop2: value2;"`形式の文字列へ変換する。
pub fn style_to_string(style: &ComputedStyle) -> String {
    style.iter().map(|(k, v)| format!("{k}: {v};")).collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_stylesheet;

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
    fn higher_specificity_wins_regardless_of_source_order() {
        // タグセレクタが後に書かれていても、IDセレクタ(specificityが
        // 高い)の宣言が勝つべき。
        let css = "#x { color: blue; } div { color: red; }";
        let rules = parse_stylesheet(css);
        let el = FakeElement { tag: "div", classes: vec![], id: Some("x") };
        let style = compute_style(&rules, &el);
        assert_eq!(style.get("color"), Some(&"blue".to_string()));
    }

    #[test]
    fn later_rule_wins_when_specificity_is_equal() {
        let css = ".a { color: red; } .b { color: blue; }";
        let rules = parse_stylesheet(css);
        let el = FakeElement { tag: "div", classes: vec!["a", "b"], id: None };
        let style = compute_style(&rules, &el);
        assert_eq!(style.get("color"), Some(&"blue".to_string()));
    }

    #[test]
    fn non_matching_rules_are_excluded() {
        let css = "span { color: red; }";
        let rules = parse_stylesheet(css);
        let el = FakeElement { tag: "div", classes: vec![], id: None };
        let style = compute_style(&rules, &el);
        assert!(style.is_empty());
    }

    #[test]
    fn distinct_properties_from_different_rules_are_merged() {
        let css = "div { color: red; } .foo { font-size: 12px; }";
        let rules = parse_stylesheet(css);
        let el = FakeElement { tag: "div", classes: vec!["foo"], id: None };
        let style = compute_style(&rules, &el);
        assert_eq!(style.get("color"), Some(&"red".to_string()));
        assert_eq!(style.get("font-size"), Some(&"12px".to_string()));
    }

    #[test]
    fn style_to_string_produces_deterministic_output() {
        let mut style = ComputedStyle::new();
        style.insert("color".to_string(), "red".to_string());
        style.insert("font-size".to_string(), "12px".to_string());
        assert_eq!(style_to_string(&style), "color: red; font-size: 12px;");
    }
}
