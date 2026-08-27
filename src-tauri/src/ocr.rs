use crate::clipboard::base64_decode;

pub fn extract_text_from_base64_image(base64_content: &str) -> Option<String> {
    let clean = base64_content.trim();
    let b64_str = if let Some(pos) = clean.find(";base64,") {
        &clean[pos + 8..]
    } else {
        clean
    };

    let image_bytes = base64_decode(b64_str).ok()?;
    if image_bytes.is_empty() {
        return None;
    }

    #[cfg(windows)]
    {
        if let Some(text) = windows_ocr(&image_bytes) {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        if let Some(text) = macos_ocr(&image_bytes) {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }

    None
}

#[cfg(windows)]
fn windows_ocr(image_bytes: &[u8]) -> Option<String> {
    use windows::core::HSTRING;
    use windows::Graphics::Imaging::BitmapDecoder;
    use windows::Media::Ocr::OcrEngine;
    use windows::Storage::Streams::{DataWriter, InMemoryRandomAccessStream};

    let stream = InMemoryRandomAccessStream::new().ok()?;
    let writer = DataWriter::CreateDataWriter(&stream).ok()?;
    let _ = writer.WriteBytes(image_bytes);
    let _ = writer.StoreAsync().ok()?.get();
    let _ = writer.DetachStream();
    let _ = stream.Seek(0);

    let decoder = BitmapDecoder::CreateAsync(&stream).ok()?.get().ok()?;
    let software_bitmap = decoder.GetSoftwareBitmapAsync().ok()?.get().ok()?;

    let engine = OcrEngine::TryCreateFromUserProfileLanguages().ok()?;
    let result = engine.RecognizeAsync(&software_bitmap).ok()?.get().ok()?;
    let text_hstring: HSTRING = result.Text().ok()?;
    let recognized_text = text_hstring.to_string();

    if recognized_text.trim().is_empty() {
        None
    } else {
        Some(recognized_text)
    }
}

#[cfg(target_os = "macos")]
fn macos_ocr(image_bytes: &[u8]) -> Option<String> {
    use cocoa::base::{id, nil};
    use cocoa::foundation::NSData;
    use objc::{class, msg_send, sel, sel_impl};

    unsafe {
        let data = NSData::dataWithBytes_length_(
            nil,
            image_bytes.as_ptr() as *const std::ffi::c_void,
            image_bytes.len() as u64,
        );
        if data == nil {
            return None;
        }

        let handler_cls = class!(VNImageRequestHandler);
        let req_cls = class!(VNRecognizeTextRequest);
        if handler_cls == nil || req_cls == nil {
            return None;
        }

        let request: id = msg_send![req_cls, alloc];
        let request: id = msg_send![request, init];
        if request == nil {
            return None;
        }

        // Fast + Accurate recognition mode
        let _: () = msg_send![request, setRecognitionLevel: 0]; // 0 = VNRequestTextRecognitionLevelAccurate

        let handler: id = msg_send![handler_cls, alloc];
        let handler: id = msg_send![handler, initWithData:data options:nil];
        if handler == nil {
            return None;
        }

        let mut error: id = nil;
        let requests_array: id = msg_send![class!(NSArray), arrayWithObject:request];
        let success: bool = msg_send![handler, performRequests:requests_array error:&mut error];

        if !success {
            return None;
        }

        let results: id = msg_send![request, results];
        if results == nil {
            return None;
        }

        let count: usize = msg_send![results, count];
        let mut full_text = String::new();

        for i in 0..count {
            let observation: id = msg_send![results, objectAtIndex:i];
            if observation != nil {
                let top_candidates: id = msg_send![observation, topCandidates:1];
                if top_candidates != nil {
                    let cand_count: usize = msg_send![top_candidates, count];
                    if cand_count > 0 {
                        let candidate: id = msg_send![top_candidates, objectAtIndex:0];
                        if candidate != nil {
                            let string_ns: id = msg_send![candidate, string];
                            if string_ns != nil {
                                let utf8_ptr: *const i8 = msg_send![string_ns, UTF8String];
                                if !utf8_ptr.is_null() {
                                    if let Ok(c_str) = std::ffi::CStr::from_ptr(utf8_ptr).to_str() {
                                        if !full_text.is_empty() {
                                            full_text.push('\n');
                                        }
                                        full_text.push_str(c_str);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        if full_text.trim().is_empty() {
            None
        } else {
            Some(full_text)
        }
    }
}
