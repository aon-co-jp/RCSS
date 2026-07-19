//! RCSS3 — CSS3相当のパーサー/カスケード/スタイル計算エンジンを、
//! 既存ブラウザエンジンのコードを一切流用せず一から開発するプロジェクト
//! (`RHTML5/RCSS3/RTypeScript/RBootStrap`構想の一部、2026-07-18)。
//! `rhtml5`とは疎結合(特定のDOM実装に直接依存しない、`ElementLike`
//! トレイト経由でマッチングする設計——`cssparser`がDOM非依存である
//! のと同じ設計判断)。
//!
//! ## 現状
//! パーサー(`parser`)・セレクタ/specificity(`selector`)・
//! カスケード計算(`cascade`)・`!important`・`@media`(`media`、
//! 対応サブセットは`media`モジュール冒頭コメント参照)。
//! レイアウト計算(flexbox/grid)・カスケードレイヤー(`@layer`)・
//! CSS変数は未着手。

pub mod cascade;
pub mod media;
pub mod parser;
pub mod selector;

pub use cascade::{compute_style, style_to_string, ComputedStyle};
pub use media::{media_query_matches, parse_media_query, MediaContext, MediaQuery, MediaType};
pub use parser::{parse_stylesheet, Declaration, Rule};
pub use selector::{
    matches, matches_selector, parse_selector, selector_specificity, specificity, Combinator, CompoundSelector,
    ElementLike, Selector, SelectorSegment, SimplePart,
};
