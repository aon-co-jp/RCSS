//! `@media`のパース・評価。**正直なスコープ開示**: 以下のサブセットのみ
//! 対応する(CSS Media Queries仕様全体の再実装ではない)。
//!
//! 対応する構文: `@media <media-type> [and (<feature>: <value>)]*`
//!   - media-type: `screen` / `print` / `all`(省略時、または`all`
//!     指定時は、どの`MediaContext::media_type`にもマッチする)。
//!   - feature: `min-width` / `max-width` / `width`。値は`<数値>px`
//!     形式のみ(単位なしの数値・`em`/`rem`/`vw`等の他単位は非対応)。
//!   - featureの連結は`and`のみ対応(カンマ区切りのOR結合・`not`/
//!     `only`キーワード・`orientation`/`prefers-color-scheme`等の他の
//!     メディア特徴は非対応)。
//! 未対応のトークン(未知のmedia-type・未知のfeature名)が出現しても、
//! パース自体は失敗させず、そのトークンだけを黙って無視する
//! (ブロック全体を誤って無効化しない、安全側の簡略化——ただし
//! これは「未知の条件は常に真」ではなく「その条件を課さない」という
//! 意味であり、他の認識済み条件は引き続き適用される)。

/// `@media`のmedia-type。`None`(未指定/`all`)は「どの`MediaContext`にも
/// マッチする」を意味するため、`MediaQuery::media_type`側で`Option`として
/// 表現する。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaType {
    Screen,
    Print,
}

/// パース済みの`@media`条件。全フィールドがAND結合される
/// (`None`のフィールドは「条件なし」を意味する)。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MediaQuery {
    pub media_type: Option<MediaType>,
    pub min_width_px: Option<u32>,
    pub max_width_px: Option<u32>,
    pub width_px: Option<u32>,
}

/// カスケード計算時に渡す、現在の出力先メディア環境。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaContext {
    pub media_type: MediaType,
    pub viewport_width_px: u32,
}

impl Default for MediaContext {
    /// 既定値: `screen`・幅1920px(デスクトップ相当)。
    fn default() -> Self {
        MediaContext { media_type: MediaType::Screen, viewport_width_px: 1920 }
    }
}

/// `query`が`ctx`にマッチするかを判定する(全条件のAND)。
pub fn media_query_matches(query: &MediaQuery, ctx: &MediaContext) -> bool {
    if let Some(t) = query.media_type {
        if t != ctx.media_type {
            return false;
        }
    }
    if let Some(min) = query.min_width_px {
        if ctx.viewport_width_px < min {
            return false;
        }
    }
    if let Some(max) = query.max_width_px {
        if ctx.viewport_width_px > max {
            return false;
        }
    }
    if let Some(w) = query.width_px {
        if ctx.viewport_width_px != w {
            return false;
        }
    }
    true
}

fn parse_px(value: &str) -> Option<u32> {
    let value = value.trim().strip_suffix("px")?;
    value.trim().parse::<u32>().ok()
}

/// `@media`と`{`の間の文字列(例: `"screen and (min-width: 768px)"`)を
/// パースする。未対応のトークンは黙って無視する(モジュール冒頭コメント
/// 参照)。
pub fn parse_media_query(input: &str) -> MediaQuery {
    let mut query = MediaQuery::default();
    for token in input.split("and") {
        let token = token.trim();
        if token.is_empty() {
            continue;
        }
        if let Some(inner) = token.strip_prefix('(').and_then(|s| s.strip_suffix(')')) {
            let Some((key, value)) = inner.split_once(':') else { continue };
            match key.trim().to_ascii_lowercase().as_str() {
                "min-width" => query.min_width_px = parse_px(value),
                "max-width" => query.max_width_px = parse_px(value),
                "width" => query.width_px = parse_px(value),
                _ => {} // 未対応のfeature名は黙って無視(スコープ外)。
            }
        } else {
            match token.to_ascii_lowercase().as_str() {
                "screen" => query.media_type = Some(MediaType::Screen),
                "print" => query.media_type = Some(MediaType::Print),
                "all" => query.media_type = None,
                _ => {} // 未知のmedia-typeは黙って無視(条件を課さない扱い)。
            }
        }
    }
    query
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_media_type_only() {
        assert_eq!(parse_media_query("screen"), MediaQuery { media_type: Some(MediaType::Screen), ..Default::default() });
        assert_eq!(parse_media_query("print"), MediaQuery { media_type: Some(MediaType::Print), ..Default::default() });
    }

    #[test]
    fn parses_media_type_with_width_feature() {
        let q = parse_media_query("screen and (min-width: 768px)");
        assert_eq!(q, MediaQuery { media_type: Some(MediaType::Screen), min_width_px: Some(768), ..Default::default() });
    }

    #[test]
    fn min_width_matches_when_viewport_is_at_or_above_threshold() {
        let q = parse_media_query("(min-width: 768px)");
        assert!(media_query_matches(&q, &MediaContext { media_type: MediaType::Screen, viewport_width_px: 768 }));
        assert!(media_query_matches(&q, &MediaContext { media_type: MediaType::Screen, viewport_width_px: 1024 }));
        assert!(!media_query_matches(&q, &MediaContext { media_type: MediaType::Screen, viewport_width_px: 500 }));
    }

    #[test]
    fn media_type_mismatch_excludes_regardless_of_width() {
        let q = parse_media_query("print");
        assert!(!media_query_matches(&q, &MediaContext { media_type: MediaType::Screen, viewport_width_px: 1920 }));
        assert!(media_query_matches(&q, &MediaContext { media_type: MediaType::Print, viewport_width_px: 1920 }));
    }
}
