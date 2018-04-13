AWS S3 WebDAV
=================

Basic AWS S3 WebDAV interface implemented in [Rust](https://www.rust-lang.org/).

## Supported methods

### `GET`

Returns object contents:

```
curl -X GET http://localhost:8080/hello.txt
```

### `HEAD`

Check object exists without fetching the body:

```
curl -X HEAD http://localhost:8080/hello.txt
```

### `PUT`

Create or Update object:

```
curl -X PUT http://localhost:8080/hello.txt \
  -d 'Hello there!'
```

or from file:

```
curl -X PUT http://localhost:8080/hello.txt \
  --upload-file ./hello.txt
```

### `DELETE`

Delete object:

```
curl -X DELETE http://localhost:8080/hello.txt 
```

### `COPY`

Copy object within same bucket:

```
curl -X COPY http://localhost:8080/hello.txt \
  -H 'Destination: /hello2.txt'
```

### `MOVE`

Move object within same bucket:

```
curl -X MOVE http://localhost:8080/hello.txt \
  -H 'Destination: /hello2.txt'
```

## Configuration

Running application requires few configuration options.

### AWS Region (`required`)

AWS service region ID can be provided using `--aws-region` argument or `AWS_REGION` environment variable.

Check Complete regions list at [https://docs.aws.amazon.com/general/latest/gr/rande.html#s3_region](https://docs.aws.amazon.com/general/latest/gr/rande.html#s3_region)


### Bucket Name (`required`)

AWS S3 bucket name can be provided using `--aws-bucket` argument or `AWS_BUCKET` environment variable.

### Key Prefix (`optional`)

A prefix to be used with key, will be automatically added before requested service path,
Can be provided usign `--aws-key-prefix` argument or `AWS_KEY_PREFIX` environment variable.

For example with `--aws-key-prefix=folder1/`, path `http://<aws-webdav-host:port>/path/to/my/file.txt` will be translated to
`folder1/path/to/my/file.txt` s3 object key.

### AWS Credentials (`optional`)

If S3 bucket requires authorization, credentials may be provided via:

#### Environment variables

  * `AWS_ACCESS_KEY_ID` for access key
  * `AWS_SECRET_ACCESS_KEY` for secret

#### Credentials File

AWS Credentials file may be used at default location (`~/.aws/credentials`) or custom location 
configured via `AWS_SHARED_CREDENTIALS_FILE` environment variable.

Profile may be optionally configured using `AWS_PROFILE` environment variable. `default` profile is used by default.


#### IAM instance profile

Will only work if running on an EC2 instance with an instance profile/role.

_(May not work with docker image, plase let me know if it does)._


## Running with Docker

There is an automated docker build configured for this repo: [lostintime/aws-s3-webdav](https://hub.docker.com/r/lostintime/aws-s3-webdav/).

### Run service container

```
docker run --read-only \
    -p 127.0.0.1:8080:8080 \
    -v ~/.aws/credentials:/.aws/credentials \
    -e "AWS_SHARED_CREDENTIALS_FILE=/.aws/credentials" \
    -e "AWS_PROFILE=my_custom_profile" \
    -e "AWS_REGION=eu-central-1" \
    -e "AWS_BUCKET=my-bucket" \
    -e "AWS_KEY_PREFIX=tmp/" \
    lostintime/aws-s3-webdav:0.1.0
```


## Limitations

### HTTPS

Application now uses `http` protocol for communication with AWS endpoint, ssl sessions seems to leak some memory when using `https`, and I didn't figure out how to fix it yet.

### Multipart Uploads

To upload files application uses [AWS Mulipart Upload](https://docs.aws.amazon.com/AmazonS3/latest/dev/mpuoverview.html), which in case of failures in the middle of the upload will leave parts stored in S3, and you will be
charged for patrs uploaded, so it's recommended to configure [Bucket Lifecycle Policy](https://docs.aws.amazon.com/AmazonS3/latest/dev/mpuoverview.html#mpu-abort-incomplete-mpu-lifecycle-config).


## License

Licensed under the Apache License, Version 2.0.
