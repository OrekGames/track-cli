#[cfg(test)]
mod tests {
    use crate::convert::convert_query_to_github;

    #[test]
    fn test_convert_query_to_github() {
        let cases = vec![
            ("", "is:issue"),
            ("   ", "is:issue"),
            ("bug", "bug is:issue"),
            ("project:owner/repo", "is:issue"),
            ("project:owner/repo bug", "bug is:issue"),
            ("#open", "is:open is:issue"),
            ("#unresolved", "is:open is:issue"),
            ("#closed", "is:closed is:issue"),
            ("#resolved", "is:closed is:issue"),
            ("#bug", "label:bug is:issue"),
            ("bug #open", "bug is:open is:issue"),
            ("some term #feature #open", "some term label:feature is:open is:issue"),
        ];

        for (input, expected) in cases {
            assert_eq!(
                convert_query_to_github(input),
                expected,
                "Failed for input: '{}'",
                input
            );
        }
    }
}
