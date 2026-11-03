//! One-based page input shared by native document controls and CLI requests.
pub(super) fn parse_page(text: &str, total: usize) -> Result<usize, String> {
    let text = text.trim();
    if text.is_empty() || !text.bytes().all(|c| c.is_ascii_digit()) {
        return Err(format!("1–{total} 사이의 페이지 번호를 입력하세요."));
    }
    text.parse::<usize>()
        .ok()
        .filter(|n| *n >= 1 && *n <= total)
        .ok_or_else(|| format!("1–{total} 사이의 페이지 번호를 입력하세요."))
}

#[cfg(test)]
mod tests {
    use super::parse_page;

    #[test]
    fn accepts_first_last_and_trimmed_page_numbers() {
        assert_eq!(parse_page("1", 12).unwrap(), 1);
        assert_eq!(parse_page("12", 12).unwrap(), 12);
        assert_eq!(parse_page(" 7 ", 12).unwrap(), 7);
    }

    #[test]
    fn rejects_non_integer_and_out_of_range_pages() {
        for input in [
            "",
            " ",
            "0",
            "-1",
            "+1",
            "1.5",
            "3x",
            "13",
            "9999999999999999999999999",
        ] {
            assert!(parse_page(input, 12).is_err(), "accepted {input:?}");
        }
        assert!(parse_page("1", 0).is_err());
    }
}
