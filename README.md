# rcss3

CSS3相当のパーサー/カスケード/スタイル計算エンジンを一から開発するプロジェクト。
[rhtml5](https://github.com/aon-co-jp/rhtml5)と対になる(`RHTML5`/`RCSS3`/`RTypeScript`/`RBootStrap`構想の一部)。

## 使用例

```rust
use rcss3::{compute_style, parse_stylesheet, style_to_string, ElementLike};

struct MyElement { tag: String, classes: Vec<String>, id: Option<String> }
impl ElementLike for MyElement {
    fn tag_name(&self) -> &str { &self.tag }
    fn classes(&self) -> Vec<&str> { self.classes.iter().map(String::as_str).collect() }
    fn id(&self) -> Option<&str> { self.id.as_deref() }
}

let rules = parse_stylesheet("p { color: red; } .highlight { font-weight: bold; } div p { color: blue; }");
let el = MyElement { tag: "p".into(), classes: vec!["highlight".into()], id: None };
// 第3引数は祖先チェーン(直近の親から順に)。子孫結合子(`div p`)を
// 使わないなら`&[]`でよい。
let style = compute_style(&rules, &el, &[]);
println!("{}", style_to_string(&style)); // "color: red; font-weight: bold;"
```

## ビルド・テスト

```bash
cargo test
```

## ライセンス

Apache-2.0 OR MIT
