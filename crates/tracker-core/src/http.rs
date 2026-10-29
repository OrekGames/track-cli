//! HTTP plumbing shared by the tracker clients.
//!
//! Each backend still builds its own requests: URLs, auth and other headers,
//! and which response bodies it decodes. This module holds what they all do
//! the same way: the agent, turning a non-2xx response into a
//! [`TrackerError`], decoding a JSON body, and attaching files to a multipart
//! form. Transport failures convert to [`TrackerError::Http`] through `?`.

use std::time::Duration;

use serde::de::DeserializeOwned;
use serde_json::Value;
use ureq::Agent;
use ureq::unversioned::multipart::{Form, Part};

use crate::{AttachmentUploadFile, Result, TrackerError};

/// A response from a tracker API.
pub type Response = ureq::http::Response<ureq::Body>;

/// HTTP agent for a tracker client: the roots from [`crate::tls::tls_config`],
/// a 30 second overall timeout, and non-2xx statuses handed back as responses
/// so [`check_response`] can read the API's error body.
pub fn agent() -> Agent {
    Agent::config_builder()
        .tls_config(crate::tls::tls_config())
        .timeout_global(Some(Duration::from_secs(30)))
        .http_status_as_error(false)
        .build()
        .into()
}

/// Pass a 2xx response through and turn any other status into an error.
///
/// 401 is [`TrackerError::Unauthorized`]; the body is read but its message is
/// dropped. Any other status is [`TrackerError::Api`] whose message is what
/// `extract_message` finds in the body parsed as JSON (used as is, even when
/// empty), else the body text verbatim, else `HTTP <status>` when the body is
/// empty. The body is read under ureq's 10 MB limit; only `text/*` bodies are
/// decoded lossily (invalid bytes become `?`), and any failed read, including
/// a non-text body that is not valid UTF-8, counts as an empty body. Status
/// rules of one API (a rate-limit header, a fixed message) belong in the
/// backend, around this call.
pub fn check_response(
    mut response: Response,
    extract_message: fn(&Value) -> Option<String>,
) -> Result<Response> {
    let status = response.status().as_u16();

    if (200..300).contains(&status) {
        return Ok(response);
    }

    // An unreadable error body counts as empty: the status still says what failed.
    let body = response
        .body_mut()
        .read_to_string()
        .unwrap_or_else(|_| String::new());

    let message = serde_json::from_str::<Value>(&body)
        .ok()
        .and_then(|json| extract_message(&json))
        .unwrap_or_else(|| {
            if body.is_empty() {
                format!("HTTP {status}")
            } else {
                body
            }
        });

    if status == 401 {
        Err(TrackerError::Unauthorized)
    } else {
        Err(TrackerError::Api { status, message })
    }
}

/// Decode a successful response's JSON body.
///
/// The body is read in full first, under ureq's 10 MB limit, so a failed or
/// oversized read is [`TrackerError::Http`]; only a body that is not the
/// expected JSON is [`TrackerError::Parse`]. Do not use ureq's
/// `Body::read_json` instead: on a body without a length it decodes from the
/// stream and reports read failures as JSON errors.
pub fn read_json<T: DeserializeOwned>(mut response: Response) -> Result<T> {
    let body = response.body_mut().read_to_vec()?;
    Ok(serde_json::from_slice(&body)?)
}

/// Multipart part holding `file`, named and typed from its path unless the
/// upload overrides them. A file that cannot be opened is
/// [`TrackerError::Io`]; an invalid MIME type is [`TrackerError::Http`], as
/// ureq reports it.
pub fn file_part<'a>(file: &AttachmentUploadFile) -> Result<Part<'a>> {
    let mut part = Part::file(&file.path)?;
    if let Some(name) = &file.name {
        part = part.file_name(name);
    }
    if let Some(mime_type) = &file.mime_type {
        part = part.mime_str(mime_type)?;
    }
    Ok(part)
}

/// Multipart form with each of `files` as a [`file_part`] named `field`.
pub fn file_form<'a>(field: &'a str, files: &[AttachmentUploadFile]) -> Result<Form<'a>> {
    files.iter().try_fold(Form::new(), |form, file| {
        Ok(form.part(field, file_part(file)?))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::path::PathBuf;

    /// A response body whose every read fails, like a connection dropped mid-body.
    struct FailingReader;

    impl std::io::Read for FailingReader {
        fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("connection reset"))
        }
    }

    fn response(status: u16, body: &str) -> Response {
        ureq::http::Response::builder()
            .status(status)
            .header("Content-Type", "application/json")
            .body(ureq::Body::builder().data(body))
            .unwrap()
    }

    fn unreadable_response(status: u16) -> Response {
        ureq::http::Response::builder()
            .status(status)
            .body(ureq::Body::builder().reader(FailingReader))
            .unwrap()
    }

    /// The extractor most backends use: a top-level string `message`.
    fn message(json: &Value) -> Option<String> {
        json.get("message")?.as_str().map(str::to_string)
    }

    /// The `Api` error `check_response` gives for `status` and `body`.
    fn api_error(status: u16, body: &str) -> (u16, String) {
        match check_response(response(status, body), message) {
            Err(TrackerError::Api { status, message }) => (status, message),
            other => panic!("expected an Api error, got {other:?}"),
        }
    }

    fn upload_file(path: impl Into<PathBuf>) -> AttachmentUploadFile {
        AttachmentUploadFile {
            path: path.into(),
            name: None,
            mime_type: None,
        }
    }

    fn existing_file() -> AttachmentUploadFile {
        upload_file(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"))
    }

    fn missing_file() -> AttachmentUploadFile {
        upload_file(concat!(env!("CARGO_MANIFEST_DIR"), "/does-not-exist.txt"))
    }

    #[test]
    fn check_response_passes_2xx_through() {
        let mut response = check_response(response(204, "x"), message).unwrap();

        assert_eq!(response.status().as_u16(), 204);
        // The body is left for the caller.
        assert_eq!(response.body_mut().read_to_string().unwrap(), "x");
    }

    #[test]
    fn check_response_401_is_unauthorized_even_with_message() {
        let result = check_response(response(401, r#"{"message":"Bad credentials"}"#), message);

        assert!(
            matches!(result, Err(TrackerError::Unauthorized)),
            "{result:?}"
        );
    }

    #[test]
    fn check_response_uses_extracted_message() {
        assert_eq!(
            api_error(400, r#"{"message":"boom"}"#),
            (400, "boom".to_string())
        );
    }

    #[test]
    fn check_response_keeps_empty_extracted_message() {
        assert_eq!(api_error(400, r#"{"message":""}"#), (400, String::new()));
    }

    #[test]
    fn check_response_json_without_message_uses_raw_body() {
        assert_eq!(
            api_error(422, r#"{"errors":[1]}"#),
            (422, r#"{"errors":[1]}"#.to_string())
        );
        assert_eq!(api_error(400, "[]"), (400, "[]".to_string()));
    }

    #[test]
    fn check_response_plain_text_body_is_verbatim() {
        assert_eq!(
            api_error(502, "<html>Bad Gateway</html>\n"),
            (502, "<html>Bad Gateway</html>\n".to_string())
        );
        assert_eq!(api_error(400, "   "), (400, "   ".to_string()));
    }

    #[test]
    fn check_response_empty_body_is_http_status() {
        assert_eq!(api_error(400, ""), (400, "HTTP 400".to_string()));
    }

    #[test]
    fn check_response_unreadable_body_is_http_status() {
        let result = check_response(unreadable_response(500), message);

        assert!(
            matches!(
                result,
                Err(TrackerError::Api { status: 500, ref message }) if message == "HTTP 500"
            ),
            "{result:?}"
        );
    }

    #[test]
    fn check_response_does_not_special_case_403_404_429() {
        for status in [403, 404, 429] {
            assert_eq!(
                api_error(status, r#"{"message":"m"}"#),
                (status, "m".to_string())
            );
        }
    }

    #[test]
    fn read_json_decodes_valid_json() {
        let value: Value = read_json(response(200, r#"{"a":1}"#)).unwrap();

        assert_eq!(value["a"], 1);
    }

    #[test]
    fn read_json_non_json_is_parse() {
        let result = read_json::<Value>(response(200, "not-json"));

        assert!(matches!(result, Err(TrackerError::Parse(_))), "{result:?}");
    }

    #[test]
    fn read_json_failed_read_is_http() {
        // No Content-Length: ureq's own read_json would decode from the stream
        // and report this read failure as a JSON error.
        let result = read_json::<Value>(unreadable_response(200));

        assert!(
            matches!(result, Err(TrackerError::Http(ref message)) if message.contains("connection reset")),
            "{result:?}"
        );
    }

    #[test]
    fn file_part_missing_file_is_io() {
        let result = file_part(&missing_file());

        assert!(
            matches!(result, Err(TrackerError::Io(_))),
            "{:?}",
            result.err()
        );
    }

    #[test]
    fn file_part_invalid_mime_is_http() {
        let file = AttachmentUploadFile {
            mime_type: Some("not a mime".to_string()),
            ..existing_file()
        };

        let result = file_part(&file);

        assert!(
            matches!(result, Err(TrackerError::Http(ref message)) if message.starts_with("invalid MIME type")),
            "{:?}",
            result.err()
        );
    }

    #[test]
    fn file_form_rejects_any_bad_file() {
        let result = file_form("file", &[existing_file(), missing_file()]);
        assert!(
            matches!(result, Err(TrackerError::Io(_))),
            "{:?}",
            result.err()
        );

        // Text parts borrowed for less than 'static still fit, as Confluence's
        // comment does.
        let comment = String::from("c");
        let form = file_form("file", &[existing_file()]).map(|form| form.text("comment", &comment));
        assert!(form.is_ok());
    }
}
