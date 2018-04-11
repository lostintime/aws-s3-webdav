use actix_web::{
  AsyncResponder, Body, Error, HttpRequest, HttpResponse, HttpMessage, http::StatusCode,
  error::ErrorInternalServerError, error::ErrorNotFound, error::ErrorForbidden,
  error::ErrorBadRequest, error::PayloadError
};
use rusoto_s3::*;
use futures::{Future, Stream, future, stream };
use bytes::Bytes;
use env::*;
use rusoto_credential;
use rusoto_core;
use rocket_aws_s3_proxy::stream_utils;

fn extract_bucket(req: &HttpRequest<AppState>) -> String {
  req.state().config.s3.bucket.as_str().to_owned()
}

fn extract_object_key(req: &HttpRequest<AppState>) -> String {
  req.path().trim_left_matches("/").to_owned()
}

/// Get object from bucket
pub fn get_object(req: HttpRequest<AppState>) -> Box<Future<Item = HttpResponse, Error = Error>> {
   req.state().s3
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
          Some(body) => HttpResponse::Ok().streaming(
              Box::new(
                body.map_err(|_e| ErrorInternalServerError("Something went wrong with body stream"))
                  .map(Bytes::from)
              )
            ),
          None => HttpResponse::from_error(ErrorNotFound("Object Not Found")),
       })
       .responder()
}

/// HEAD object from bucket
pub fn head_object(req: HttpRequest<AppState>) -> Box<Future<Item = HttpResponse, Error = Error>> {
   req.state().s3
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
       .map(|_r| HttpResponse::Ok().finish())
       .responder()
}


fn parts_stream(req: HttpRequest<AppState>) -> (Box<Stream<Item=(i64, Vec<u8>), Error=Error>>, HttpRequest<AppState>) {
  
  let z: Box<Stream<Item=(i64, Vec<u8>), Error=Error>> = Box::new(
    stream_utils::numbers(1)
      .map_err(|e| ErrorInternalServerError(e))
      .zip(
        req.to_owned() // FIXME - avoid request copy!!!
          .map_err(|e| ErrorInternalServerError("Something went wrong while reading request stream"))
      )
      .map(|(n, b)| -> (i64, Vec<u8>) {
        (n, b.to_vec())
      })
  );

  // (Box::new(stream::once(Ok((1, vec![])))), req)
  let mut data: Vec<u8> = Vec::new();
  data.extend_from_slice("Hello world".as_bytes());

  // (Box::new(stream::once(Ok((1, data)))), req)
  (z, req)
}

fn create_upload(req: &HttpRequest<AppState>, bucket: String, key: String) -> Box<Future<Item=CreateMultipartUploadOutput, Error=CreateMultipartUploadError>> {
  Box::new(
    req.state().s3
      .create_multipart_upload(&CreateMultipartUploadRequest {
        bucket: bucket,
        key: key,
        ..CreateMultipartUploadRequest::default()
      })
  )
}

fn upload_parts(req: HttpRequest<AppState>, upload: &CreateMultipartUploadOutput) -> Box<Future<Item=(HttpRequest<AppState>, Vec<CompletedPart>), Error=UploadPartError>> {
  let (stream, req) = parts_stream(req);

  let bucket: String = upload.bucket.to_owned().unwrap();
  let key: String = upload.key.to_owned().unwrap();
  let upload_id: String = upload.upload_id.to_owned().unwrap();

  Box::new(
    stream
      .map_err(|_| UploadPartError::Unknown("Something went wrong".to_owned()))
      .fold((req, vec![]), move |(req, mut parts), (part_number, data)| -> Box<future::Future<Item=(HttpRequest<AppState>, Vec<CompletedPart>), Error=UploadPartError>> {
        let nr = part_number.to_owned();
        Box::new(
          req.state().s3.upload_part(&UploadPartRequest {
            bucket: bucket.to_owned(),
            key: key.to_owned(),
            upload_id: upload_id.to_owned(),
            part_number: nr,
            body: Some(data),
            ..UploadPartRequest::default()
          })
          .map(move |output| {
            parts.push(CompletedPart {
              e_tag: output.e_tag,
              part_number: Some(nr)
            });

            (req, parts)
          })
        )
      })
  )
}

fn complete_upload(req: &HttpRequest<AppState>, upload: &CreateMultipartUploadOutput, parts: Vec<CompletedPart>) -> Box<Future<Item=CompleteMultipartUploadOutput, Error=CompleteMultipartUploadError>> {
  Box::new(req.state().s3
    .complete_multipart_upload(&CompleteMultipartUploadRequest {
      bucket: upload.bucket.to_owned().unwrap(),
      key: upload.key.to_owned().unwrap(),
      multipart_upload: Some(CompletedMultipartUpload {
        parts: Some(parts)
      }),
      request_payer: None,
      upload_id: upload.upload_id.to_owned().unwrap(),
    })
  )
}

pub fn put_object(req: HttpRequest<AppState>) -> Box<Future<Item = HttpResponse, Error = Error>> {
  let bucket = extract_bucket(&req);
  let key = extract_object_key(&req);
  
  let f1: Box<Future<Item=HttpResponse, Error=Error>> = Box::new(
    create_upload(&req, bucket, key)
      .map_err(|e| match e {
        CreateMultipartUploadError::HttpDispatch(e) => ErrorInternalServerError(e),
        CreateMultipartUploadError::Credentials(e) => ErrorForbidden(e),
        CreateMultipartUploadError::Validation(e) => ErrorBadRequest(e),
        CreateMultipartUploadError::Unknown(e) => ErrorInternalServerError(e),
      })
      .and_then(move |upload| {
        upload_parts(req, &upload)
          .map_err(|e| match e {
            UploadPartError::HttpDispatch(e) => ErrorInternalServerError(e),
            UploadPartError::Credentials(e) => ErrorForbidden(e),
            UploadPartError::Validation(e) => ErrorBadRequest(e),
            UploadPartError::Unknown(e) => ErrorInternalServerError(e),
          })
          .and_then(move |(req, parts)| {
            println!("Complete upload! {:?}", parts);
            complete_upload(&req, &upload, parts)
              .map_err(|e| match e {
                CompleteMultipartUploadError::HttpDispatch(e) => ErrorInternalServerError(e),
                CompleteMultipartUploadError::Credentials(e) => ErrorForbidden(e),
                CompleteMultipartUploadError::Validation(e) => ErrorBadRequest(e),
                CompleteMultipartUploadError::Unknown(e) => ErrorInternalServerError(e),
              })
          })
      })
      .map(|_| HttpResponse::Ok().finish())
  );

  f1
}

pub fn delete_object(req: HttpRequest<AppState>) -> Box<Future<Item = HttpResponse, Error = Error>> {
  req.state().s3
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

pub fn copy_object(req: HttpRequest<AppState>) -> Box<Future<Item = HttpResponse, Error = Error>> {
  future::ok(HttpResponse::Ok().finish())
  .responder()
}

pub fn move_object(req: HttpRequest<AppState>) -> Box<Future<Item = HttpResponse, Error = Error>> {
  future::ok(HttpResponse::Ok().finish())
  .responder()
}

