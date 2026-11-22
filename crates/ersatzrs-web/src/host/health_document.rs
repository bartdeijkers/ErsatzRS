use std::io;
use std::sync::{Arc, Mutex};

use axum::body::{Body, Bytes};
use axum::extract::Request;
use axum::http::{header, Method};
use axum::middleware::Next;
use axum::response::Response;
use futures_util::{stream, StreamExt};
use lol_html::{doc_comments, element, send::HtmlRewriter, send::Settings};

/// Leptos emits empty comment markers as `<!>` and appends resource scripts after
/// the document. Keep its comment nodes, script order and streaming behavior.
pub(super) async fn normalize(request: Request, next: Next) -> Response {
    let health_page = request.method() == Method::GET && request.uri().path() == "/system/health";
    let response = next.run(request).await;
    let is_html = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.split(';').next() == Some("text/html"));
    if !health_page || !response.status().is_success() || !is_html {
        return response;
    }
    let (mut parts, body) = response.into_parts();
    parts.headers.remove(header::CONTENT_LENGTH);
    Response::from_parts(parts, document_body(body))
}

fn document_body(body: Body) -> Body {
    let output = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&output);
    let settings = Settings::new_send()
        .append_document_content_handler(doc_comments!(|comment| {
            if comment.text().is_empty() {
                comment.set_text("")?;
            }
            Ok(())
        }))
        // The HTML5 end tags for body and html are optional. Omitting them puts
        // the trailing Leptos resource scripts inside the body.
        .append_element_content_handler(element!("body, html", |element: &mut lol_html::send::Element| {
            element.on_end_tag(Box::new(|tag| {
                tag.remove();
                Ok(())
            }))
        }));
    let rewriter = HtmlRewriter::new(settings, move |bytes: &[u8]| {
        sink.lock()
            .expect("exclusive HTML output")
            .extend_from_slice(bytes);
    });
    let chunks = stream::try_unfold(Some((body.into_data_stream(), rewriter)), move |state| {
        let output = Arc::clone(&output);
        async move {
            let Some((mut input, mut parser)) = state else {
                return Ok::<_, io::Error>(None);
            };
            let next = if let Some(chunk) = input.next().await {
                parser
                    .write(&chunk.map_err(io::Error::other)?)
                    .map_err(io::Error::other)?;
                Some((input, parser))
            } else {
                parser.end().map_err(io::Error::other)?;
                None
            };
            let bytes = std::mem::take(&mut *output.lock().expect("exclusive HTML output"));
            Ok(Some((Bytes::from(bytes), next)))
        }
    });
    Body::from_stream(chunks)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn streamed_markers_and_resources_preserve_script_and_attribute_text() {
        let input = [
            "<!DOCTYPE html><html><head><title>Health</title></head><body><!",
            "><p title='<!>'>Check</p></bo",
            "dy></html><script nonce='example'>const marker = '<!>';</script>",
        ];
        let source = Body::from_stream(stream::iter(input.map(Ok::<_, io::Error>)));
        let page = axum::body::to_bytes(document_body(source), 1024)
            .await
            .unwrap();
        assert_eq!(page.as_ref(), b"<!DOCTYPE html><html><head><title>Health</title></head><body><!----><p title='<!>'>Check</p><script nonce='example'>const marker = '<!>';</script>");
    }
}
