//! カスケード計算。ある要素にマッチする全ルールを集め、
//! (specificity, ソース順)で昇順ソートしてから宣言を順に適用する
//! ことで、「同点なら後勝ち・specificityが高い方が勝つ」という
//! CSSの基本原則を再現する。`!important`(2026-07-19実装: 非important
//! 宣言を全て適用した後、important宣言だけをもう一度同じ順序ルールで
//! 適用し直すことで、「specificityに関わらずimportantが常に勝つ」を
//! 再現)と`@media`(2026-07-19実装: `rule.media`が`Some`の場合、
//! `MediaContext::default()`(screen・1920px)に対して
//! `media_query_matches`で評価し、マッチしないルールはカスケードから
//! 除外)にも対応済み。カスケードレイヤー(`@layer`)は引き続き次段階の
//! 課題。
//!
//! **経緯(正直な開示)**: `Rule.media`・`Declaration.important`という
//! データモデルとパーサー対応(`parser.rs`)・`media_query_matches`
//! (`media.rs`)は先行して実装されていたが、本関数(実際にカスケード
//! 計算へ適用する箇所)は未着手のまま放置されていた——値をパースする
//! だけで実際の計算に一切使われない「配線されていない」状態だった。
//! 依存クレート`RBootStrap`は`Declaration`/`Rule`の新フィールドに
//! 追従していなかったためビルドが壊れていた(`important: false`・
//! `media: None`を追加して修正済み、RBootStrap側CLAUDE.md参照)。

use std::collections::BTreeMap;

use crate::media::{media_query_matches, MediaContext};
use crate::parser::Rule;
use crate::selector::{matches_selector, selector_specificity, ElementLike};

/// 計算済みスタイル(プロパティ名→値)。`BTreeMap`を使うことで
/// `style_to_string`の出力順が決定的になる(テスト・SSR出力の
/// 再現性のため)。
pub type ComputedStyle = BTreeMap<String, String>;

/// `el`に対する計算済みスタイルを求める。`ancestors`は子孫結合子・
/// 子結合子のマッチングに使う祖先チェーン(`ancestors[0]`が直近の親、
/// 以降ルート方向へ向かう)。`preceding_siblings`は隣接兄弟結合子
/// (`+`)のマッチングに使う直前の兄弟列(`preceding_siblings[0]`が
/// 直前の兄弟)。祖先・兄弟のいずれも辿らないシンプルなセレクタしか
/// 使わない場合はどちらも`&[]`を渡せばよい。`@media`条件付きルールは
/// `MediaContext::default()`(screen・1920px、デスクトップ相当)を
/// 出力先メディア環境とみなして評価する。
pub fn compute_style<E: ElementLike + ?Sized>(
    stylesheet: &[Rule],
    el: &E,
    ancestors: &[&E],
    preceding_siblings: &[&E],
) -> ComputedStyle {
    let media_ctx = MediaContext::default();
    let mut matched: Vec<(u32, u32, u32, usize, &Vec<crate::parser::Declaration>)> = Vec::new();

    for (index, rule) in stylesheet.iter().enumerate() {
        if let Some(query) = &rule.media {
            if !media_query_matches(query, &media_ctx) {
                continue;
            }
        }
        let best = rule
            .selectors
            .iter()
            .filter(|sel| matches_selector(sel, el, ancestors, preceding_siblings))
            .map(selector_specificity)
            .max();
        if let Some((ids, classes, tags)) = best {
            matched.push((ids, classes, tags, index, &rule.declarations));
        }
    }

    // (specificity, ソース順)昇順にソート → 後で適用したものが勝つ、
    // という単純な「上書き」ループでカスケードを実現する。
    matched.sort_by_key(|(ids, classes, tags, index, _)| (*ids, *classes, *tags, *index));

    let mut style = ComputedStyle::new();
    // 第1パス: 非`!important`宣言を通常のカスケード順で適用。
    for (_, _, _, _, declarations) in &matched {
        for decl in declarations.iter().filter(|d| !d.important) {
            style.insert(decl.property.clone(), decl.value.clone());
        }
    }
    // 第2パス: `!important`宣言を同じソート順で再適用し、非important
    // 宣言を(specificityに関わらず)常に上書きする——CSS本来の
    // 「importantは独立したカスケード層として最優先」という
    // セマンティクスを、既存のソート済み`matched`を再利用する形で再現。
    for (_, _, _, _, declarations) in &matched {
        for decl in declarations.iter().filter(|d| d.important) {
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
        let style = compute_style(&rules, &el, &[], &[]);
        assert_eq!(style.get("color"), Some(&"blue".to_string()));
    }

    #[test]
    fn later_rule_wins_when_specificity_is_equal() {
        let css = ".a { color: red; } .b { color: blue; }";
        let rules = parse_stylesheet(css);
        let el = FakeElement { tag: "div", classes: vec!["a", "b"], id: None };
        let style = compute_style(&rules, &el, &[], &[]);
        assert_eq!(style.get("color"), Some(&"blue".to_string()));
    }

    #[test]
    fn non_matching_rules_are_excluded() {
        let css = "span { color: red; }";
        let rules = parse_stylesheet(css);
        let el = FakeElement { tag: "div", classes: vec![], id: None };
        let style = compute_style(&rules, &el, &[], &[]);
        assert!(style.is_empty());
    }

    #[test]
    fn distinct_properties_from_different_rules_are_merged() {
        let css = "div { color: red; } .foo { font-size: 12px; }";
        let rules = parse_stylesheet(css);
        let el = FakeElement { tag: "div", classes: vec!["foo"], id: None };
        let style = compute_style(&rules, &el, &[], &[]);
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

    #[test]
    fn descendant_combinator_matches_through_ancestor_chain() {
        let css = "div p { color: green; }";
        let rules = parse_stylesheet(css);
        let ancestor = FakeElement { tag: "div", classes: vec![], id: None };
        let el = FakeElement { tag: "p", classes: vec![], id: None };
        let style = compute_style(&rules, &el, &[&ancestor], &[]);
        assert_eq!(style.get("color"), Some(&"green".to_string()));
    }

    #[test]
    fn descendant_combinator_does_not_match_without_matching_ancestor() {
        let css = "div p { color: green; }";
        let rules = parse_stylesheet(css);
        let ancestor = FakeElement { tag: "section", classes: vec![], id: None };
        let el = FakeElement { tag: "p", classes: vec![], id: None };
        let style = compute_style(&rules, &el, &[&ancestor], &[]);
        assert!(style.is_empty());
    }

    #[test]
    fn child_combinator_only_matches_the_immediate_parent() {
        let css = "div > p { color: purple; }";
        let rules = parse_stylesheet(css);
        let parent = FakeElement { tag: "div", classes: vec![], id: None };
        let el = FakeElement { tag: "p", classes: vec![], id: None };
        let style = compute_style(&rules, &el, &[&parent], &[]);
        assert_eq!(style.get("color"), Some(&"purple".to_string()));

        // grandparentがdivでも、直接の親(section)は`>`条件を満たさない。
        let grandparent = FakeElement { tag: "div", classes: vec![], id: None };
        let non_matching_parent = FakeElement { tag: "section", classes: vec![], id: None };
        let style2 = compute_style(&rules, &el, &[&non_matching_parent, &grandparent], &[]);
        assert!(style2.is_empty());
    }

    #[test]
    fn important_declaration_wins_over_higher_specificity_non_important() {
        // #x(specificity高)が非important、.aは通常なら負けるはずだが
        // !importantが付いているので勝つべき。
        let css = "#x { color: blue; } .a { color: red !important; }";
        let rules = parse_stylesheet(css);
        let el = FakeElement { tag: "div", classes: vec!["a"], id: Some("x") };
        let style = compute_style(&rules, &el, &[], &[]);
        assert_eq!(style.get("color"), Some(&"red".to_string()));
    }

    #[test]
    fn later_important_wins_over_earlier_important_at_equal_specificity() {
        let css = ".a { color: red !important; } .b { color: blue !important; }";
        let rules = parse_stylesheet(css);
        let el = FakeElement { tag: "div", classes: vec!["a", "b"], id: None };
        let style = compute_style(&rules, &el, &[], &[]);
        assert_eq!(style.get("color"), Some(&"blue".to_string()));
    }

    #[test]
    fn non_important_properties_are_unaffected_by_unrelated_important_declarations() {
        let css = "div { color: red !important; } .foo { font-size: 12px; }";
        let rules = parse_stylesheet(css);
        let el = FakeElement { tag: "div", classes: vec!["foo"], id: None };
        let style = compute_style(&rules, &el, &[], &[]);
        assert_eq!(style.get("color"), Some(&"red".to_string()));
        assert_eq!(style.get("font-size"), Some(&"12px".to_string()));
    }

    #[test]
    fn media_query_that_matches_default_context_is_applied() {
        // MediaContext::default()はscreen・1920pxなので、
        // screenかつmin-width:768pxの条件はマッチするはず。
        let css = "@media screen and (min-width: 768px) { .a { color: green; } }";
        let rules = parse_stylesheet(css);
        let el = FakeElement { tag: "div", classes: vec!["a"], id: None };
        let style = compute_style(&rules, &el, &[], &[]);
        assert_eq!(style.get("color"), Some(&"green".to_string()));
    }

    #[test]
    fn media_query_that_does_not_match_default_context_is_excluded() {
        // デスクトップ既定(1920px)ではmax-width:480pxにマッチしない
        // ので、このルールはカスケードから除外されるべき。
        let css = "@media (max-width: 480px) { .a { color: green; } }";
        let rules = parse_stylesheet(css);
        let el = FakeElement { tag: "div", classes: vec!["a"], id: None };
        let style = compute_style(&rules, &el, &[], &[]);
        assert!(style.get("color").is_none());
    }

    #[test]
    fn print_media_query_is_excluded_under_default_screen_context() {
        let css = "@media print { .a { color: green; } }";
        let rules = parse_stylesheet(css);
        let el = FakeElement { tag: "div", classes: vec!["a"], id: None };
        let style = compute_style(&rules, &el, &[], &[]);
        assert!(style.get("color").is_none());
    }

    #[test]
    fn adjacent_sibling_combinator_only_matches_the_immediately_preceding_sibling() {
        let css = "li + li { color: orange; }";
        let rules = parse_stylesheet(css);
        let preceding = FakeElement { tag: "li", classes: vec![], id: None };
        let el = FakeElement { tag: "li", classes: vec![], id: None };
        let style = compute_style(&rules, &el, &[], &[&preceding]);
        assert_eq!(style.get("color"), Some(&"orange".to_string()));

        let style_no_sibling = compute_style(&rules, &el, &[], &[]);
        assert!(style_no_sibling.is_empty());
    }
}
