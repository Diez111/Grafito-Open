#![allow(clippy::unwrap_used, clippy::expect_used)]
use base64::Engine;
use grafito_assistant::{
    build_anthropic_messages_payload, build_chat_completion_payload, build_fusion_audit_payload,
    chat_completion_endpoint, messages_endpoint, request_remote_models_with_api_key_on_worker,
    request_remote_on_worker, request_remote_with_api_key_on_worker, validate_attachment,
    validate_endpoint, CancellationToken, ProviderSettings,
};
use grafito_assistant_types::{
    AssistantFocus, AssistantRepairFailure, AssistantRepairFailureKind, AssistantRepairFeedback,
    AssistantRequest, AttachmentLimits, ConversationTurn, ImageAttachment,
    ImmutableDocumentContext, PrivacyMode, ProviderCapabilities, ProviderProfile,
};
use image::{DynamicImage, ImageFormat, Rgb, RgbImage, Rgba, RgbaImage};
use std::io::{Cursor, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

fn png_bytes(width: u32, height: u32) -> Vec<u8> {
    let image = RgbaImage::from_pixel(width, height, Rgba([1, 2, 3, 255]));
    let mut bytes = Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(image)
        .write_to(&mut bytes, ImageFormat::Png)
        .unwrap();
    bytes.into_inner()
}

fn jpeg_bytes(width: u32, height: u32) -> Vec<u8> {
    let image = RgbImage::from_pixel(width, height, Rgb([1, 2, 3]));
    let mut bytes = Cursor::new(Vec::new());
    DynamicImage::ImageRgb8(image)
        .write_to(&mut bytes, ImageFormat::Jpeg)
        .unwrap();
    bytes.into_inner()
}

fn jpeg_bytes_with_exif_marker(marker: &[u8]) -> Vec<u8> {
    let original = jpeg_bytes(1, 1);
    assert_eq!(&original[..2], &[0xff, 0xd8]);
    let mut metadata = b"Exif\0\0".to_vec();
    metadata.extend_from_slice(marker);
    let segment_len = u16::try_from(metadata.len() + 2).unwrap();
    let mut encoded = Vec::with_capacity(original.len() + metadata.len() + 4);
    encoded.extend_from_slice(&original[..2]);
    encoded.extend_from_slice(&[0xff, 0xe1]);
    encoded.extend_from_slice(&segment_len.to_be_bytes());
    encoded.extend_from_slice(&metadata);
    encoded.extend_from_slice(&original[2..]);
    encoded
}

fn png_bytes_with_text_marker(marker: &[u8]) -> Vec<u8> {
    let mut original = png_bytes(1, 1);
    let mut data = b"Comment\0".to_vec();
    data.extend_from_slice(marker);
    let mut checksum_input = b"tEXt".to_vec();
    checksum_input.extend_from_slice(&data);
    let mut chunk = Vec::with_capacity(data.len() + 12);
    chunk.extend_from_slice(&(data.len() as u32).to_be_bytes());
    chunk.extend_from_slice(b"tEXt");
    chunk.extend_from_slice(&data);
    chunk.extend_from_slice(&png_crc32(&checksum_input).to_be_bytes());
    original.splice(33..33, chunk);
    original
}

fn png_crc32(bytes: &[u8]) -> u32 {
    let mut crc = u32::MAX;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xedb8_8320 & mask);
        }
    }
    !crc
}

fn remote_request(problem: &str) -> AssistantRequest {
    let mut request = AssistantRequest::local(problem, ImmutableDocumentContext::empty(3));
    request.privacy_mode = PrivacyMode::RemoteAllowed;
    request
}

fn read_http_request(stream: &mut TcpStream) -> Vec<u8> {
    let mut request = Vec::new();
    let mut expected_total = None;
    let mut buffer = [0; 4096];

    loop {
        if let Some(expected_total) = expected_total {
            if request.len() >= expected_total {
                return request;
            }
        }

        let bytes = stream.read(&mut buffer).unwrap();
        assert!(
            bytes > 0,
            "client closed before completing its HTTP request"
        );
        request.extend_from_slice(&buffer[..bytes]);

        if expected_total.is_none() {
            let Some(headers_end) = request.windows(4).position(|window| window == b"\r\n\r\n")
            else {
                continue;
            };
            let headers = std::str::from_utf8(&request[..headers_end]).unwrap();
            let content_length = headers
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().unwrap())
                })
                .unwrap_or(0);
            expected_total = Some(headers_end + 4 + content_length);
        }
    }
}

fn tetrahedron_repair_feedback() -> AssistantRepairFeedback {
    AssistantRepairFeedback {
        failures: vec![AssistantRepairFailure {
            command: "Polyhedron".into(),
            kind: AssistantRepairFailureKind::UnsupportedCommand,
            expected_syntax: Vec::new(),
        }],
    }
}

fn chat_completion_result(body: &str) -> Result<String, String> {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}/v1", listener.local_addr().unwrap());
    let body = body.to_owned();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let _ = read_http_request(&mut stream);
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        )
        .unwrap();
    });
    let result = request_remote_with_api_key_on_worker(
        ProviderSettings::for_profile(ProviderProfile::OllamaLocal, "local")
            .with_endpoint(endpoint)
            .unwrap(),
        remote_request("2 + 2"),
        Some("test-key".into()),
        CancellationToken::default(),
    )
    .join()
    .unwrap()
    .map(|completion| completion.text);
    server.join().unwrap();
    result
}

fn vision_settings() -> ProviderSettings {
    ProviderSettings::for_profile(ProviderProfile::OllamaLocal, "vision").with_capabilities(
        ProviderCapabilities {
            openai_compatible: true,
            vision: true,
            streaming: false,
        },
    )
}

#[test]
fn endpoint_validation_accepts_https_and_loopback_http_only() {
    grafito_assistant::clear_rate_limit_for_tests();
    assert!(validate_endpoint("https://api.deepseek.com/v1").is_ok());
    assert!(validate_endpoint("http://127.0.0.1:11434/v1").is_ok());
    assert!(validate_endpoint("http://example.com/v1").is_err());
    assert!(validate_endpoint("https://key@example.com/v1").is_err());
}

#[test]
fn openai_payload_contains_budgeted_messages_without_a_secret() {
    grafito_assistant::clear_rate_limit_for_tests();
    let mut request = remote_request("2 + 2");
    request.budget.max_output_chars = 32;
    let settings = ProviderSettings::for_profile(ProviderProfile::DeepSeek, "deepseek-chat");

    let payload = build_chat_completion_payload(&settings, &request).unwrap();
    let rendered = payload.to_string();
    assert!(rendered.contains("deepseek-chat"));
    assert!(rendered.contains("2 + 2"));
    assert!(payload["messages"][0]["content"]
        .as_str()
        .is_some_and(|system| system.contains("Tetrahedron[x, y, z, edge]")));
    assert!(!rendered.contains("api_key"));
    assert_eq!(payload["max_tokens"], 8);
}

#[test]
fn opencode_go_uses_the_official_base_and_chat_completion_path() {
    grafito_assistant::clear_rate_limit_for_tests();
    let settings = ProviderSettings::for_profile(ProviderProfile::OpenCodeGo, "deepseek-v4-flash");

    assert_eq!(settings.endpoint, "https://opencode.ai/zen/go/v1");
    assert_eq!(
        chat_completion_endpoint(&settings).unwrap().as_str(),
        "https://opencode.ai/zen/go/v1/chat/completions"
    );
}

#[test]
fn opencode_go_derives_the_official_anthropic_messages_path_for_minimax() {
    grafito_assistant::clear_rate_limit_for_tests();
    let settings = ProviderSettings::for_profile(ProviderProfile::OpenCodeGo, "mimo-2.5-vl");

    assert_eq!(
        messages_endpoint(&settings).unwrap().as_str(),
        "https://opencode.ai/zen/go/v1/messages"
    );
}

#[test]
fn minimax_payload_uses_anthropic_messages_without_a_secret() {
    grafito_assistant::clear_rate_limit_for_tests();
    let request = remote_request("2 + 2");
    let settings = ProviderSettings::for_profile(ProviderProfile::OpenCodeGo, "mimo-2.5-vl");

    let payload = build_anthropic_messages_payload(&settings, &request).unwrap();
    let rendered = payload.to_string();

    assert_eq!(payload["model"], "mimo-2.5-vl");
    assert_eq!(payload["messages"][0]["role"], "user");
    assert!(payload["system"]
        .as_str()
        .is_some_and(|text| !text.is_empty()));
    assert!(payload["system"].as_str().is_some_and(|text| {
        text.contains("```grafito")
            && text.contains("LaTex")
            && text.contains("Function[expr]")
            && text.contains("Tetrahedron[x, y, z, edge]")
    }));
    assert!(rendered.contains("2 + 2"));
    assert!(!rendered.contains("api_key"));
}

#[test]
fn repair_requests_reject_images_in_openai_and_anthropic_payloads() {
    grafito_assistant::clear_rate_limit_for_tests();
    let mut request = remote_request("construí un tetraedro");
    request.repair_feedback = Some(tetrahedron_repair_feedback());
    request.image_upload_consent = true;
    request
        .attachments
        .push(ImageAttachment::new("image/png", png_bytes(1, 1), 1, 1));

    let openai_error = build_chat_completion_payload(&vision_settings(), &request)
        .expect_err("a repair request must never serialize OpenAI image blocks");
    assert!(openai_error.contains("repair requests cannot include images"));

    let anthropic_settings =
        ProviderSettings::for_profile(ProviderProfile::OpenCodeGo, "mimo-2.5-vl")
            .with_capabilities(ProviderCapabilities {
                vision: true,
                ..ProviderProfile::OpenCodeGo.capabilities()
            });
    let anthropic_error = build_anthropic_messages_payload(&anthropic_settings, &request)
        .expect_err("a repair request must never serialize Anthropic image blocks");
    assert!(anthropic_error.contains("repair requests cannot include images"));
}

#[test]
fn minimax_payload_transmits_validated_images_as_anthropic_base64_blocks() {
    grafito_assistant::clear_rate_limit_for_tests();
    let mut request = remote_request("interpretá ambas imágenes");
    request.image_upload_consent = true;
    request
        .attachments
        .push(ImageAttachment::new("image/png", png_bytes(1, 1), 1, 1));
    request
        .attachments
        .push(ImageAttachment::new("image/jpeg", jpeg_bytes(1, 1), 1, 1));
    let settings = ProviderSettings::for_profile(ProviderProfile::OpenCodeGo, "mimo-2.5-vl")
        .with_capabilities(ProviderCapabilities {
            vision: true,
            ..ProviderProfile::OpenCodeGo.capabilities()
        });

    let payload = build_anthropic_messages_payload(&settings, &request).unwrap();
    let content = payload["messages"][0]["content"]
        .as_array()
        .expect("Anthropic multimodal content");

    assert_eq!(content[0]["type"], "text");
    assert_eq!(content[0]["text"], "interpretá ambas imágenes");
    assert_eq!(content[1]["type"], "image");
    assert_eq!(content[1]["source"]["type"], "base64");
    assert_eq!(content[1]["source"]["media_type"], "image/png");
    assert!(content[1]["source"]["data"]
        .as_str()
        .is_some_and(|data| !data.starts_with("data:") && !data.is_empty()));
    assert_eq!(content[2]["source"]["media_type"], "image/jpeg");
}

#[test]
fn image_payload_reencodes_pixels_without_png_text_or_jpeg_exif_metadata() {
    grafito_assistant::clear_rate_limit_for_tests();
    const PNG_MARKER: &[u8] = b"PRIVATE_PNG_TEXT_GPS";
    const JPEG_MARKER: &[u8] = b"PRIVATE_JPEG_EXIF_GPS";
    let png = png_bytes_with_text_marker(PNG_MARKER);
    let jpeg = jpeg_bytes_with_exif_marker(JPEG_MARKER);
    let mut request = remote_request("interpretá ambas imágenes");
    request.image_upload_consent = true;
    request
        .attachments
        .push(ImageAttachment::new("image/png", png.clone(), 1, 1));
    request
        .attachments
        .push(ImageAttachment::new("image/jpeg", jpeg.clone(), 1, 1));
    let settings = ProviderSettings::for_profile(ProviderProfile::OpenCodeGo, "mimo-2.5-vl")
        .with_capabilities(ProviderCapabilities {
            vision: true,
            ..ProviderProfile::OpenCodeGo.capabilities()
        });

    let payload = build_anthropic_messages_payload(&settings, &request).unwrap();
    let content = payload["messages"][0]["content"].as_array().unwrap();
    let sanitized_png = base64::engine::general_purpose::STANDARD
        .decode(content[1]["source"]["data"].as_str().unwrap())
        .unwrap();
    let sanitized_jpeg = base64::engine::general_purpose::STANDARD
        .decode(content[2]["source"]["data"].as_str().unwrap())
        .unwrap();
    let limits = AttachmentLimits::default();

    assert_ne!(sanitized_png, png);
    assert_ne!(sanitized_jpeg, jpeg);
    assert!(sanitized_png.len() <= limits.max_bytes);
    assert!(sanitized_jpeg.len() <= limits.max_bytes);
    assert!(sanitized_png.len() + sanitized_jpeg.len() <= limits.max_total_bytes);
    assert!(!sanitized_png
        .windows(PNG_MARKER.len())
        .any(|window| window == PNG_MARKER));
    assert!(!sanitized_jpeg
        .windows(JPEG_MARKER.len())
        .any(|window| window == JPEG_MARKER));
    assert_eq!(
        image::guess_format(&sanitized_png).unwrap(),
        ImageFormat::Png
    );
    assert_eq!(
        image::guess_format(&sanitized_jpeg).unwrap(),
        ImageFormat::Jpeg
    );
    let sanitized_png_pixels = image::load_from_memory(&sanitized_png).unwrap();
    let sanitized_jpeg_pixels = image::load_from_memory(&sanitized_jpeg).unwrap();
    assert_eq!(
        (sanitized_png_pixels.width(), sanitized_png_pixels.height()),
        (1, 1)
    );
    assert_eq!(
        (
            sanitized_jpeg_pixels.width(),
            sanitized_jpeg_pixels.height()
        ),
        (1, 1)
    );
}

#[test]
fn remote_payload_keeps_document_binding_metadata_local() {
    grafito_assistant::clear_rate_limit_for_tests();
    let request = remote_request("2 + 2");
    let document_digest = request.context.digest.clone();
    let chat = build_chat_completion_payload(
        &ProviderSettings::for_profile(ProviderProfile::OpenCodeGo, "deepseek-v4-pro"),
        &request,
    )
    .unwrap();
    let fusion = build_fusion_audit_payload(
        &ProviderSettings::for_profile(ProviderProfile::OpenCodeGo, "fusion"),
        &request,
        "4",
    )
    .unwrap();

    assert!(chat.get("metadata").is_none());
    assert!(fusion.get("metadata").is_none());
    assert!(!chat.to_string().contains(&document_digest));
    assert!(!fusion.to_string().contains(&document_digest));
}

#[test]
fn minimax_image_payload_requires_capability_and_consent_and_fusion_stays_text_only() {
    grafito_assistant::clear_rate_limit_for_tests();
    let mut request = remote_request("read this image");
    request
        .attachments
        .push(ImageAttachment::new("image/png", png_bytes(1, 1), 1, 1));
    let minimax = ProviderSettings::for_profile(ProviderProfile::OpenCodeGo, "mimo-2.5-vl");

    assert!(build_anthropic_messages_payload(&minimax, &request).is_err());
    request.image_upload_consent = true;
    assert!(build_anthropic_messages_payload(&minimax, &request).is_err());

    let vision_minimax = minimax.with_capabilities(ProviderCapabilities {
        vision: true,
        ..ProviderProfile::OpenCodeGo.capabilities()
    });
    assert!(build_anthropic_messages_payload(&vision_minimax, &request).is_ok());

    let fusion = ProviderSettings::for_profile(ProviderProfile::OpenCodeGo, "fusion")
        .with_capabilities(ProviderCapabilities {
            vision: true,
            ..ProviderProfile::OpenCodeGo.capabilities()
        });
    assert!(build_anthropic_messages_payload(&fusion, &request).is_err());
}

#[test]
fn fusion_audit_uses_deepseek_pro_and_includes_only_reviewed_text() {
    grafito_assistant::clear_rate_limit_for_tests();
    let request = remote_request("Explicá 2 + 2");
    let settings = ProviderSettings::for_profile(ProviderProfile::OpenCodeGo, "fusion");

    let payload = build_fusion_audit_payload(&settings, &request, "El resultado es 4.").unwrap();
    let rendered = payload.to_string();

    assert_eq!(payload["model"], "deepseek-v4-pro");
    assert!(rendered.contains("Explicá 2 + 2"));
    assert!(rendered.contains("El resultado es 4."));
    assert!(rendered.contains("Audit"));
    assert!(!rendered.contains("api_key"));
}

#[test]
fn fusion_audit_requires_explicit_remote_privacy_consent() {
    grafito_assistant::clear_rate_limit_for_tests();
    let request = AssistantRequest::local("Explicá 2 + 2", ImmutableDocumentContext::empty(0));
    let settings = ProviderSettings::for_profile(ProviderProfile::OpenCodeGo, "fusion");

    assert!(build_fusion_audit_payload(&settings, &request, "El resultado es 4.").is_err());
}

#[test]
fn remote_payload_includes_only_bounded_focus_and_conversation_text() {
    grafito_assistant::clear_rate_limit_for_tests();
    let mut request = remote_request("analyze the selected function");
    request.focus = Some(AssistantFocus::function(
        "f",
        "sin(x)",
        Some(-3.0),
        Some(3.0),
        false,
    ));
    request.conversation = vec![
        ConversationTurn::user("What are its roots?"),
        ConversationTurn::assistant("I can inspect f(x) = sin(x)."),
    ];

    let payload = build_chat_completion_payload(
        &ProviderSettings::for_profile(ProviderProfile::OpenCodeGo, "deepseek-v4-flash"),
        &request,
    )
    .unwrap();
    let rendered = payload.to_string();

    assert!(rendered.contains("Focused object"));
    assert!(rendered.contains("f(x) = sin(x)"));
    assert!(rendered.contains("What are its roots?"));
    assert!(rendered.contains("I can inspect f(x) = sin(x)."));
}

#[test]
fn worker_appends_chat_completions_and_accepts_an_explicit_key() {
    grafito_assistant::clear_rate_limit_for_tests();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}/v1", listener.local_addr().unwrap());
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let request = read_http_request(&mut stream);
        let request = String::from_utf8_lossy(&request);
        assert!(request.starts_with("POST /v1/chat/completions HTTP/1.1"));
        assert!(request.contains("authorization: Bearer test-key"));
        stream
            .write_all(
                b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 84\r\nConnection: close\r\n\r\n{\"choices\":[{\"finish_reason\":\"stop\",\"message\":{\"role\":\"assistant\",\"content\":\"ok\"}}]}"
            )
            .unwrap();
    });

    let result = request_remote_with_api_key_on_worker(
        ProviderSettings::for_profile(ProviderProfile::OllamaLocal, "vision")
            .with_endpoint(endpoint)
            .unwrap(),
        remote_request("2 + 2"),
        Some("test-key".into()),
        CancellationToken::default(),
    )
    .join()
    .unwrap()
    .unwrap();
    server.join().unwrap();

    assert_eq!(result.text, "ok");
}

#[test]
fn chat_completions_accepts_a_final_assistant_content_array_of_text_blocks() {
    grafito_assistant::clear_rate_limit_for_tests();
    let text = chat_completion_result(
        r#"{"choices":[{"finish_reason":"stop","message":{"role":"assistant","content":[{"type":"text","text":"final "},{"type":"text","text":"answer"}]}}]}"#,
    )
    .unwrap();

    assert_eq!(text, "final answer");
}

#[test]
fn chat_completions_rejects_non_final_or_non_displayable_content_without_echoing_provider_data() {
    grafito_assistant::clear_rate_limit_for_tests();
    let incomplete = chat_completion_result(
        r#"{"choices":[{"finish_reason":"length","message":{"role":"assistant","content":"provider-private-partial"}}]}"#,
    )
    .unwrap_err();
    assert_eq!(
        incomplete,
        "remote assistant response schema is invalid: first choice is not a completed text response"
    );
    assert!(!incomplete.contains("provider-private-partial"));

    let tool_call = chat_completion_result(
        r#"{"choices":[{"finish_reason":"stop","message":{"role":"assistant","content":"provider-private-tool","tool_calls":[]}}]}"#,
    )
    .unwrap_err();
    assert_eq!(
        tool_call,
        "remote assistant response content is not displayable: tool or function calls are not displayable final content"
    );
    assert!(!tool_call.contains("provider-private-tool"));
}

#[test]
fn chat_completions_classifies_invalid_json_and_schema_without_leaking_credentials_or_bodies() {
    grafito_assistant::clear_rate_limit_for_tests();
    let invalid_json = chat_completion_result("{\"choices\":[").unwrap_err();
    assert_eq!(invalid_json, "remote assistant response JSON is invalid");
    assert!(!invalid_json.contains("test-key"));

    let invalid_schema =
        chat_completion_result(r#"{"choices":"provider-private-body"}"#).unwrap_err();
    assert_eq!(
        invalid_schema,
        "remote assistant response schema is invalid: choices must be an array"
    );
    assert!(!invalid_schema.contains("provider-private-body"));
    assert!(!invalid_schema.contains("test-key"));
}

#[test]
fn model_worker_uses_the_models_path_and_reduces_metadata_to_identifiers() {
    grafito_assistant::clear_models_cache_for_tests();
    grafito_assistant::clear_rate_limit_for_tests();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}/v1", listener.local_addr().unwrap());
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let request = read_http_request(&mut stream);
        let request = String::from_utf8_lossy(&request);
        assert!(request.starts_with("GET /v1/models HTTP/1.1"));
        let body = r#"{"data":[{"id":"deepseek-v4-flash","ignored":"secret"},{"id":"glm-5.2"}]}"#;
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        )
        .unwrap();
    });

    let models = request_remote_models_with_api_key_on_worker(
        ProviderSettings::for_profile(ProviderProfile::OllamaLocal, "llama3.2")
            .with_endpoint(endpoint)
            .unwrap(),
        Some("test-key".into()),
        CancellationToken::default(),
    )
    .join()
    .unwrap()
    .unwrap();
    server.join().unwrap();

    assert_eq!(models, vec!["deepseek-v4-flash", "glm-5.2"]);
}

#[test]
fn payload_rejects_images_when_the_selected_model_has_no_vision_capability() {
    grafito_assistant::clear_rate_limit_for_tests();
    let mut request = remote_request("read the image");
    request
        .attachments
        .push(grafito_assistant_types::ImageAttachment::new(
            "image/png",
            vec![1],
            1,
            1,
        ));
    let settings = ProviderSettings::for_profile(ProviderProfile::DeepSeek, "deepseek-chat");

    assert!(build_chat_completion_payload(&settings, &request).is_err());
}

#[test]
fn named_provider_profiles_reject_attacker_hosts_and_nonstandard_ports() {
    grafito_assistant::clear_rate_limit_for_tests();
    let attacker = ProviderSettings::for_profile(ProviderProfile::DeepSeek, "deepseek-chat")
        .with_endpoint("https://api.deepseek.com.attacker.invalid/v1");
    let alternate_port = ProviderSettings::for_profile(ProviderProfile::DeepSeek, "deepseek-chat")
        .with_endpoint("https://api.deepseek.com:444/v1");

    assert!(attacker.is_err());
    assert!(alternate_port.is_err());

    let custom = ProviderSettings::custom_openai_compatible(
        "https://compatible.example/v1",
        "custom-model",
        "GRAFITO_ASSISTANT_CUSTOM_COMPATIBLE_API_KEY",
    )
    .unwrap();
    assert_eq!(custom.profile, ProviderProfile::CustomOpenAiCompatible);
    assert_eq!(
        custom.api_key_env.as_deref(),
        Some("GRAFITO_ASSISTANT_CUSTOM_COMPATIBLE_API_KEY")
    );
    assert!(ProviderSettings::custom_openai_compatible(
        "https://key@compatible.example/v1",
        "custom-model",
        "GRAFITO_ASSISTANT_CUSTOM_COMPATIBLE_API_KEY",
    )
    .is_err());
}

#[test]
fn custom_endpoints_accept_only_scoped_assistant_api_key_references() {
    grafito_assistant::clear_rate_limit_for_tests();
    for reference in [
        "AWS_SECRET_ACCESS_KEY",
        "GH_TOKEN",
        "PATH",
        "OPENCODEGO_API_KEY",
        "DEEPSEEK_API_KEY",
    ] {
        assert!(
            ProviderSettings::custom_openai_compatible(
                "https://compatible.example/v1",
                "custom-model",
                reference,
            )
            .is_err(),
            "custom endpoint accepted unsafe environment reference {reference}",
        );
    }

    assert!(ProviderSettings::custom_openai_compatible(
        "https://compatible.example/v1",
        "custom-model",
        "GRAFITO_ASSISTANT_CUSTOM_COMPATIBLE_API_KEY",
    )
    .is_ok());
}

#[test]
fn payload_validation_rejects_unscoped_custom_credentials_before_a_request() {
    grafito_assistant::clear_rate_limit_for_tests();
    let request = remote_request("2 + 2");
    for api_key_env in [
        "AWS_SECRET_ACCESS_KEY",
        "GH_TOKEN",
        "PATH",
        "OPENCODEGO_API_KEY",
        "DEEPSEEK_API_KEY",
    ] {
        let settings = ProviderSettings {
            profile: ProviderProfile::CustomOpenAiCompatible,
            endpoint: "https://compatible.example/v1".into(),
            model: "custom-model".into(),
            api_key_env: Some(api_key_env.into()),
            capabilities: ProviderProfile::CustomOpenAiCompatible.capabilities(),
        };

        assert!(build_chat_completion_payload(&settings, &request).is_err());
    }
}

#[test]
fn remote_payload_does_not_transmit_legacy_transcription_and_requires_image_consent() {
    grafito_assistant::clear_rate_limit_for_tests();
    let mut transcription = remote_request("solve the problem");
    transcription.transcription.text = "x + 1 = 2".into();
    let payload = build_chat_completion_payload(
        &ProviderSettings::for_profile(ProviderProfile::DeepSeek, "deepseek-chat"),
        &transcription,
    )
    .unwrap()
    .to_string();
    assert!(!payload.contains("x + 1 = 2"));

    let mut image = remote_request("read this image");
    image
        .attachments
        .push(grafito_assistant_types::ImageAttachment::new(
            "image/png",
            png_bytes(1, 1),
            1,
            1,
        ));
    assert!(build_chat_completion_payload(&vision_settings(), &image).is_err());
}

#[test]
fn payload_rejects_malformed_images_and_mismatched_real_dimensions() {
    grafito_assistant::clear_rate_limit_for_tests();
    let mut malformed = remote_request("read this image");
    malformed
        .attachments
        .push(grafito_assistant_types::ImageAttachment::new(
            "image/png",
            vec![1, 2, 3],
            1,
            1,
        ));
    malformed.image_upload_consent = true;
    assert!(build_chat_completion_payload(&vision_settings(), &malformed).is_err());

    let mut mismatched_dimensions = remote_request("read this image");
    mismatched_dimensions
        .attachments
        .push(grafito_assistant_types::ImageAttachment::new(
            "image/png",
            png_bytes(2, 2),
            1,
            1,
        ));
    mismatched_dimensions.image_upload_consent = true;
    assert!(build_chat_completion_payload(&vision_settings(), &mismatched_dimensions).is_err());

    let wrong_mime = ImageAttachment::new("image/png", jpeg_bytes(1, 1), 1, 1);
    assert!(validate_attachment(&wrong_mime, &AttachmentLimits::default()).is_err());

    let pixel_limited = ImageAttachment::new("image/png", png_bytes(2, 2), 2, 2);
    let limits = AttachmentLimits {
        max_pixels: 1,
        ..AttachmentLimits::default()
    };
    assert!(validate_attachment(&pixel_limited, &limits).is_err());
}

#[test]
fn image_payload_strips_unknown_source_path_metadata() {
    grafito_assistant::clear_rate_limit_for_tests();
    let mut attachment =
        serde_json::to_value(ImageAttachment::new("image/png", png_bytes(1, 1), 1, 1)).unwrap();
    attachment.as_object_mut().unwrap().insert(
        "source_path".into(),
        serde_json::Value::String("/private/photo.png".into()),
    );
    let attachment = serde_json::from_value(attachment).unwrap();

    let mut request = remote_request("read this image");
    request.image_upload_consent = true;
    request.attachments.push(attachment);
    let payload = build_chat_completion_payload(&vision_settings(), &request)
        .unwrap()
        .to_string();

    assert!(!payload.contains("source_path"));
    assert!(!payload.contains("/private/photo.png"));
}

#[test]
fn remote_worker_rejects_an_oversized_completion_before_deserializing_it() {
    grafito_assistant::clear_rate_limit_for_tests();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}/v1", listener.local_addr().unwrap());
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let _ = read_http_request(&mut stream);
        let body = format!(
            r#"{{"choices":[{{"message":{{"content":"{}"}}}}]}}"#,
            "x".repeat(20_000)
        );
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        )
        .unwrap();
    });

    let mut request = remote_request("2 + 2");
    request.budget.max_output_chars = 32;
    let result = request_remote_on_worker(
        ProviderSettings::for_profile(ProviderProfile::OllamaLocal, "local")
            .with_endpoint(endpoint)
            .unwrap(),
        request,
        CancellationToken::default(),
    )
    .join()
    .unwrap();
    server.join().unwrap();

    assert!(result.is_err());
}

#[test]
fn remote_worker_rejects_malformed_provider_json() {
    grafito_assistant::clear_rate_limit_for_tests();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}/v1", listener.local_addr().unwrap());
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let _ = read_http_request(&mut stream);
        stream
            .write_all(
                b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 12\r\nConnection: close\r\n\r\n{\"choices\":[",
            )
            .unwrap();
    });

    let result = request_remote_on_worker(
        ProviderSettings::for_profile(ProviderProfile::OllamaLocal, "local")
            .with_endpoint(endpoint)
            .unwrap(),
        remote_request("2 + 2"),
        CancellationToken::default(),
    )
    .join()
    .unwrap();
    server.join().unwrap();

    assert_eq!(
        result.unwrap_err(),
        "remote assistant response JSON is invalid"
    );
}

#[test]
fn remote_worker_does_not_follow_redirects() {
    grafito_assistant::clear_rate_limit_for_tests();
    let target = TcpListener::bind("127.0.0.1:0").unwrap();
    let target_address = target.local_addr().unwrap();
    let target_hits = Arc::new(AtomicUsize::new(0));
    let target_hits_for_server = Arc::clone(&target_hits);
    let target_server = thread::spawn(move || {
        target.set_nonblocking(true).unwrap();
        let deadline = Instant::now() + Duration::from_millis(250);
        while Instant::now() < deadline {
            match target.accept() {
                Ok((mut stream, _)) => {
                    target_hits_for_server.fetch_add(1, Ordering::SeqCst);
                    let _ = stream.write_all(
                        b"HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\n\r\n",
                    );
                    return;
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(5));
                }
                Err(error) => panic!("redirect target failed: {error}"),
            }
        }
    });

    let redirect = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}/v1", redirect.local_addr().unwrap());
    let redirect_server = thread::spawn(move || {
        let (mut stream, _) = redirect.accept().unwrap();
        let _ = read_http_request(&mut stream);
        write!(
            stream,
            "HTTP/1.1 302 Found\r\nLocation: http://{target_address}/redirected\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        )
        .unwrap();
    });

    let result = request_remote_on_worker(
        ProviderSettings::for_profile(ProviderProfile::OllamaLocal, "local")
            .with_endpoint(endpoint)
            .unwrap(),
        remote_request("2 + 2"),
        CancellationToken::default(),
    )
    .join()
    .unwrap();
    redirect_server.join().unwrap();
    target_server.join().unwrap();

    assert!(matches!(result, Err(message) if message.contains("HTTP 302")));
    assert_eq!(target_hits.load(Ordering::SeqCst), 0);
}

#[test]
fn chat_completions_429_includes_retry_after_seconds_without_sleeping() {
    grafito_assistant::clear_rate_limit_for_tests();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}/v1", listener.local_addr().unwrap());
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let _ = read_http_request(&mut stream);
        let body = "busy";
        write!(
            stream,
            "HTTP/1.1 429 Too Many Requests\r\nContent-Type: text/plain\r\nRetry-After: 7\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        )
        .unwrap();
    });

    let started = Instant::now();
    let result = request_remote_with_api_key_on_worker(
        ProviderSettings::for_profile(ProviderProfile::OllamaLocal, "local")
            .with_endpoint(endpoint)
            .unwrap(),
        remote_request("2 + 2"),
        Some("test-key".into()),
        CancellationToken::default(),
    )
    .join()
    .unwrap();
    server.join().unwrap();

    let error = result.unwrap_err();
    grafito_assistant::clear_rate_limit_for_tests();
    assert!(error.contains("429"), "{error}");
    assert!(error.contains("reintentá en 7s"), "{error}");
    assert!(!error.contains("test-key"), "{error}");
    // Sin dormir el worker: un 429 con Retry-After 7s debe fallar rápido.
    assert!(
        started.elapsed() < Duration::from_secs(7),
        "worker slept on Retry-After"
    );
}

#[test]
fn chat_completions_429_parses_http_date_retry_after_with_clamp() {
    grafito_assistant::clear_rate_limit_for_tests();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}/v1", listener.local_addr().unwrap());
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let _ = read_http_request(&mut stream);
        let body = "slow down";
        // Fecha muy futura → clamp a 120s (sin dormir el worker).
        write!(
            stream,
            "HTTP/1.1 429 Too Many Requests\r\nContent-Type: text/plain\r\nRetry-After: Wed, 21 Oct 2030 07:28:00 GMT\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        )
        .unwrap();
    });

    let result = request_remote_with_api_key_on_worker(
        ProviderSettings::for_profile(ProviderProfile::OllamaLocal, "local")
            .with_endpoint(endpoint)
            .unwrap(),
        remote_request("2 + 2"),
        Some("test-key".into()),
        CancellationToken::default(),
    )
    .join()
    .unwrap();
    server.join().unwrap();

    let error = result.unwrap_err();
    grafito_assistant::clear_rate_limit_for_tests();
    assert!(error.contains("429"), "{error}");
    assert!(error.contains("reintentá en 120s"), "{error}");
    assert!(!error.contains("test-key"), "{error}");
}

#[test]
fn chat_completions_500_truncates_long_body_without_secrets() {
    grafito_assistant::clear_rate_limit_for_tests();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}/v1", listener.local_addr().unwrap());
    // Cuerpo largo del proveedor (2000 chars): debe truncarse a 500 sin eco de clave.
    let long_body = "E".repeat(2_000);
    let server_body = long_body.clone();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let _ = read_http_request(&mut stream);
        write!(
            stream,
            "HTTP/1.1 500 Internal Server Error\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            server_body.len(),
            server_body
        )
        .unwrap();
    });

    let result = request_remote_with_api_key_on_worker(
        ProviderSettings::for_profile(ProviderProfile::OllamaLocal, "local")
            .with_endpoint(endpoint)
            .unwrap(),
        remote_request("2 + 2"),
        Some("test-key".into()),
        CancellationToken::default(),
    )
    .join()
    .unwrap();
    server.join().unwrap();

    let error = result.unwrap_err();
    assert!(
        error.starts_with("remote assistant returned HTTP 500: "),
        "{error}"
    );
    let snippet = error
        .strip_prefix("remote assistant returned HTTP 500: ")
        .unwrap();
    assert_eq!(snippet.chars().count(), 500, "{error}");
    assert!(long_body.starts_with(snippet), "{error}");
    assert!(!error.contains("test-key"), "{error}");
    // El cuerpo completo (2000) nunca viaja entero al mensaje.
    assert!(error.len() < long_body.len(), "{error}");
}
