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

fn extract_bucket(req: &HttpRequest<AppState>) -> String {
  String::from(req.state().config.s3.bucket.as_str())
}

fn extract_object_key(req: &HttpRequest<AppState>) -> String {
  String::from(req.path().trim_left_matches("/"))
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
  (Box::new(stream::empty()), req)
}

fn upload_part(s3: Box<S3Client<rusoto_credential::ProfileProvider, rusoto_core::reactor::RequestDispatcher>>, 
               upload: &CreateMultipartUploadOutput,
               part_number: i64,
               body: Option<Vec<u8>>) -> Box<Future<Item=UploadPartOutput, Error=UploadPartError>> {
  match upload.bucket {
    Some(ref bucket) => match upload.key {
      Some(ref key) => match upload.upload_id {
        Some(ref upload_id) => {
          Box::new(
            s3.upload_part(&UploadPartRequest {
              bucket: bucket.clone(),
              key: key.clone(),
              upload_id: upload_id.clone(),
              part_number: part_number,
              body: body,
              ..UploadPartRequest::default()
            }))
        },
        None => Box::new(future::err(UploadPartError::Unknown(String::from("Got no upload_id value from CreateMultipartUploadOutput"))))
      },
      None => Box::new(future::err(UploadPartError::Unknown(String::from("Got no key value from CreateMultipartUploadOutput"))))
    },
    None => Box::new(future::err(UploadPartError::Unknown(String::from("Got no bucket value from CreateMultipartUploadOutput"))))
  }
}

pub fn upload_stream(upload: &CreateMultipartUploadOutput,
                     stream: Box<Stream<Item=(i64, Vec<u8>), Error=Error>>) -> Box<Future<Item = i64, Error = Error>> {
  Box::new(
    stream
      .fold(0i64, |acc, t| -> future::FutureResult<i64, Error> {
        future::ok(t.0) 
      })
  )
}

fn create_upload(req: HttpRequest<AppState>, bucket: String, key: String) -> Box<Future<Item=(CreateMultipartUploadOutput, HttpRequest<AppState>), Error=CreateMultipartUploadError>> {
  Box::new(
    req.state().s3
      .create_multipart_upload(&CreateMultipartUploadRequest {
        bucket: bucket,
        key: key,
        ..CreateMultipartUploadRequest::default()
      })
      .map(|output| (output, req))
  )
}

fn upload_parts(req: HttpRequest<AppState>, upload: CreateMultipartUploadOutput) -> Box<Future<Item=(CreateMultipartUploadOutput, HttpRequest<AppState>), Error=UploadPartError>> {
  let (stream, req) = parts_stream(req);
  
  let bucket: String = upload.bucket.clone().unwrap();
  let key: String = upload.key.clone().unwrap();
  let upload_id: String = upload.upload_id.clone().unwrap();

  Box::new(
    req.state().s3.upload_part(&UploadPartRequest {
      bucket: bucket,
      key: key,
      upload_id: upload_id,
      part_number: 1,
      body: Some(vec![]),
      ..UploadPartRequest::default()
    })
    .map(|_| (upload, req))
  )
}

fn complete_upload(req: HttpRequest<AppState>, upload: CreateMultipartUploadOutput) -> Box<Future<Item=(CompleteMultipartUploadOutput, HttpRequest<AppState>), Error=CompleteMultipartUploadError>> {
  Box::new(req.state().s3
    .complete_multipart_upload(&CompleteMultipartUploadRequest {
      bucket: upload.bucket.unwrap(),
      key: upload.key.unwrap(),
      upload_id: upload.upload_id.unwrap(),
      ..CompleteMultipartUploadRequest::default()
    })
    .map(|output| (output, req))
  )
}

pub fn put_object(req: HttpRequest<AppState>) -> Box<Future<Item = HttpResponse, Error = Error>> {
  let bucket = extract_bucket(&req);
  let key = extract_object_key(&req);
  
  let f1: Box<Future<Item=HttpResponse, Error=Error>> = Box::new(
    create_upload(req, bucket, key)
      .map_err(|e| match e {
        CreateMultipartUploadError::HttpDispatch(e) => ErrorInternalServerError(e),
        CreateMultipartUploadError::Credentials(e) => ErrorForbidden(e),
        CreateMultipartUploadError::Validation(e) => ErrorBadRequest(e),
        CreateMultipartUploadError::Unknown(e) => ErrorInternalServerError(e),
      })
      .and_then(|(upload, req)| {
        upload_parts(req, upload)
          .map_err(|e| match e {
            UploadPartError::HttpDispatch(e) => ErrorInternalServerError(e),
            UploadPartError::Credentials(e) => ErrorForbidden(e),
            UploadPartError::Validation(e) => ErrorBadRequest(e),
            UploadPartError::Unknown(e) => ErrorInternalServerError(e),
          })
      })
      .and_then(|(upload, req)| {
        println!("Complete upload!");
        complete_upload(req, upload)
          .map_err(|e| match e {
            CompleteMultipartUploadError::HttpDispatch(e) => ErrorInternalServerError(e),
            CompleteMultipartUploadError::Credentials(e) => ErrorForbidden(e),
            CompleteMultipartUploadError::Validation(e) => ErrorBadRequest(e),
            CompleteMultipartUploadError::Unknown(e) => ErrorInternalServerError(e),
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

