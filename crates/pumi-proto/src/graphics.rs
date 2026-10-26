use std::borrow::Cow;
pub fn da1_with_sixel(replies: &[u8]) -> Cow<'_, [u8]> {
    const START: &[u8] = b"\x1b[?";
    let Some(at) = replies.windows(START.len()).position(|w| w == START) else { return Cow::Borrowed(replies) };
    let params_at = at + START.len();
    let Some(len) = replies[params_at..].iter().position(|b| !(b.is_ascii_digit() || *b == b';')) else {
        return Cow::Borrowed(replies);
    };
    let params = &replies[params_at..params_at + len];
    let rest = &replies[params_at + len..];
    let mut out = replies[..params_at].to_vec();
    if rest.first() != Some(&b'c') || params.is_empty() || params.split(|b| *b == b';').skip(1).any(|p| p == b"4") {
        out.extend_from_slice(params);
        out.extend_from_slice(&da1_with_sixel(rest));
        return Cow::Owned(out);
    }
    let level = params.split(|b| *b == b';').next().unwrap_or_default();
    out.extend_from_slice(level);
    out.extend_from_slice(b";4");
    out.extend_from_slice(&params[level.len()..]);
    out.extend_from_slice(&da1_with_sixel(rest));
    Cow::Owned(out)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn da1_says_sixel() {
        assert_eq!(&*da1_with_sixel(b"\x1b[?62;22c"), b"\x1b[?62;4;22c");
        assert_eq!(&*da1_with_sixel(b"\x1b[?62c"), b"\x1b[?62;4c");
        assert_eq!(&*da1_with_sixel(b"\x1b[?62;4;22c"), b"\x1b[?62;4;22c", "once");
        assert_eq!(&*da1_with_sixel(b"\x1b[?1u\x1b[?62;22c"), b"\x1b[?1u\x1b[?62;4;22c", "after another reply");
        assert_eq!(&*da1_with_sixel(b"\x1b[>1;10;0c"), b"\x1b[>1;10;0c", "DA2 untouched");
        assert!(matches!(da1_with_sixel(b"plain"), Cow::Borrowed(_)));
    }
}
