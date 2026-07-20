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
  昇順ソートし、宣言を順に適用(後勝ち)する`compute_style()`
  (2026-07-18: 第3引数に祖先チェーン`ancestors: &[&E]`を追加、
  子孫結合子のマッチングに使う)。`style_to_string()`でHTML `style`
  属性用の文字列に変換。
- `src/selector.rs`(2026-07-18更新): **子孫結合子(スペース区切り、
  例: `div p`)対応**。`Selector = Vec<CompoundSelector>`型、
  `parse_selector()`・`selector_specificity()`(各コンパウンドの
  specificityの合計)・`matches_selector()`(祖先チェーンを右から左へ
  消費して判定)を追加。
- **2026-07-18追記: 子結合子(`>`)・隣接兄弟結合子(`+`)対応完了**。
  `Selector`型を`Vec<CompoundSelector>`から`Vec<SelectorSegment>`
  (各segmentが「1つ左隣とどう関係するか」を表す`Combinator`
  (`Descendant`/`Child`/`AdjacentSibling`)を保持)へ変更(破壊的変更、
  依存クレート`RReact`/`RBootStrap`も追従修正済み)。`parse_selector`は
  `>`/`+`の前後にスペースが無い表記(`div>p`)にも対応。
  `matches_selector`/`compute_style`に`preceding_siblings`引数を追加
  (`+`のマッチングに使う、直前の兄弟から順に並べた配列)。
  **正直なスコープの限界**: `+`はセレクタの最も右側の結合でのみ判定
  可能(例: `li + li`)。`div + p span`のように`+`がそれより左側
  (祖先チェーン側)に現れる場合、祖先の兄弟情報がそもそも呼び出し側
  から渡されないため判定不能——安全側に倒して常に不一致を返す
  (`selector.rs`冒頭コメント参照)。
- **2026-07-19追記: 一般兄弟結合子(`~`)対応完了**。`Combinator`に
  `GeneralSibling`を追加、`parse_selector`は`~`の前後にスペースが
  無い表記(`.a~.b`)にも対応。マッチングは`+`と違い
  `preceding_siblings`を1件目だけでなく全件スキャンし、いずれか一致
  すれば真(直前である必要はない——CSS仕様通り)。
  **正直なスコープの限界(`+`と共通)**: `matches_selector`が受け取る
  `preceding_siblings`はtarget要素自身のものだけなので、`~`もセレクタ
  最右の結合でのみ判定可能。`div ~ p span`のように左側(祖先チェーン側)
  に現れる場合は`+`と同じ理由で判定不能——安全側に倒して不一致を返す
  (`selector.rs`冒頭コメント参照)。依存クレート`RReact`/`RBootStrap`は
  `Combinator`を網羅的にmatchしていない(構築のみ)ため、変更不要
  ・両クレートとも無変更でビルド/テスト green を確認済み。
- **未対応(次段階)**: `@media`等のat-rule、`!important`、
  カスケードレイヤー(`@layer`)、CSS変数、レイアウト計算
  (flexbox/grid)、深い位置での`+`/`~`結合子(上記参照)。
- **検証**: `cargo test`で32件全green(既存27件+一般兄弟結合子の
  パース2件・マッチング3件)。警告0件。依存先の`RBootStrap`
  (22件、無変更)・`RReact`(`dom_bridge`フィーチャ有効時16件、
  無効時10件、無変更)も影響なし(`Combinator`を網羅的にmatchしておらず
  構築のみのため)を確認済み。

## 次にすべきこと

1. 「RHTML5+RCSS3を使った最小のPoem SSRエンドポイント」マイルストーン
   (`rhtml5`側CLAUDE.md参照)
2. `@media`等のat-rule、`!important`
3. 深い位置での`+`/`~`結合子対応(祖先の兄弟情報を渡せるAPI設計の検討)

## 関連プロジェクト

- [rhtml5](https://github.com/aon-co-jp/rhtml5) — 対になるDOM実装、
  全体構想の詳細はこちらに集約
- [rreact](https://github.com/aon-co-jp/RReact) — `dom_bridge`
  フィーチャで`rhtml5::Element`が`rcss3::ElementLike`を実装するアダプタ
  (`ElementRef`)を提供し、RHTML→RCSS→RReactのEnd-to-Endパイプラインを
  実装済み(2026-07-18、詳細はRReact側CLAUDE.md参照)
- [open-raid-z](https://github.com/aon-co-jp/open-raid-z) — 開発ルールの正本

## HANDOFF

- **2026-07-19(続き) `!important`/`@media`をcompute_styleへ実配線
  (未完成の発見と修正)**: `audiocafe-tokyo-rust`ユーザーからの
  エコシステム完成度向上要望を受けて監査したところ、2つの問題が
  見つかった。(1) 前回セッションが`!important`/`@media`のパーサー・
  データモデル(`src/media.rs`新設・`parser.rs`更新・`lib.rs`の
  re-export)を完成させていたにもかかわらず、**一度もコミット・push
  されていなかった**(このファイルの「未対応」記載もそのため古いままに
  なっていた)。(2) さらに、そのデータモデル自体は完成していても、
  実際にスタイルを計算する`cascade::compute_style`が
  `Declaration.important`/`Rule.media`を一切参照しておらず、
  パースはされるが計算結果に何の影響も与えない「配線されていない」
  状態だった。両方を修正: `compute_style`に2パス適用
  (非important→important)による優先順位と、`MediaContext::default()`
  (screen・1920px)に対する`@media`フィルタリングを実装。新規テスト
  6件追加、既存分と合わせ42件全green・警告0件。依存クレート
  `RBootStrap`は`Declaration`/`Rule`の新フィールドに追従しておらず
  ビルドが壊れていたため、`important: false`/`media: None`を追加して
  修正した(RBootStrap側CLAUDE.md参照)。`RReact`の`compute_style`
  呼び出し(`dom_bridge.rs`)はシグネチャ変更が無いため無修正で
  ビルド・テストとも影響なしを確認済み。
  次にすべきこと: 深い位置での`+`/`~`結合子対応、カスケードレイヤー
  (`@layer`)、CSS変数、レイアウト計算(flexbox/grid)。

- **2026-07-19 一般兄弟結合子(`~`)対応**: `Combinator::GeneralSibling`
  追加、`parse_selector`が`~`を認識、`matches_selector`は
  `preceding_siblings`を全件スキャンして「直前でなくてもよい」
  マッチングを実装(`+`との違い)。深い位置での`~`は`+`と同じ理由で
  スコープ外(安全側に倒して不一致)。テスト27件→32件(全green、
  警告0件)。`RBootStrap`/`RReact`は`Combinator`を網羅的にmatchして
  いないため無変更・影響なし(両クレートとも既存テスト数のまま
  green確認済み)。次にすべきこと: `@media`/`!important`対応、
  深い位置での`+`/`~`結合子対応(API設計の見直しが必要)。

- **2026-07-18 子孫結合子(スペース区切りセレクタ)対応**: `Selector`
  型(`Vec<CompoundSelector>`)・`parse_selector`・`selector_specificity`・
  `matches_selector`(祖先チェーンを右から左へ消費するアルゴリズム)を
  追加。`Rule.selectors`の型を`Vec<CompoundSelector>`から
  `Vec<Selector>`へ変更(破壊的変更、`parser.rs`/`cascade.rs`両方を
  追従修正)。`compute_style`に祖先チェーン引数を追加(既存呼び出しは
  `&[]`で後方互換)。テストは13件→20件(全green、警告0件)。
  併せて、次のステップとして明記されていた「`rhtml5::Element`への
  `ElementLike`アダプタ」は、orphan ruleの制約により本クレートではなく
  `rreact`クレート側の`dom_bridge`フィーチャで実装した(利用側クレートで
  実装する、という本ファイルの元々の方針通り)。
  次にすべきこと: 子結合子(`>`)・隣接兄弟結合子(`+`)対応、
  `@media`/`!important`対応。
