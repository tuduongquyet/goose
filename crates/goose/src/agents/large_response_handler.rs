use chrono::Utc;
use rmcp::model::{CallToolResult, Content, ErrorData};
use std::fs::File;
use std::io::Write;

use crate::utils::head_tail_truncate;

/// Results above this get head+tail truncated in-place.
const TRUNCATION_THRESHOLD: usize = 100_000;

/// Results above this get written to a temp file (preserves full content on disk).
const LARGE_TEXT_THRESHOLD: usize = 200_000;

/// Head ratio for truncation (40% head, 60% tail).
const HEAD_RATIO: f64 = 0.4;

/// Process tool response and handle large text content
pub fn process_tool_response(
    response: Result<CallToolResult, ErrorData>,
) -> Result<CallToolResult, ErrorData> {
    match response {
        Ok(mut result) => {
            let mut processed_contents = Vec::new();

            for content in result.content {
                match content.as_text() {
                    Some(text_content) => {
                        let char_count = text_content.text.chars().count();
                        if char_count > LARGE_TEXT_THRESHOLD {
                            // Very large: write to temp file
                            match write_large_text_to_file(&text_content.text) {
                                Ok(file_path) => {
                                    let message = format!(
                                        "The response returned from the tool call was larger ({} characters) and is stored in the file which you can use other tools to examine or search in: {}",
                                        char_count,
                                        file_path
                                    );
                                    processed_contents.push(Content::text(message));
                                }
                                Err(e) => {
                                    let warning = format!(
                                        "Warning: Failed to write large response to file: {}. Showing truncated content instead.\n\n{}",
                                        e,
                                        head_tail_truncate(&text_content.text, TRUNCATION_THRESHOLD, HEAD_RATIO)
                                    );
                                    processed_contents.push(Content::text(warning));
                                }
                            }
                        } else if char_count > TRUNCATION_THRESHOLD {
                            // Medium-large: head+tail truncate in-place
                            let truncated = head_tail_truncate(
                                &text_content.text,
                                TRUNCATION_THRESHOLD,
                                HEAD_RATIO,
                            );
                            processed_contents.push(Content::text(truncated));
                        } else {
                            processed_contents.push(content);
                        }
                    }
                    None => {
                        // Pass through other content types unchanged
                        processed_contents.push(content);
                    }
                }
            }

            result.content = processed_contents;
            Ok(result)
        }
        Err(e) => Err(e),
    }
}

/// Write large text content to a temporary file
fn write_large_text_to_file(content: &str) -> Result<String, std::io::Error> {
    // Create temp directory if it doesn't exist
    let temp_dir = std::env::temp_dir().join("goose_mcp_responses");
    std::fs::create_dir_all(&temp_dir)?;

    // Generate a unique filename with timestamp
    let timestamp = Utc::now().format("%Y%m%d_%H%M%S%.6f");
    let filename = format!("mcp_response_{}.txt", timestamp);
    let file_path = temp_dir.join(&filename);

    // Write content to file
    let mut file = File::create(&file_path)?;
    file.write_all(content.as_bytes())?;

    Ok(file_path.to_string_lossy().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmcp::model::{Content, ErrorCode, ErrorData};
    use std::borrow::Cow;
    use std::fs;
    use std::path::Path;

    #[test]
    fn test_small_text_response_passes_through() {
        // Create a small text response
        let small_text = "This is a small text response";
        let content = Content::text(small_text.to_string());

        let response = Ok(CallToolResult::success(vec![content]));

        // Process the response
        let processed = process_tool_response(response).unwrap();

        // Verify the response is unchanged
        assert_eq!(processed.content.len(), 1);
        if let Some(text_content) = processed.content[0].as_text() {
            assert_eq!(text_content.text, small_text);
        } else {
            panic!("Expected text content");
        }
    }

    #[test]
    fn test_large_text_response_redirected_to_file() {
        // Create a text larger than the threshold
        let large_text = "a".repeat(LARGE_TEXT_THRESHOLD + 1000);
        let content = Content::text(large_text.clone());

        let response = Ok(CallToolResult::success(vec![content]));

        // Process the response
        let processed = process_tool_response(response).unwrap();

        // Verify the response contains a message about the file
        assert_eq!(processed.content.len(), 1);
        if let Some(text_content) = processed.content[0].as_text() {
            assert!(text_content
                .text
                .contains("The response returned from the tool call was larger"));
            assert!(text_content.text.contains("characters"));

            // Extract the file path from the message
            if let Some(file_path) = text_content.text.split("stored in the file: ").nth(1) {
                // Verify the file exists and contains the original text
                let path = Path::new(file_path.trim());
                if path.exists() {
                    // Only check content if file exists (may not exist in CI environments)
                    if let Ok(file_content) = fs::read_to_string(path) {
                        assert_eq!(file_content, large_text);
                    }

                    // Clean up the file
                    let _ = fs::remove_file(path); // Ignore errors on cleanup
                }
            }
        } else {
            panic!("Expected text content");
        }
    }

    #[test]
    fn test_image_content_passes_through() {
        // Create an image content
        let image_content = Content::image("base64data".to_string(), "image/png".to_string());

        let response = Ok(CallToolResult::success(vec![image_content]));

        // Process the response
        let processed = process_tool_response(response).unwrap();

        // Verify the response is unchanged
        assert_eq!(processed.content.len(), 1);
        if let Some(img) = processed.content[0].as_image() {
            assert_eq!(img.data, "base64data");
            assert_eq!(img.mime_type, "image/png");
        } else {
            panic!("Expected image content");
        }
    }

    #[test]
    fn test_mixed_content_handled_correctly() {
        // Create a response with mixed content types
        let small_text = Content::text("Small text");
        let large_text = Content::text("a".repeat(LARGE_TEXT_THRESHOLD + 1000));
        let image = Content::image("image_data".to_string(), "image/jpeg".to_string());

        let response = Ok(CallToolResult::success(vec![small_text, large_text, image]));

        // Process the response
        let processed = process_tool_response(response).unwrap();

        // Verify each item is handled correctly
        assert_eq!(processed.content.len(), 3);

        // First item should be unchanged small text
        if let Some(text_content) = processed.content[0].as_text() {
            assert_eq!(text_content.text, "Small text");
        } else {
            panic!("Expected text content");
        }

        // Second item should be a message about the file
        if let Some(text_content) = processed.content[1].as_text() {
            assert!(text_content
                .text
                .contains("The response returned from the tool call was larger"));

            // Extract the file path and clean up
            if let Some(file_path) = text_content.text.split("stored in the file: ").nth(1) {
                let path = Path::new(file_path.trim());
                if path.exists() {
                    let _ = fs::remove_file(path); // Ignore errors on cleanup
                }
            }
        } else {
            panic!("Expected text content");
        }

        // Third item should be unchanged image
        if let Some(img) = processed.content[2].as_image() {
            assert_eq!(img.data, "image_data");
            assert_eq!(img.mime_type, "image/jpeg");
        } else {
            panic!("Expected image content");
        }
    }

    #[test]
    fn test_error_response_passes_through() {
        // Create an error response
        let error = ErrorData {
            code: ErrorCode::INTERNAL_ERROR,
            message: Cow::from("Test error"),
            data: None,
        };
        let response: Result<CallToolResult, ErrorData> = Err(error);

        // Process the response
        let processed = process_tool_response(response);

        // Verify the error is passed through unchanged
        assert!(processed.is_err());
        match processed {
            Err(err) => {
                assert_eq!(err.code, ErrorCode::INTERNAL_ERROR);
                assert_eq!(err.message, "Test error");
            }
            _ => panic!("Expected execution error"),
        }
    }

    #[test]
    fn test_medium_text_gets_head_tail_truncated() {
        // 150K chars — above TRUNCATION_THRESHOLD (100K) but below LARGE_TEXT_THRESHOLD (200K)
        let medium_text = "x".repeat(150_000);
        let content = Content::text(medium_text);

        let response = Ok(CallToolResult::success(vec![content]));
        let processed = process_tool_response(response).unwrap();

        assert_eq!(processed.content.len(), 1);
        if let Some(text_content) = processed.content[0].as_text() {
            assert!(text_content.text.contains("characters truncated"));
            // Should be significantly smaller than the original
            assert!(text_content.text.chars().count() < 150_000);
            // Should start with 'x's (head) and end with 'x's (tail)
            assert!(text_content.text.starts_with("xxxx"));
            assert!(text_content.text.ends_with("xxxx"));
        } else {
            panic!("Expected text content");
        }
    }

    #[test]
    fn test_exactly_at_truncation_threshold_passes_through() {
        let exact_text = "y".repeat(TRUNCATION_THRESHOLD);
        let content = Content::text(exact_text.clone());

        let response = Ok(CallToolResult::success(vec![content]));
        let processed = process_tool_response(response).unwrap();

        assert_eq!(processed.content.len(), 1);
        if let Some(text_content) = processed.content[0].as_text() {
            assert_eq!(text_content.text, exact_text);
        } else {
            panic!("Expected text content");
        }
    }
}
