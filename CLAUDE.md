# 開発方針＆開発環境ルール(rcss3)

作業ドライブは`F:\open-runo`。この節は[`open-raid-z`](https://github.com/aon-co-jp/open-raid-z)の`CLAUDE.md`を正本とし、各プロジェクトへコピーして同期する方針に準じる。

## このプロジェクトの構想(2026-07-18新設)

`RHTML5`/`RCSS3`/`RTypeScript`/`RBootStrap`という4プロジェクト構想の1つ。
詳細な全体構想・開発順序・マイルストーンは[`rhtml5`](https://github.com/aon-co-jp/rhtml5)
のCLAUDE.mdを参照(構想はrhtml5側に集約して記録)。

**設計判断**: `rcss3`は特定のDOM実装(`rhtml5::Element`等)に直接
依存しない。要素とのマッチングは`ElementLike`トレイト(`tag_name()`/
`classes()`/`id()`)経由で行う——`cssparser`(Servo由来)がDOM非依存で
あるのと同じ設計判断。これにより将来的に別のDOM実装と組み合わせる
自由度を保つ。

## 現状(第一段、2026-07-18)

- `src/selector.rs`: `SimplePart`(`Tag`/`Class`/`Id`/`Universal`)、
  `CompoundSelector`(単純セレクタの組み合わせ、結合子は未対応)、
  `ElementLike`トレイト、`matches()`、`specificity()`
  (標準的な(id数,class数,tag数)モデル)。
- `src/parser.rs`: コメント除去→`セレクタ { 宣言 }`ブロック分割→
  宣言(`property: value;`)パースの逐次スキャナ。カンマ区切りの
  複数セレクタ対応。
- `src/cascade.rs`: マッチする全ルールを(specificity, ソース順)で
  昇順ソートし、宣言を順に適用(後勝ち)する`compute_style()`。
  `style_to_string()`でHTML `style`属性用の文字列に変換。
- **未対応(次段階)**: 子孫/子/隣接兄弟結合子、`@media`等のat-rule、
  `!important`、カスケードレイヤー(`@layer`)、CSS変数、
  レイアウト計算(flexbox/grid)。
- **検証**: `cargo test`で13件全green(セレクタパース4件・
  スタイルシートパース4件・カスケード5件、specificityの優先順位・
  同点時の後勝ち・複数ルールのプロパティ統合を含む)。警告0件。

## 次にすべきこと

1. `rhtml5::Element`が`rcss3::ElementLike`を実装する薄いアダプタ
   (どちらのクレートにも新規の相互依存を追加しない形——利用側の
   クレート、またはこのアダプタ専用の第三のクレートで実装するのが
   cssparser/htm5everの実例に近い)
2. 「RHTML5+RCSS3を使った最小のPoem SSRエンドポイント」マイルストーン
   (`rhtml5`側CLAUDE.md参照)
3. 子孫結合子(スペース)の対応(コンビネータ全体のうち最優先で
   よく使われるもの)

## 関連プロジェクト

- [rhtml5](https://github.com/aon-co-jp/rhtml5) — 対になるDOM実装、
  全体構想の詳細はこちらに集約
- [open-raid-z](https://github.com/aon-co-jp/open-raid-z) — 開発ルールの正本
