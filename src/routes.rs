use actix_web::{
  AsyncResponder, Error, HttpRequest, HttpResponse,
  error::ErrorInternalServerError, error::ErrorNotFound, error::ErrorForbidden,
  error::ErrorBadRequest
};
use rusoto_s3::*;
use futures::{Future, Stream, future};
use bytes::Bytes;
use env::*;
use rocket_aws_s3_proxy::stream_utils;
use std::sync::Arc;

/// Alias for application environment, shared between handlers
type AppEnv = Arc<AppState>;

fn extract_bucket(req: &HttpRequest<AppEnv>) -> String {
  req.state().config.s3.bucket.as_str().to_owned()
}

fn extract_object_key(req: &HttpRequest<AppEnv>) -> String {
  match req.state().config.s3.prefix {
    Some(ref prefix) => {
      let mut s: String = prefix.into();

      s.as_str()
        .trim_right_matches("/").to_owned()
        .push('/');

      s.push_str(req.path().trim_left_matches("/"));

      s
    },
    None => req.path().trim_left_matches("/").to_owned()
  }
}

/// Get object from bucket
pub fn get_object(req: HttpRequest<AppEnv>) -> Box<Future<Item = HttpResponse, Error = Error>> {
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
      Some(body) => {
        HttpResponse::Ok().streaming(
          Box::new(
            body.map_err(|_e| ErrorInternalServerError("Something went wrong with body stream"))
              .map(Bytes::from)
          )
        )
      },
      None => HttpResponse::from_error(ErrorNotFound("Object Not Found")),
    })
    .responder()
}

/// HEAD object from bucket
pub fn head_object(req: HttpRequest<AppEnv>) -> Box<Future<Item = HttpResponse, Error = Error>> {
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
    .map(|_| HttpResponse::Ok().finish())
    .responder()
}

fn create_upload(env: AppEnv, bucket: String, key: String) -> Box<Future<Item=CreateMultipartUploadOutput, Error=CreateMultipartUploadError>> {
  Box::new(
    env.s3
      .create_multipart_upload(&CreateMultipartUploadRequest {
        bucket: bucket,
        key: key,
        ..CreateMultipartUploadRequest::default()
      })
  )
}

fn upload_parts(req: HttpRequest<AppEnv>, upload: &CreateMultipartUploadOutput) -> Box<Future<Item=Vec<CompletedPart>, Error=UploadPartError>> {
  let state = req.state().clone();

  let bucket: String = upload.bucket.to_owned().unwrap();
  let key: String = upload.key.to_owned().unwrap();
  let upload_id: String = upload.upload_id.to_owned().unwrap();

  Box::new(
    stream_utils::numbers(1)
      .map_err(|e| ErrorInternalServerError(e))
      .zip(
        req
          .map_err(|_e| ErrorInternalServerError("Something went wrong while reading request stream"))
      )
      .map(|(n, b)| -> (i64, Vec<u8>) {
        (n, b.to_vec())
      })
      .map_err(|_| UploadPartError::Unknown("Something went wrong with HttpRequest stream".to_owned()))
      .fold(vec![], move |mut parts, (part_number, data)| -> Box<future::Future<Item=Vec<CompletedPart>, Error=UploadPartError>> {
        Box::new(
          state.s3.upload_part(&UploadPartRequest {
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
              part_number: Some(part_number)
            });

            parts
          })
        )
      })
  )
}

fn complete_upload(env: AppEnv, upload: &CreateMultipartUploadOutput, parts: Vec<CompletedPart>) -> Box<Future<Item=CompleteMultipartUploadOutput, Error=CompleteMultipartUploadError>> {
  Box::new(env.s3
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

fn abort_upload(env: AppEnv, upload: &CreateMultipartUploadOutput) -> Box<Future<Item = AbortMultipartUploadOutput, Error =AbortMultipartUploadError>> {
  Box::new(env.s3
    .abort_multipart_upload(&AbortMultipartUploadRequest {
      bucket: upload.bucket.to_owned().unwrap(),
      key: upload.key.to_owned().unwrap(),
      request_payer: None,
      upload_id: upload.upload_id.to_owned().unwrap(),
    })
  )
}

pub fn put_object(req: HttpRequest<AppEnv>) -> Box<Future<Item = HttpResponse, Error = Error>> {
  let bucket = extract_bucket(&req);
  let key = extract_object_key(&req);

  let state = req.state().clone();
  
  let f1: Box<Future<Item=HttpResponse, Error=Error>> = Box::new(
    create_upload(req.state().clone(), bucket, key)
      .map_err(|e| match e {
        CreateMultipartUploadError::HttpDispatch(e) => ErrorInternalServerError(e),
        CreateMultipartUploadError::Credentials(e) => ErrorForbidden(e),
        CreateMultipartUploadError::Validation(e) => ErrorBadRequest(e),
        CreateMultipartUploadError::Unknown(e) => ErrorInternalServerError(e),
      })
      .and_then(move |upload| {
        upload_parts(req, &upload)
          .then(move |parts_r| match parts_r {
            Ok(parts) => {
              let c: Box<Future<Item = HttpResponse, Error = Error>> = Box::new(
                complete_upload(state, &upload, parts)
                  .map(|_| HttpResponse::Ok().finish())
                  .map_err(|e| match e {
                    CompleteMultipartUploadError::HttpDispatch(e) => ErrorInternalServerError(e),
                    CompleteMultipartUploadError::Credentials(e) => ErrorForbidden(e),
                    CompleteMultipartUploadError::Validation(e) => ErrorBadRequest(e),
                    CompleteMultipartUploadError::Unknown(e) => ErrorInternalServerError(e),
                  })
              );

              c
            },
            Err(e) => {
              let c: Box<Future<Item = HttpResponse, Error = Error>> = Box::new(
                abort_upload(state, &upload)
                  .map(|_| HttpResponse::Ok().finish())
                  .then(|_|
                    Err(match e {
                      UploadPartError::HttpDispatch(e) => ErrorInternalServerError(e),
                      UploadPartError::Credentials(e) => ErrorForbidden(e),
                      UploadPartError::Validation(e) => ErrorBadRequest(e),
                      UploadPartError::Unknown(e) => ErrorInternalServerError(e),
                    })
                  )
              );

              c
            }
          })
      })
      .map(|_| HttpResponse::Ok().finish())
  );

  f1
}

pub fn delete_object(req: HttpRequest<AppEnv>) -> Box<Future<Item = HttpResponse, Error = Error>> {
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

enum DestinationHeaderError {
  Missing,
  Invalid,
}

fn extract_destination_header(req: &mut HttpRequest<AppEnv>) -> Result<String, DestinationHeaderError> {
  match req.headers_mut().get("destination") {
    Some(destination) => match destination.to_str() {
      Ok(dest) => Ok(dest.trim_left_matches("/").to_owned()),
      Err(_) => Err(DestinationHeaderError::Invalid)
    },
    None => Err(DestinationHeaderError::Missing)
  }
}

pub fn copy_object(mut req: HttpRequest<AppEnv>) -> Box<Future<Item = HttpResponse, Error = Error>> {
  let state = req.state().clone();
  let bucket = extract_bucket(&req);
  let source_key = extract_object_key(&req);

  match extract_destination_header(&mut req) {
    Ok(dest) => {
      state.s3
        .copy_object(&CopyObjectRequest {
            bucket: bucket.clone(),
            copy_source: format!("{}/{}", bucket, source_key),
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
    },
    Err(_) => Box::new(
      future::err(ErrorBadRequest("Invalid Destination header"))
    )      
  }
}

pub fn move_object(req: HttpRequest<AppEnv>) -> Box<Future<Item = HttpResponse, Error = Error>> {
  let state = req.state().clone();
  let bucket = extract_bucket(&req);
  let source_key = extract_object_key(&req);

  copy_object(req)
    .and_then(move |_| {
      state.s3
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
