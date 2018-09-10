use actix_web::{AsyncResponder, Error, HttpRequest, HttpResponse, HttpMessage, error::ErrorBadRequest,
                error::ErrorForbidden, error::ErrorInternalServerError, error::ErrorNotFound,
                http::header, Responder};
use rusoto_s3::*;
use futures::{future, stream, Future, Stream};
use bytes::Bytes;
use env::*;
use aws_s3_webdav::stream_utils;
use std::sync::Arc;

/// Alias for application environment, shared between handlers
type AppEnv = Arc<AppState>;

fn extract_bucket(req: &HttpRequest<AppEnv>) -> String {
    req.state().config.s3.bucket.as_str().to_owned()
}

fn extract_object_key(req: &HttpRequest<AppEnv>) -> String {
    match req.state().config.s3.prefix {
        Some(ref prefix) => format!("{}{}", prefix, req.path().trim_left_matches("/")),
        None => req.path().trim_left_matches("/").to_owned(),
    }
}

fn header_string(h: &header::HeaderValue) -> Option<String> {
    h.to_str().map(|h| h.to_string()).ok()
}

pub fn index(_req: &HttpRequest<AppEnv>) -> impl Responder {
    HttpResponse::NotImplemented()
}

/// Get object from bucket
pub fn get_object(req: &HttpRequest<AppEnv>) -> impl Responder {
    req.state()
        .s3
        .get_object(&GetObjectRequest {
            bucket: extract_bucket(&req),
            key: extract_object_key(&req),
            ..GetObjectRequest::default()
        })
        .map_err(|e| match e {
            // http://rusoto.github.io/rusoto/rusoto_s3/enum.GetObjectError.html
            GetObjectError::NoSuchKey(e) => ErrorNotFound(e),
            GetObjectError::HttpDispatch(e) => ErrorInternalServerError(e),
            GetObjectError::Credentials(e) => ErrorForbidden(e),
            GetObjectError::Validation(e) => ErrorBadRequest(e),
            GetObjectError::Unknown(e) => ErrorInternalServerError(e),
        })
        .map(|r| match r.body {
            Some(body) => {
                let mut response = HttpResponse::Ok();

                if let Some(cache_control) = r.cache_control {
                    response.header(header::CACHE_CONTROL, cache_control.as_str());
                }

                if let Some(content_disposition) = r.content_disposition {
                    response.header(header::CONTENT_DISPOSITION, content_disposition.as_str());
                }

                if let Some(content_encoding) = r.content_encoding {
                    response.header(header::CONTENT_ENCODING, content_encoding.as_str());
                }

                if let Some(content_language) = r.content_language {
                    response.header(header::CONTENT_LANGUAGE, content_language.as_str());
                }

                if let Some(content_type) = r.content_type {
                    response.header(header::CONTENT_TYPE, content_type.as_str());
                }

                if let Some(e_tag) = r.e_tag {
                    response.header(header::ETAG, e_tag.as_str());
                }

                if let Some(expires) = r.expires {
                    response.header(header::EXPIRES, expires.as_str());
                }

                if let Some(last_modified) = r.last_modified {
                    response.header(header::LAST_MODIFIED, last_modified.as_str());
                }

                response.streaming(Box::new(body.map_err(|_e| {
                    ErrorInternalServerError("Something went wrong with body stream")
                }).map(Bytes::from)))
            }
            None => HttpResponse::from_error(ErrorNotFound("Object Not Found")),
        })
        .responder()
}

/// HEAD object from bucket
pub fn head_object(req: &HttpRequest<AppEnv>) -> Box<Future<Item=HttpResponse, Error=Error>> {
    req.state()
        .s3
        .head_object(&HeadObjectRequest {
            bucket: extract_bucket(&req),
            key: extract_object_key(&req),
            ..HeadObjectRequest::default()
        })
        .map_err(|e| match e {
            // http://rusoto.github.io/rusoto/rusoto_s3/enum.HeadObjectError.html
            HeadObjectError::NoSuchKey(e) => ErrorNotFound(e),
            HeadObjectError::HttpDispatch(e) => ErrorInternalServerError(e),
            HeadObjectError::Credentials(e) => ErrorForbidden(e),
            HeadObjectError::Validation(e) => ErrorBadRequest(e),
            HeadObjectError::Unknown(e) => ErrorInternalServerError(e),
        })
        .map(|r| {
            let mut response = HttpResponse::Ok();

            // TODO add Accept-Ranges support
            // if let Some(accept_ranges) = r.accept_ranges {
            //   response.header(header::ACCEPT_RANGES, accept_ranges.as_str());
            // }

            if let Some(cache_control) = r.cache_control {
                response.header(header::CACHE_CONTROL, cache_control.as_str());
            }

            if let Some(content_disposition) = r.content_disposition {
                response.header(header::CONTENT_DISPOSITION, content_disposition.as_str());
            }

            if let Some(content_encoding) = r.content_encoding {
                response.header(header::CONTENT_ENCODING, content_encoding.as_str());
            }

            if let Some(content_language) = r.content_language {
                response.header(header::CONTENT_LANGUAGE, content_language.as_str());
            }

            if let Some(content_length) = r.content_length {
                response.header(header::CONTENT_LENGTH, content_length.to_string().as_str());
            }

            if let Some(content_type) = r.content_type {
                response.header(header::CONTENT_TYPE, content_type.as_str());
            }

            if let Some(e_tag) = r.e_tag {
                response.header(header::ETAG, e_tag.as_str());
            }

            if let Some(expires) = r.expires {
                response.header(header::EXPIRES, expires.as_str());
            }

            if let Some(last_modified) = r.last_modified {
                response.header(header::LAST_MODIFIED, last_modified.as_str());
            }

            response.finish()
        })
        .responder()
}

fn upload_parts(
    body_stream: Box<Stream<Item=Bytes, Error=Error>>,
    state: AppEnv,
    upload: &CreateMultipartUploadOutput,
) -> Box<Future<Item=Vec<CompletedPart>, Error=UploadPartError>> {
    let bucket: String = upload.bucket.to_owned().unwrap();
    let key: String = upload.key.to_owned().unwrap();
    let upload_id: String = upload.upload_id.to_owned().unwrap();

    Box::new(
        stream_utils::numbers(1)
            .map_err(|e| ErrorInternalServerError(e))
            .zip(
                // Buffer into 5Mb chunks, AWS doesn't allow parts smaller than 5Mb
                body_stream
                    // TODO optimize this
                    .map(|b| stream::iter_ok(b.to_vec()))
                    .flatten()
                    .chunks(5 * 1024 * 1024),
            )
            .map_err(|_| {
                UploadPartError::Unknown("Something went wrong with HttpRequest stream".to_owned())
            })
            .fold(
                vec![],
                move |mut parts,
                      (part_number, data)|
                      -> Box<
                          future::Future<Item=Vec<CompletedPart>, Error=UploadPartError>,
                      > {
                    Box::new(
                        state
                            .s3
                            .upload_part(&UploadPartRequest {
                                bucket: bucket.to_owned(),
                                key: key.to_owned(),
                                upload_id: upload_id.to_owned(),
                                part_number: part_number.to_owned(),
                                body: Some(data),
                                ..UploadPartRequest::default()
                            })
                            .map(move |output| {
                                parts.push(CompletedPart {
                                    e_tag: output.e_tag,
                                    part_number: Some(part_number),
                                });

                                parts
                            }),
                    )
                },
            ),
    )
}

fn complete_upload(
    env: &AppEnv,
    upload: &CreateMultipartUploadOutput,
    parts: Vec<CompletedPart>,
) -> Box<Future<Item=CompleteMultipartUploadOutput, Error=CompleteMultipartUploadError>> {
    Box::new(
        env.s3
            .complete_multipart_upload(&CompleteMultipartUploadRequest {
                bucket: upload.bucket.to_owned().unwrap(),
                key: upload.key.to_owned().unwrap(),
                multipart_upload: Some(CompletedMultipartUpload { parts: Some(parts) }),
                request_payer: None,
                upload_id: upload.upload_id.to_owned().unwrap(),
            }),
    )
}

fn abort_upload(
    env: &AppEnv,
    upload: &CreateMultipartUploadOutput,
) -> Box<Future<Item=AbortMultipartUploadOutput, Error=AbortMultipartUploadError>> {
    Box::new(env.s3.abort_multipart_upload(&AbortMultipartUploadRequest {
        bucket: upload.bucket.to_owned().unwrap(),
        key: upload.key.to_owned().unwrap(),
        request_payer: None,
        upload_id: upload.upload_id.to_owned().unwrap(),
    }))
}

pub fn put_object(req: &HttpRequest<AppEnv>) -> Box<Future<Item=HttpResponse, Error=Error>> {
    let bucket = extract_bucket(&req);
    let key = extract_object_key(&req);

    let state = req.state().clone();
    // select headers
    let cache_control = req.headers().get(header::CACHE_CONTROL).and_then(header_string);
    let content_disposition = req.headers().get(header::CONTENT_DISPOSITION).and_then(header_string);
    let content_encoding = req.headers().get(header::CONTENT_ENCODING).and_then(header_string);
    let content_language = req.headers().get(header::CONTENT_LANGUAGE).and_then(header_string);
    let content_type = req.headers().get(header::CONTENT_TYPE).and_then(header_string);
    let expires = req.headers().get(header::EXPIRES).and_then(header_string);

    // TODO optimize upload - check request size then decide which upload method to
    // use (multipart_upload vs put_object)

    let body_stream: Box<Stream<Item=Bytes, Error=Error>> = Box::new(
        req.payload()
            .map_err(|_e| ErrorInternalServerError("Something went wrong while reading request stream"))
    );

    return Box::new(
        state
            .s3
            .create_multipart_upload(&CreateMultipartUploadRequest {
                bucket: bucket.to_owned(),
                key: key.to_owned(),
                cache_control: cache_control.to_owned(),
                content_disposition: content_disposition.to_owned(),
                content_encoding: content_encoding.to_owned(),
                content_language: content_language.to_owned(),
                content_type: content_type.to_owned(),
                expires: expires.to_owned(),
                ..CreateMultipartUploadRequest::default()
            })
            .map_err(|e| match e {
                CreateMultipartUploadError::HttpDispatch(e) => ErrorInternalServerError(e),
                CreateMultipartUploadError::Credentials(e) => ErrorForbidden(e),
                CreateMultipartUploadError::Validation(e) => ErrorBadRequest(e),
                CreateMultipartUploadError::Unknown(e) => ErrorInternalServerError(e),
            })
            .and_then(move |upload| {
                upload_parts(body_stream, state.to_owned(), &upload).then(move |parts_r| match parts_r {
                    Ok(parts) => {
                        let c: Box<Future<Item=HttpResponse, Error=Error>>;

                        if parts.is_empty() {
                            // no parts upload - file is empty
                            c = Box::new(
                                abort_upload(&state, &upload)
                                    .then(move |_| {
                                        state
                                            .s3
                                            .put_object(&PutObjectRequest {
                                                bucket: bucket,
                                                key: key,
                                                body: Some(vec![]),
                                                cache_control: cache_control.to_owned(),
                                                content_disposition: content_disposition
                                                    .to_owned(),
                                                content_encoding: content_encoding.to_owned(),
                                                content_language: content_language.to_owned(),
                                                content_type: content_type.to_owned(),
                                                expires: expires.to_owned(),
                                                ..PutObjectRequest::default()
                                            })
                                            .map_err(|e| match e {
                                                PutObjectError::HttpDispatch(e) => {
                                                    ErrorInternalServerError(e)
                                                }
                                                PutObjectError::Credentials(e) => {
                                                    ErrorForbidden(e)
                                                }
                                                PutObjectError::Validation(e) => {
                                                    ErrorBadRequest(e)
                                                }
                                                PutObjectError::Unknown(e) => {
                                                    ErrorInternalServerError(e)
                                                }
                                            })
                                    })
                                    .map(|_| HttpResponse::Ok().finish()),
                            );
                        } else {
                            c = Box::new(
                                complete_upload(&state, &upload, parts)
                                    .map_err(|e| match e {
                                        CompleteMultipartUploadError::HttpDispatch(e) => {
                                            ErrorInternalServerError(e)
                                        }
                                        CompleteMultipartUploadError::Credentials(e) => {
                                            ErrorForbidden(e)
                                        }
                                        CompleteMultipartUploadError::Validation(e) => {
                                            ErrorBadRequest(e)
                                        }
                                        CompleteMultipartUploadError::Unknown(e) => {
                                            ErrorInternalServerError(e)
                                        }
                                    })
                                    .map(|_| HttpResponse::Ok().finish()),
                            );
                        }

                        c
                    }
                    Err(e) => {
                        let c: Box<Future<Item=HttpResponse, Error=Error>> = Box::new(
                            abort_upload(&state, &upload)
                                .map(|_| HttpResponse::Ok().finish())
                                .then(|_| {
                                    Err(match e {
                                        UploadPartError::HttpDispatch(e) => {
                                            ErrorInternalServerError(e)
                                        }
                                        UploadPartError::Credentials(e) => ErrorForbidden(e),
                                        UploadPartError::Validation(e) => ErrorBadRequest(e),
                                        UploadPartError::Unknown(e) => ErrorInternalServerError(e),
                                    })
                                }),
                        );

                        c
                    }
                })
            })
            .map(|_| HttpResponse::Ok().finish()),
    );
}

pub fn delete_object(req: &HttpRequest<AppEnv>) -> impl Responder {
    req.state()
        .s3
        .delete_object(&DeleteObjectRequest {
            bucket: extract_bucket(&req),
            key: extract_object_key(&req),
            ..DeleteObjectRequest::default()
        })
        .map_err(|e| match e {
            // http://rusoto.github.io/rusoto/rusoto_s3/enum.DeleteObjectError.html
            DeleteObjectError::HttpDispatch(e) => ErrorInternalServerError(e),
            DeleteObjectError::Credentials(e) => ErrorForbidden(e),
            DeleteObjectError::Validation(e) => ErrorBadRequest(e),
            DeleteObjectError::Unknown(e) => ErrorInternalServerError(e),
        })
        .map(|_| HttpResponse::Ok().finish())
        .responder()
}

enum DestinationHeaderError {
    Missing,
    Invalid,
}

fn extract_destination_header(
    req: &HttpRequest<AppEnv>,
) -> Result<String, DestinationHeaderError> {
    match req.headers().get("destination") {
        Some(destination) => match destination.to_str() {
            Ok(dest) => Ok(dest.trim_left_matches("/").to_owned()),
            Err(_) => Err(DestinationHeaderError::Invalid),
        },
        None => Err(DestinationHeaderError::Missing),
    }
}

pub fn copy_object(req: &HttpRequest<AppEnv>) -> Box<Future<Item=HttpResponse, Error=Error>> {
    let state = req.state().clone();
    let bucket = extract_bucket(&req);
    let source_key = extract_object_key(&req);

    match extract_destination_header(req) {
        Ok(dest) => {
            state
                .s3
                .copy_object(&CopyObjectRequest {
                    bucket: bucket.clone(),
                    copy_source: util::encode_key(format!("{}/{}", bucket, source_key)),
                    key: dest,
                    ..CopyObjectRequest::default()
                })
                .map_err(|e| match e {
                    // http://rusoto.github.io/rusoto/rusoto_s3/enum.CopyObjectError.html
                    CopyObjectError::HttpDispatch(e) => ErrorInternalServerError(e),
                    CopyObjectError::Credentials(e) => ErrorForbidden(e),
                    CopyObjectError::Validation(e) => ErrorBadRequest(e),
                    CopyObjectError::ObjectNotInActiveTierError(e) => ErrorForbidden(e),
                    CopyObjectError::Unknown(e) => ErrorInternalServerError(e),
                })
                .map(|_| HttpResponse::Ok().finish())
                .responder()
        }
        Err(_) => Box::new(future::err(ErrorBadRequest("Invalid Destination header"))),
    }
}

pub fn move_object(req: &HttpRequest<AppEnv>) -> impl Responder {
    let state = req.state().clone();
    let bucket = extract_bucket(&req);
    let source_key = extract_object_key(&req);

    copy_object(req)
        .and_then(move |_| {
            state
                .s3
                .delete_object(&DeleteObjectRequest {
                    bucket: bucket,
                    key: source_key,
                    ..DeleteObjectRequest::default()
                })
                .map_err(|e| match e {
                    // http://rusoto.github.io/rusoto/rusoto_s3/enum.DeleteObjectError.html
                    DeleteObjectError::HttpDispatch(e) => ErrorInternalServerError(e),
                    DeleteObjectError::Credentials(e) => ErrorForbidden(e),
                    DeleteObjectError::Validation(e) => ErrorBadRequest(e),
                    DeleteObjectError::Unknown(e) => ErrorInternalServerError(e),
                })
                .map(|_| HttpResponse::Ok().finish())
        })
        .responder()
}
